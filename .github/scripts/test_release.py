import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


SCRIPT = Path(__file__).with_name("publish-release.sh").resolve()


class PublishingTests(unittest.TestCase):
    def run_release(self, state="missing", tag="v1.0.0", fail_upload=False):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            fake_gh = root / "gh"
            fake_gh.write_text("""#!/usr/bin/env python3
import json, os, sys
with open(os.environ['COMMAND_LOG'], 'a') as log:
    log.write(json.dumps(sys.argv[1:]) + '\\n')
if sys.argv[1:3] == ['release', 'view']:
    state = os.environ['RELEASE_STATE']
    if state == 'missing':
        sys.exit(1)
    print('true' if state == 'draft' else 'false')
if sys.argv[1:3] == ['release', 'upload'] and os.environ['FAIL_UPLOAD'] == 'true':
    sys.exit(1)
""")
            fake_gh.chmod(0o755)
            log = root / "commands.jsonl"
            environment = {
                **os.environ, "PATH": f"{root}{os.pathsep}{os.environ['PATH']}",
                "COMMAND_LOG": str(log), "RELEASE_STATE": state,
                "RELEASE_TAG": tag, "FAIL_UPLOAD": str(fail_upload).lower(),
            }
            result = subprocess.run(["bash", str(SCRIPT)], env=environment, capture_output=True)
            return result.returncode, [json.loads(line) for line in log.read_text().splitlines()]

    def test_new_release_is_draft_until_all_assets_are_uploaded(self):
        code, commands = self.run_release()
        self.assertEqual(code, 0)
        self.assertEqual([command[1] for command in commands], ["view", "create", "upload", "edit"])
        self.assertIn("--draft", commands[1])
        self.assertIn("--verify-tag", commands[1])
        self.assertEqual(sum(name.startswith("release-assets/") for name in commands[2]), 3)
        self.assertIn("--draft=false", commands[3])
        self.assertIn("--latest", commands[3])

    def test_upload_failure_leaves_release_unpublished(self):
        code, commands = self.run_release(fail_upload=True)
        self.assertNotEqual(code, 0)
        self.assertNotIn("edit", [command[1] for command in commands])

    def test_existing_published_release_is_never_modified(self):
        code, commands = self.run_release(state="published")
        self.assertEqual(code, 0)
        self.assertEqual([command[1] for command in commands], ["view"])

    def test_draft_prerelease_can_resume_without_replacing_latest(self):
        code, commands = self.run_release(state="draft", tag="v1.1.0-rc.1")
        self.assertEqual(code, 0)
        self.assertEqual([command[1] for command in commands], ["view", "upload", "edit"])
        self.assertIn("--prerelease", commands[-1])
        self.assertIn("--latest=false", commands[-1])


if __name__ == "__main__":
    unittest.main()
