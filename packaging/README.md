# Release packaging

The `Release` workflow (`.github/workflows/release.yml`) builds `ghs-app` in
release mode on Linux, Windows, and macOS, then attaches the release assets to a
**draft** GitHub Release. [`docs/RELEASE.md`](../docs/RELEASE.md) is the release
checklist; publishing is a manual decision after that checklist passes.

## Artifacts

| Platform | Asset | Contents |
|---|---|---|
| Linux x86_64 | `ghs-rs-<tag>-linux-x86_64.tar.gz` | binary, desktop entry, icon, notices |
| Linux x86_64 | `ghs-rs_<version>_amd64.deb` | binary, desktop entry, icon, docs |
| Linux x86_64 | `ghs-rs-<tag>-x86_64.AppImage` | binary, `AppRun`, desktop entry, icon, docs |
| Windows x86_64 | `ghs-rs-<tag>-windows-x86_64.zip` | `ghs-app.exe`, notices |
| macOS | `ghs-rs-<tag>-macos-<arch>.tar.gz` | `ghs-rs.app`, notices |

`<tag>` includes the leading `v` (for example `v0.5.0`). The Debian package
drops it, as Debian versions must start with a digit: `ghs-rs_0.5.0_amd64.deb`.
`<arch>` in the macOS archive is the architecture reported by the macOS runner
(`aarch64` on Apple Silicon, or `x86_64` if the runner image changes). The release
workflow currently expects x86_64 on Linux; the packaging scripts also map
`aarch64`/`arm64` to `arm64` (deb) and `aarch64` (AppImage) for future arm
runners.

The release job also publishes `SHA256SUMS.txt`, which is not inside any
archive. Both the Linux and macOS tar.gz archives unpack to a single top-level
directory named after the archive without its extension. The `.deb` and
`.AppImage` are single-file downloads.

## Linux

### tar.gz

```sh
tar -xzf ghs-rs-v0.5.0-linux-x86_64.tar.gz
cd ghs-rs-v0.5.0-linux-x86_64
install -Dm755 ghs-app ~/.local/bin/ghs-app
install -Dm644 ghs-rs.desktop ~/.local/share/applications/ghs-rs.desktop
install -Dm644 ghs-rs.svg ~/.local/share/icons/hicolor/scalable/apps/ghs-rs.svg
```

The desktop entry uses `Exec=ghs-app`, so `~/.local/bin` must be in `PATH`, and
`Icon=ghs-rs`, which the installed SVG provides. Some desktop environments cache
the menu; `gtk-update-icon-cache ~/.local/share/icons/hicolor` or a re-login
refreshes it.

### Debian package

The package is unsigned. Install it with `apt`, which resolves the GPUI runtime
dependencies in one step:

```sh
sudo apt install ./ghs-rs_0.5.0_amd64.deb
```

`dpkg -i` also works, but it does not install missing dependencies:

```sh
sudo dpkg -i ghs-rs_0.5.0_amd64.deb
sudo apt-get install -f   # only if dpkg reported unmet dependencies
```

The package installs `/usr/bin/ghs-app`, the desktop entry at
`/usr/share/applications/ghs-rs.desktop`, the icon at
`/usr/share/icons/hicolor/scalable/apps/ghs-rs.svg`, and the README, licenses,
NOTICE, copyright, and changelog under `/usr/share/doc/ghs-rs/`. Remove it with
`sudo apt remove ghs-rs`.

The package does not bundle GTK, Wayland, Vulkan, or Mesa libraries. It depends
on the host GPUI runtime: `libc6`, `libwayland-client0`, `libxkbcommon0`,
`libfontconfig1`, `libfreetype6`, `libvulkan1`, plus the X11/xcb and udev
libraries the binary links directly (`libxcb1`, `libxkbcommon-x11-0`,
`libudev1`). A working Vulkan driver is still required.

### AppImage

```sh
chmod +x ghs-rs-v0.5.0-x86_64.AppImage
./ghs-rs-v0.5.0-x86_64.AppImage
```

