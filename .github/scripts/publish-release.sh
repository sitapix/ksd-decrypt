#!/usr/bin/env bash
set -euo pipefail

# Reruns may repair an incomplete draft, but must never replace a published release.
if release_draft=$(gh release view "$RELEASE_TAG" --json isDraft --jq .isDraft); then
  if [[ "$release_draft" != true ]]; then
    echo "Release $RELEASE_TAG is already published; leaving its downloads unchanged."
    exit 0
  fi
else
  gh release create "$RELEASE_TAG" --verify-tag --draft \
    --title "KSD Decrypt $RELEASE_TAG" --notes-file .github/release-notes.md
fi

gh release upload "$RELEASE_TAG" \
  release-assets/KSD-Decrypt-macOS-universal.dmg \
  release-assets/KSD-Decrypt-macOS-arm64.dmg \
  release-assets/KSD-Decrypt-macOS-x64.dmg \
  release-assets/KSD-Decrypt-Windows-x64-setup.exe \
  release-assets/KSD-Decrypt-Windows-x64-offline-setup.exe \
  release-assets/SHA256SUMS.txt --clobber

# A prerelease never replaces the stable download linked from the README.
if [[ "$RELEASE_TAG" == *-* ]]; then
  gh release edit "$RELEASE_TAG" --draft=false --prerelease --latest=false
else
  gh release edit "$RELEASE_TAG" --draft=false --prerelease=false --latest
fi
