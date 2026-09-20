# Synthetic recovery fixtures

The 19 `.ksd` files in this directory are synthetic test inputs, not personal backups.

- The ten image fixtures contain a small generated drawing: a colored rectangle, a circle, and the word “Unfold.” JPEG variants exercise progressive encoding, padding, and streaming chunk boundaries.
- The nine video fixtures contain generated color bars and a test timer at 128 × 96 pixels.
- The fixtures contain no personal photographs, recordings, GPS data, author names, or camera-owner metadata.

`manifest.json` records the expected decrypted size and SHA-256 hash of each fixture. `npm test` verifies those values and checks that recovery leaves the encrypted originals unchanged.

Only generated test data belongs here. Personal backups, recovered media, and recovery reports must remain outside the repository. The root `.gitignore` excludes `.ksd` files elsewhere while allowing these fixtures.
