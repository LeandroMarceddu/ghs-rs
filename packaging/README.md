# Release packaging

The `Release` workflow (`.github/workflows/release.yml`) builds `ghs-app` in
release mode on Linux, Windows, and macOS, then attaches one archive per
platform to a **draft** GitHub Release. [`docs/RELEASE.md`](../docs/RELEASE.md)
is the release checklist; publishing is a manual decision after that checklist
passes.

## Archive layout

Every archive contains the binary, the README, both licenses, the NOTICE file,
and a copy of this file as `PACKAGING.md`. File names are stable so download
scripts can find them:

| Platform | Archive | Binary | Extra |
|---|---|---|---|
| Linux x86_64 | `ghs-rs-<tag>-linux-x86_64.tar.gz` | `ghs-app` | `ghs-rs.desktop` |
| Windows x86_64 | `ghs-rs-<tag>-windows-x86_64.zip` | `ghs-app.exe` | none |
| macOS | `ghs-rs-<tag>-macos-<arch>.tar.gz` | `ghs-app` | none |

`<tag>` includes the leading `v` (for example `v0.1.0`). `<arch>` is the
architecture reported by the macOS runner (`aarch64` on Apple Silicon, or
`x86_64` if the runner image changes). The release job also publishes
`SHA256SUMS.txt`; it is not inside the archives.

Each archive unpacks to a single top-level directory named after the archive
without its extension.

## Linux

```sh
tar -xzf ghs-rs-v0.1.0-linux-x86_64.tar.gz
cd ghs-rs-v0.1.0-linux-x86_64
install -Dm755 ghs-app ~/.local/bin/ghs-app
install -Dm644 ghs-rs.desktop ~/.local/share/applications/ghs-rs.desktop
```

The desktop entry uses `Exec=ghs-app`, so `~/.local/bin` must be in `PATH`.
Users who open serial ports need membership in `dialout`:

```sh
sudo usermod -aG dialout "$USER"
```

Log out and back in after changing the group. GPUI needs an X11 or Wayland
session with a Vulkan driver; see the repository README for the Debian/Ubuntu
package list.

## macOS

The archive is a bare binary, not an `.app` bundle. It is unsigned and not
notarized, so Gatekeeper blocks the first launch. After verifying the checksum,
either right-click the binary and choose Open, or remove the quarantine
attribute for that file only:

```sh
xattr -dr com.apple.quarantine ./ghs-app
chmod +x ./ghs-app
./ghs-app
```

Do not disable Gatekeeper system-wide. A signed and notarized `.app` bundle is
a follow-up.

## Windows

The zip contains an unsigned `ghs-app.exe`. SmartScreen warns on first launch:
choose "More info", verify the checksum, then "Run anyway". There is no
installer yet; the executable is portable. Do not disable SmartScreen
system-wide.

## Checksums

Download `SHA256SUMS.txt` from the same release and verify before running:

```sh
sha256sum -c SHA256SUMS.txt
```

On Windows:

```powershell
Get-FileHash .\ghs-rs-v0.1.0-windows-x86_64.zip -Algorithm SHA256
```

Compare the value with the matching line in `SHA256SUMS.txt`.

## Not included

- Code signing, notarization, and Windows Authenticode.
- macOS `.app` bundle, `.dmg`, and universal binary.
- Windows installer (MSI/MSIX); Linux `.deb`, `.rpm`, Flatpak, or AppImage.
- Auto-update metadata.
- A custom application icon. The desktop entry uses the themed
  `applications-engineering` fallback until branding lands.
