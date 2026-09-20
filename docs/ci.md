# CI and releases

The [workflow](../.github/workflows/build.yml) supports the app's two distribution targets: universal macOS (Apple silicon + Intel) and Windows x64. Linux is used only for lightweight coordination and publishing.

| Event | Checks | Installers | GitHub release |
| --- | --- | --- | --- |
| Pull request with app, dependency, fixture, or CI changes | Full Mac + Windows checks | No | No |
| Default-branch push with those changes | Full Mac + Windows checks | No | No |
| Documentation-only PR or default-branch push | Workflow lint, CI helper tests, version consistency | No | No |
| Other branch push | Covered when a PR is opened | No | No |
| Manual workflow run | Full Mac + Windows checks | Both, saved for 14 days | No |
| Matching `v*` tag push | Full Mac + Windows checks | Both, saved for 14 days | Published after both succeed |

Use **Desktop checks** as the required branch-protection check. It reports success for a verified documentation-only change and fails if preparation or either native job fails. There is no workflow-level path filter that could leave a required check pending. Branch protection is a repository setting, separate from this workflow.

The documentation allowlist covers root Markdown files, Markdown under `docs/`, the two reviewed README screenshots, and the synthetic-fixture README. Everything else runs full checks. Unavailable comparison commits also run full checks. Workflow changes deliberately exercise both native jobs because this workflow controls their build and packaging commands.

## Release outputs

- `KSD-Decrypt-macOS-universal.dmg`: contains the app, with both architectures checked using `lipo`.
- `KSD-Decrypt-Windows-x64-setup.exe`: current-user NSIS installer with offline WebView2.
- `SHA256SUMS.txt`: hashes of the two installers.

The release job has `contents: write`; build and PR jobs have read-only repository access. A release stays a draft until both installers have uploaded. Reruns can repair a draft but never overwrite published downloads. Prereleases do not change the README's stable download links. Signing credentials are not configured.

## Efficiency changes

The original workflow already cached npm and Rust dependencies. Its main waste sources were duplicate branch-push/PR builds, full native jobs for documentation edits, and older runs continuing after replacement commits.

1. **Avoid duplicate builds:** feature branch pushes no longer allocate runners; PR validation and post-merge default-branch validation remain. Expected saving: one Mac + Windows check pair per feature-branch update with an open PR.
2. **Skip native work for known documentation changes:** workflow lint, CI helper tests, and version validation still run. Expected saving: one Mac + Windows check pair per documentation-only run.
3. **Cancel stale checks:** a newer update cancels an older run for the same event and branch/PR. Release and manual runs are not cancelled. Savings depend on how much unfinished work the old run had left.

These reduce total runner time. Mac and Windows remain parallel; PR wall-clock improvement depends on queue time and cache hits. Exact minutes require live run history and should not be inferred from YAML alone. Installer artifacts use no extra ZIP compression and expire after 14 days; GitHub release downloads remain available.

## Local validation

```sh
actionlint .github/workflows/build.yml
python3 -m unittest discover -s .github/scripts -p 'test_*.py'
bash -n .github/scripts/publish-release.sh
```

Local tests cover documentation gating, runtime changes mixed with docs, real Git renames/deletions, missing history, forced full tag/manual builds, mismatched release versions, partial upload failures, prereleases, and safe reruns. Live GitHub runs are still needed to verify hosted runners, permissions, caching, installer creation, and cancellation.

Implementation references: [GitHub release links](https://docs.github.com/en/repositories/releasing-projects-on-github/linking-to-releases), [Tauri CI distribution](https://v2.tauri.app/distribute/pipelines/github/), and [actionlint](https://github.com/rhysd/actionlint).
