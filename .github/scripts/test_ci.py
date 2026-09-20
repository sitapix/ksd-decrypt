import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

from ci import check_versions, documentation_only, select_work


ROOT = Path(__file__).resolve().parents[2]


class SelectionTests(unittest.TestCase):
    def test_only_reviewed_documentation_can_skip_native_checks(self):
        self.assertTrue(documentation_only([
            "README.md", "DESIGN.md", "docs/ci.md",
            "docs/screenshots/ksd-decrypt-light.png", "tests/fixtures/README.md",
        ]))
        for changed in [
            "src/app.ts", "src-tauri/src/recovery.rs", "src-tauri/Cargo.lock",
            "package-lock.json", "build.mjs", "tests/fixtures/sample.jpg.ksd",
            "src-tauri/tauri.conf.json", ".github/workflows/build.yml",
            ".github/scripts/ci.py", ".github/release-notes.md", "unknown-file",
        ]:
            with self.subTest(changed=changed):
                self.assertFalse(documentation_only(["README.md", changed]))
        self.assertFalse(documentation_only([]))

    def test_tags_and_manual_runs_always_check_and_package(self):
        for event, ref in [
            ("push", "refs/tags/v1.0.0"),
            ("push", "refs/tags/v1.0.0-rc.1"),
            ("workflow_dispatch", "refs/heads/main"),
        ]:
            self.assertEqual(select_work(event, ref, {}), (True, True))

    def test_new_or_unavailable_push_base_runs_full_checks(self):
        self.assertEqual(select_work("push", "refs/heads/main", {
            "before": "0" * 40, "after": "b" * 40,
        }), (True, False))

        def missing_base(args):
            raise subprocess.CalledProcessError(128, args)

        self.assertEqual(select_work("push", "refs/heads/main", {
            "before": "a" * 40, "after": "b" * 40,
        }, missing_base), (True, False))

    def test_real_git_diff_includes_runtime_deletions_and_renames(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)

            def git(*args):
                return subprocess.check_output(["git", "-C", directory, *args], stderr=subprocess.DEVNULL)

            git("init", "-b", "main")
            git("config", "user.name", "CI test")
            git("config", "user.email", "ci@example.invalid")
            git("config", "commit.gpgsign", "false")
            git("config", "core.hooksPath", "/dev/null")
            (root / "README.md").write_text("App\n")
            (root / "engine.rs").write_text("fn main() {}\n")
            git("add", ".")
            git("commit", "-m", "Initial")
            base = git("rev-parse", "HEAD").decode().strip()
            (root / "README.md").write_text("App documentation\n")
            git("add", ".")
            git("commit", "-m", "Docs")
            docs = git("rev-parse", "HEAD").decode().strip()

            def run(args):
                return git(*args[1:])

            self.assertEqual(select_work("push", "refs/heads/main", {
                "before": base, "after": docs,
            }, run), (False, False))
            # Renaming runtime code into an ignored documentation path must still run tests.
            git("mv", "engine.rs", "ENGINE.md")
            git("commit", "-m", "Remove engine")
            head = git("rev-parse", "HEAD").decode().strip()
            self.assertEqual(select_work("pull_request", "refs/pull/1/merge", {
                "pull_request": {"base": {"sha": base}, "head": {"sha": head}},
            }, run), (True, False))


class VersionTests(unittest.TestCase):
    def test_current_manifests_agree_and_wrong_tag_is_rejected(self):
        version = check_versions(ROOT)
        self.assertEqual(check_versions(ROOT, f"v{version}"), version)
        for tag in ["v999.0.0", "vnext", "v1.0.0;echo invalid"]:
            with self.subTest(tag=tag), self.assertRaises(ValueError):
                check_versions(ROOT, tag)

    def test_stale_manifest_is_rejected_before_building(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name in ["package.json", "package-lock.json", "src-tauri/tauri.conf.json",
                         "src-tauri/Cargo.toml", "src-tauri/Cargo.lock"]:
                destination = root / name
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(ROOT / name, destination)
            config = root / "src-tauri/tauri.conf.json"
            data = json.loads(config.read_text())
            data["version"] = "0.0.0"
            config.write_text(json.dumps(data))
            with self.assertRaisesRegex(ValueError, "tauri.conf.json"):
                check_versions(root)


if __name__ == "__main__":
    unittest.main()
