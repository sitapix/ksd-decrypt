"""Keep releases consistent and skip native work only for known documentation paths."""

import json
import os
from pathlib import Path
import re
import subprocess
import tomllib


def check_versions(root, tag=None):
    package = json.loads((root / "package.json").read_text())
    npm_lock = json.loads((root / "package-lock.json").read_text())
    tauri = json.loads((root / "src-tauri/tauri.conf.json").read_text())
    cargo = tomllib.loads((root / "src-tauri/Cargo.toml").read_text())
    cargo_lock = tomllib.loads((root / "src-tauri/Cargo.lock").read_text())
    locked_crate = next(p for p in cargo_lock["package"] if p["name"] == package["name"])
    version = package["version"]
    versions = {
        "package-lock.json": npm_lock["version"],
        "package-lock.json root package": npm_lock["packages"][""]["version"],
        "tauri.conf.json": tauri["version"],
        "Cargo.toml": cargo["package"]["version"],
        "Cargo.lock": locked_crate["version"],
    }
    for source, actual in versions.items():
        if actual != version:
            raise ValueError(f"{source} has version {actual}; expected {version}")
    if tag is not None:
        if not re.fullmatch(r"v\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?", tag):
            raise ValueError("Release tags must look like v1.0.0 or v1.0.0-rc.1")
        if tag != f"v{version}":
            raise ValueError(f"Tag {tag} does not match app version v{version}")
    return version


def documentation_only(paths):
    # Unknown files, CI, dependencies, fixtures, and configuration always run native checks.
    return bool(paths) and all(
        ("/" not in name and name.endswith(".md"))
        or (name.startswith("docs/") and name.endswith(".md"))
        or name in {
            "docs/screenshots/ksd-decrypt-light.png",
            "docs/screenshots/ksd-decrypt-dark.png",
            "tests/fixtures/README.md",
        }
        for name in paths
    )


def select_work(event_name, ref, event, git=subprocess.check_output):
    package = event_name == "workflow_dispatch" or ref.startswith("refs/tags/")
    if package:
        return True, True
    if event_name == "pull_request":
        base = event["pull_request"]["base"]["sha"]
        head = event["pull_request"]["head"]["sha"]
        comparison = f"{base}...{head}"
    else:
        base = event.get("before", "")
        head = event.get("after", "")
        if not base or not head or set(base) == {"0"}:
            return True, False
        comparison = f"{base}..{head}"
    try:
        changed = git(["git", "diff", "--name-only", "--no-renames", "-z", comparison, "--"])
    except subprocess.CalledProcessError:
        # Force pushes can make the old commit unavailable. Fail open to full validation.
        return True, False
    paths = changed.decode("utf-8", errors="replace").rstrip("\0").split("\0")
    return not documentation_only(paths), False


def main():
    ref = os.environ.get("GITHUB_REF", "")
    event_name = os.environ.get("GITHUB_EVENT_NAME", "")
    tag = ref.removeprefix("refs/tags/") if ref.startswith("refs/tags/") else None
    version = check_versions(Path.cwd(), tag)
    event = json.loads(Path(os.environ["GITHUB_EVENT_PATH"]).read_text())
    runtime, package = select_work(event_name, ref, event)
    with open(os.environ["GITHUB_OUTPUT"], "a") as output:
        output.write(f"runtime={str(runtime).lower()}\npackage={str(package).lower()}\n")
    print(f"Version {version}: native checks={runtime}, installers={package}")


if __name__ == "__main__":
    main()