The AppImage is unsigned and is not a self-contained runtime: it bundles the
application, desktop entry, icon, and notices, but intentionally uses the host
GPUI libraries listed above instead of bundling GTK, Wayland, Vulkan, or Mesa.
Install the same packages the `.deb` depends on, and keep a working Vulkan
driver.

Type 2 AppImages normally need FUSE 2 (`libfuse2` on Debian/Ubuntu). If FUSE is
unavailable, or inside a container, extract and run without it:

```sh
./ghs-rs-v0.5.0-x86_64.AppImage --appimage-extract-and-run
```

`--appimage-extract` also works for inspection; it never mounts the image. The
AppImage has no embedded update information and does not self-update.

### Serial port access

Users who open serial ports need membership in `dialout`:

```sh
sudo usermod -aG dialout "$USER"
```

Log out and back in after changing the group. GPUI needs an X11 or Wayland
session with a Vulkan driver; see the repository README for the Debian/Ubuntu
package list.

## macOS

The archive contains an unsigned `ghs-rs.app` beside the README, licenses,
NOTICE, and `PACKAGING.md`:

```sh
tar -xzf ghs-rs-v0.5.0-macos-aarch64.tar.gz
mv ghs-rs-v0.5.0-macos-aarch64/ghs-rs.app /Applications/
```

The bundle is not code-signed and not notarized, so Gatekeeper blocks the first
launch. After verifying the checksum, remove the quarantine attribute for that
app only:

```sh
xattr -dr com.apple.quarantine /Applications/ghs-rs.app
open /Applications/ghs-rs.app
```

Alternatively, right-click the app in Finder and choose Open, then confirm. Do
not disable Gatekeeper system-wide. `codesign -dv ghs-rs.app` reports an
unsigned bundle and `spctl -a -vv ghs-rs.app` rejects it; both are expected. A
signed and notarized `.app` and a `.dmg` are follow-ups.

## Windows

The zip contains an unsigned `ghs-app.exe`. SmartScreen warns on first launch:
choose "More info", verify the checksum, then "Run anyway". There is no
installer yet; the executable is portable. Do not disable SmartScreen
system-wide.

## Checksums

Download `SHA256SUMS.txt` from the same release and verify every asset before
running it:

```sh
sha256sum -c SHA256SUMS.txt
```

The file covers the Linux tar.gz, `.deb`, and `.AppImage`, the Windows zip, and
the macOS tar.gz. On Windows:

```powershell
Get-FileHash .\ghs-rs-v0.5.0-windows-x86_64.zip -Algorithm SHA256
```

Compare the value with the matching line in `SHA256SUMS.txt`.

## Building the packages locally

From the repository root, after `cargo build --release -p ghs-app`:

```sh
packaging/linux/build-deb.sh target/release/ghs-app v0.5.0 x86_64 dist
packaging/linux/build-appimage.sh target/release/ghs-app v0.5.0 x86_64 dist
packaging/macos/build-app.sh target/release/ghs-app v0.5.0 dist
```

The Debian script needs `ar`, GNU tar, `gzip`, and `md5sum`; it does not use
`dpkg-deb`. The AppImage script downloads the pinned appimagetool 1.9.1 (with a
SHA-256 check) and validates the desktop entry with `desktop-file-validate`; it
runs the tool with `--appimage-extract-and-run`, so the build host does not need
FUSE. appimagetool downloads its type 2 runtime during the build, so the first
AppImage build needs network access. The macOS script runs on Linux or macOS for
layout validation; `plutil -lint` is used on macOS when available.

## Not included

- Code signing, notarization, and Windows Authenticode.
- macOS `.dmg` and universal binary.
- Windows installer (MSI/MSIX); RPM, Flatpak, Snap, or distribution repositories.
- AppImage update information, zsync metadata, and self-updates.
- Auto-update metadata.
- Final branded icons. A simple placeholder `ghs-rs.svg` is bundled with the
  `.tar.gz`, `.deb`, and AppImage and used by the desktop entry; replacing it
  with real branding is a follow-up.
