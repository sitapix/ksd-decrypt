KSD Decrypt v1.0.1 reduces application size and recovery overhead and adds the updated purple vault and gold key icon.

- Smaller Rust release builds with unused commands removed and speed optimization retained.
- Recovery reuses the first decrypted chunk, allocates smaller buffers for small files, and scans JPEG data more efficiently. Apple silicon builds use hardware-accelerated SHA-256 when available.
- Separate Apple silicon and Intel Mac downloads, alongside the universal build.
- A small standard Windows installer that downloads WebView2 when needed, plus an offline installer with WebView2 included.

Recovery validation, source preservation, cancellation cleanup, and protection against overwriting outputs remain in place. Local verification included 23 Rust tests, 19 independently encrypted media fixtures, and byte-for-byte recovery of a file larger than 4 GiB. Browser UI tests use mocked native commands; native installer and desktop UI verification remain incomplete.

Download the installer for your computer from **Assets** below:

- **Mac (Apple silicon):** `KSD-Decrypt-macOS-arm64.dmg`.
- **Mac (Intel):** `KSD-Decrypt-macOS-x64.dmg`.
- **Mac (either processor):** `KSD-Decrypt-macOS-universal.dmg`. This larger download supports both architectures. All Mac builds require macOS 12.3 or later. Open the disk image and copy KSD Decrypt to Applications.
- **Windows x64:** `KSD-Decrypt-Windows-x64-setup.exe`. Requires Windows 10/11 and installs for the current user. This smaller installer downloads Microsoft WebView2 if it is missing or needs updating; internet may be required during installation.
- **Windows x64 (offline installation):** `KSD-Decrypt-Windows-x64-offline-setup.exe`. Includes the full WebView2 installer for computers without internet access.
- **Checksums:** `SHA256SUMS.txt` contains SHA-256 hashes of all five installers.

These builds are unsigned or ad-hoc signed and are not notarized. The operating system may warn about or block them. CI checks do not replace testing the installers on each operating system.

Recovery runs locally and leaves the originals untouched. Only the documented legacy Android KeepSafe format is supported; see the repository README for usage and compatibility.
