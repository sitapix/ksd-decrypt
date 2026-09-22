#!/usr/bin/env python3
"""Compare two release recovery CLIs; needs cryptography and Pillow.

Includes process startup, inspection, decryption, hashes, validation, and disk
writes. Verifies outputs outside the timed region. This is a warm-cache local
benchmark, not a measurement of native UI startup or physical disk throughput.
"""

import argparse
import hashlib
import io
import json
import platform
import random
from pathlib import Path
import shutil
import statistics
import struct
import subprocess
import tempfile
import time

from cryptography.hazmat.primitives.ciphers import Cipher, algorithms, modes
from PIL import Image


def sha256(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def positive_int(value):
    number = int(value)
    if number < 1:
        raise argparse.ArgumentTypeError("must be positive")
    return number


def create_workloads(root, large_mib, small_files):
    fixtures = Path(__file__).resolve().parent / "fixtures"
    fixture = (fixtures / "sample.mp4.ksd").read_bytes()
    key = b"keepsafe" * 4
    decrypt = Cipher(algorithms.AES(key), modes.CTR(fixture[3705:3721])).decryptor()
    movie = decrypt.update(fixture[3722:]) + decrypt.finalize()
    padding = large_mib * 1024**2
    prefix = movie + struct.pack(">I4sQ", 1, b"free", padding + 16)
    header = fixture[:3705] + bytes(range(16)) + b"\x02"
    source = root / "Large.mp4.ksd"
    expected = hashlib.sha256()
    encrypt = Cipher(algorithms.AES(key), modes.CTR(header[3705:3721])).encryptor()
    with source.open("wb") as stream:
        stream.write(header)
        expected.update(prefix)
        stream.write(encrypt.update(prefix))
        zeros = bytes(1024**2)
        for _ in range(large_mib):
            expected.update(zeros)
            stream.write(encrypt.update(zeros))
        stream.write(encrypt.finalize())
    large = {
        source: (len(prefix) + padding, expected.hexdigest(), sha256(source))
    }
    manifest = json.loads((fixtures / "manifest.json").read_text())
    photo = next(item for item in manifest if item["name"] == "sample.jpg.ksd")
    original = fixtures / photo["name"]
    original_hash = sha256(original)
    small = {}
    for index in range(small_files):
        source = root / f"Photo-{index:04}.jpg.ksd"
        shutil.copyfile(original, source)
        small[source] = (photo["size"], photo["sha256"], original_hash)
    # Real encoded image data exercises the JPEG scan, unlike trailing padding
    # after an end marker, which the validator deliberately does not scan.
    pixels = random.Random(0).randbytes(4096 * 4096 * 3)
    with Image.frombytes("RGB", (4096, 4096), pixels) as image:
        encoded = io.BytesIO()
        image.save(encoded, format="JPEG", quality=90)
    plaintext = encoded.getvalue()
    jpeg_hash = hashlib.sha256(plaintext).hexdigest()
    jpeg_header = original.read_bytes()[:3722]
    encrypt = Cipher(algorithms.AES(key), modes.CTR(jpeg_header[3705:3721])).encryptor()
    encrypted = jpeg_header + encrypt.update(plaintext) + encrypt.finalize()
    original_hash = hashlib.sha256(encrypted).hexdigest()
    large_jpegs = {}
    for index in range(8):
        source = root / f"Large-photo-{index}.jpg.ksd"
        source.write_bytes(encrypted)
        large_jpegs[source] = (len(plaintext), jpeg_hash, original_hash)
    return {
        "large_mp4": large,
        "small_jpeg_batch": small,
        "large_jpeg_batch": large_jpegs,
    }


def measure(binary, sources, output):
    start = time.perf_counter()
    completed = subprocess.run(
        [str(binary), str(output), *(str(path) for path in sources)],
        capture_output=True,
        text=True,
        check=True,
    )
    elapsed = time.perf_counter() - start
    results = [json.loads(line) for line in completed.stdout.splitlines()]
    if len(results) != len(sources):
        raise RuntimeError("recovery returned the wrong number of results")
    seen = set()
    for result in results:
        source = Path(result["source"])
        if source in seen:
            raise RuntimeError("recovery returned duplicate source results")
        seen.add(source)
        size, recovered_hash, source_hash = sources[source]
        recovered = Path(result["output"])
        if (
            result["bytes"] != size
            or result["recovered_sha256"] != recovered_hash
            or result["source_sha256"] != source_hash
            or recovered.stat().st_size != size
            or sha256(recovered) != recovered_hash
            or sha256(source) != source_hash
        ):
            raise RuntimeError(f"recovery verification failed: {source.name}")
    shutil.rmtree(output)
    return elapsed


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--candidate", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--runs", type=positive_int, default=7)
    parser.add_argument("--large-mib", type=positive_int, default=128)
    parser.add_argument("--small-files", type=positive_int, default=256)
    args = parser.parse_args()
    binaries = {
        "baseline": args.baseline.resolve(),
        "candidate": args.candidate.resolve(),
    }
    report = {
        "platform": platform.platform(),
        "method": "One warmup per binary per workload, then alternating order; "
        "whole CLI process timed; warm filesystem cache; hashes independently "
        "verified after every run; excludes native UI and IPC.",
        "binaries": {
            name: {"bytes": path.stat().st_size, "sha256": sha256(path)}
            for name, path in binaries.items()
        },
        "workloads": {},
    }
    with tempfile.TemporaryDirectory(prefix="ksd-recovery-bench-") as directory:
        root = Path(directory)
        workloads = create_workloads(root, args.large_mib, args.small_files)
        for workload, sources in workloads.items():
            samples = {name: [] for name in binaries}
            for iteration in range(args.runs + 1):
                order = list(binaries)
                if iteration % 2:
                    order.reverse()
                for name in order:
                    seconds = measure(
                        binaries[name], sources, root / f"{workload}-{name}-{iteration}"
                    )
                    if iteration:
                        samples[name].append(seconds)
                    print(f"{workload} {name} run {iteration}: {seconds:.4f}s", flush=True)
            medians = {name: statistics.median(times) for name, times in samples.items()}
            report["workloads"][workload] = {
                "files": len(sources),
                "payload_bytes": sum(item[0] for item in sources.values()),
                "seconds": samples,
                "median_seconds": medians,
                "candidate_time_ratio": medians["candidate"] / medians["baseline"],
                "hashes_match": True,
            }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report["workloads"], indent=2))


if __name__ == "__main__":
    main()
