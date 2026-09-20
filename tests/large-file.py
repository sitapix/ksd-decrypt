#!/usr/bin/env python3
"""Optional >4 GiB integration check. Needs cryptography and about 9 GiB free.

Usage: python3 tests/large-file.py /path/to/release/examples/ksd-decrypt-cli
All generated large files live in a temporary directory and are removed.
"""
import hashlib
import json
from pathlib import Path
import shutil
import struct
import subprocess
import sys
import tempfile
from cryptography.hazmat.primitives.ciphers import Cipher, algorithms, modes

def main():
    binary = str(Path(sys.argv[1]).resolve())
    if shutil.disk_usage(tempfile.gettempdir()).free < 10 * 1024**3:
        raise SystemExit('This optional test needs 10 GiB of free temporary storage.')
    fixture = (Path(__file__).parent / 'fixtures/sample.mp4.ksd').read_bytes()
    key = b'keepsafe' * 4
    decrypt = Cipher(algorithms.AES(key), modes.CTR(fixture[3705:3721])).decryptor()
    movie = decrypt.update(fixture[3722:]) + decrypt.finalize()
    # A real short MP4 plus a standards-compliant large "free" padding box.
    # This tests large byte counts and extended ISO box sizes, not long playback.
    padding = 4 * 1024**3 + 65536
    prefix = movie + struct.pack('>I4sQ', 1, b'free', padding + 16)
    header = fixture[:3705] + bytes(range(16)) + b'\x02'
    expected = hashlib.sha256()
    source_hash = hashlib.sha256(header)
    with tempfile.TemporaryDirectory(prefix='ksd-decrypt-large-') as directory:
        root = Path(directory)
        source = root / 'Large.mp4.ksd'
        encrypt = Cipher(algorithms.AES(key), modes.CTR(header[3705:3721])).encryptor()
        with source.open('wb') as out:
            out.write(header)
            def write(data):
                expected.update(data)
                ciphertext = encrypt.update(data)
                source_hash.update(ciphertext)
                out.write(ciphertext)
            write(prefix)
            zeros = bytes(1024**2)
            remaining = padding
            while remaining:
                part = zeros[:min(remaining, len(zeros))]
                write(part)
                remaining -= len(part)
            out.write(encrypt.finalize())
        print(f'Testing {source.stat().st_size:,} encrypted bytes…', flush=True)
        command = [binary, str(root / 'recovered'), str(source)]
        if sys.platform == 'darwin':
            command = ['/usr/bin/time', '-l', *command]
        run = subprocess.run(command, capture_output=True, text=True, check=True)
        result = json.loads(run.stdout)
        assert result['bytes'] == len(prefix) + padding
        assert result['recovered_sha256'] == expected.hexdigest()
        assert result['source_sha256'] == source_hash.hexdigest()
        recovered = Path(result['output'])
        with recovered.open('rb') as stream:
            assert hashlib.file_digest(stream, 'sha256').hexdigest() == expected.hexdigest()
        with source.open('rb') as stream:
            assert hashlib.file_digest(stream, 'sha256').hexdigest() == source_hash.hexdigest()
        print(json.dumps({'bytes': result['bytes'], 'sha256': expected.hexdigest(), 'source_unchanged': True, 'byte_identical': True}, indent=2))
        print(run.stderr)

if __name__ == '__main__':
    main()
