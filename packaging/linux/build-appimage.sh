#!/usr/bin/env bash
#
# Build an unsigned AppImage for ghs-app with the pinned appimagetool 1.9.1.
#
# Usage:
#   packaging/linux/build-appimage.sh <binary> <version> <arch> <outdir>
#
#   <binary>   path to the release ghs-app binary
#   <version>  release version, with or without a leading v (v0.1.0, 0.1.0)
#   <arch>     x86_64/amd64 or aarch64/arm64
#   <outdir>   directory that receives ghs-rs-<tag>-<arch>.AppImage
#
# appimagetool is downloaded once per architecture from its pinned release and
# verified against a SHA-256. It runs with --appimage-extract-and-run, so FUSE
# is not needed on the build host. The produced AppImage uses the host GPUI
# runtime libraries: no GTK, Wayland, Vulkan, or Mesa binaries are bundled.

set -euo pipefail

prog=$(basename -- "$0")

die() {
    printf '%s: error: %s\n' "$prog" "$*" >&2
    exit 1
}

usage() {
    cat <<EOF
Usage: $prog <binary> <version> <arch> <outdir>

Build an unsigned AppImage for ghs-app with appimagetool 1.9.1.

Arguments:
  binary    path to the release ghs-app binary
  version   release version, with or without a leading v (v0.1.0, 0.1.0)
  arch      x86_64/amd64 or aarch64/arm64
  outdir    directory that receives ghs-rs-<tag>-<arch>.AppImage

Environment:
  GHS_APPIMAGETOOL  path to an existing appimagetool instead of downloading it
  GHS_TOOL_CACHE    download cache (default: \${XDG_CACHE_HOME:-\$HOME/.cache}/ghs-rs)
EOF
}

case "${1:-}" in
    -h | --help)
        usage
        exit 0
        ;;
esac

if [ "$#" -ne 4 ]; then
    usage >&2
    exit 2
fi

binary=$1
tag_version=$2
version=${2#v}
source_arch=$3
outdir=$4

[ -f "$binary" ] || die "binary not found: $binary"
[ -r "$binary" ] || die "binary is not readable: $binary"

if ! [[ "$version" =~ ^[0-9][0-9A-Za-z.+~-]*$ ]]; then
    die "invalid version '$version' (expected something like 0.1.0 or 0.1.0-rc1)"
fi

case "$source_arch" in
    x86_64 | amd64)
        appimage_arch=x86_64
        ;;
    aarch64 | arm64)
        appimage_arch=aarch64
        ;;
    *)
        die "unsupported architecture '$source_arch' (expected x86_64/amd64 or aarch64/arm64)"
        ;;
esac

for tool in desktop-file-validate sha256sum; do
    command -v "$tool" >/dev/null 2>&1 || die "required tool not found: $tool"
done
command -v curl >/dev/null 2>&1 || command -v wget >/dev/null 2>&1 ||
    die "curl or wget is required to download appimagetool"

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
repo_root=$(cd -- "$script_dir/../.." && pwd)

required_files=(
    "$script_dir/ghs-rs.desktop"
    "$script_dir/ghs-rs.svg"
    "$repo_root/README.md"
    "$repo_root/LICENSE-APACHE"
    "$repo_root/LICENSE-MIT"
    "$repo_root/NOTICE"
    "$repo_root/packaging/README.md"
)
for file in "${required_files[@]}"; do
    [ -f "$file" ] || die "missing packaging file: $file"
done

mkdir -p -- "$outdir"
outdir=$(cd -- "$outdir" && pwd)
output="$outdir/ghs-rs-${tag_version}-${appimage_arch}.AppImage"

workdir=$(mktemp -d "${TMPDIR:-/tmp}/ghs-rs-appimage.XXXXXX")
cleanup() {
    rm -rf -- "$workdir"
}
trap cleanup EXIT

# The AppDir name must match the desktop file and icon base name.
appdir="$workdir/ghs-rs.AppDir"
docdir="$appdir/usr/share/doc/ghs-rs"
iconsdir="$appdir/usr/share/icons/hicolor/scalable/apps"

mkdir -p "$appdir/usr/bin" "$appdir/usr/share/applications" "$iconsdir" "$docdir"

install -m 0755 -- "$binary" "$appdir/usr/bin/ghs-app"
install -m 0644 -- "$script_dir/ghs-rs.desktop" "$appdir/ghs-rs.desktop"
install -m 0644 -- "$script_dir/ghs-rs.desktop" \
    "$appdir/usr/share/applications/ghs-rs.desktop"
install -m 0644 -- "$script_dir/ghs-rs.svg" "$appdir/ghs-rs.svg"
install -m 0644 -- "$script_dir/ghs-rs.svg" "$iconsdir/ghs-rs.svg"

for doc in README.md LICENSE-APACHE LICENSE-MIT NOTICE; do
    install -m 0644 -- "$repo_root/$doc" "$docdir/$doc"
done
install -m 0644 -- "$repo_root/packaging/README.md" "$docdir/PACKAGING.md"

# AppRun must stay relative: AppImages are mounted or extracted at arbitrary
# paths, and an absolute symlink or path breaks the bundle.
cat >"$appdir/AppRun" <<'APPRUN'
#!/bin/sh
#
# ghs-rs uses the host GPUI runtime instead of bundling GTK, Wayland, Vulkan,
# or Mesa libraries. Install the distribution packages listed in
# usr/share/doc/ghs-rs/PACKAGING.md (libc6, libwayland-client0,
# libxkbcommon0, libfontconfig1, libfreetype6, libvulkan1, libxcb1,
# libxkbcommon-x11-0, libudev1) and keep a working Vulkan driver.
set -eu
HERE="$(dirname "$(readlink -f "$0")")"
export PATH="$HERE/usr/bin:$PATH"
export XDG_DATA_DIRS="$HERE/usr/share:/usr/share:/usr/local/share:${XDG_DATA_DIRS:-/usr/share}"
exec "$HERE/usr/bin/ghs-app" "$@"
APPRUN
chmod 0755 "$appdir/AppRun"

# .DirIcon must be a relative symlink. Absolute symlinks survive the build but
# dangle once the AppImage is mounted or extracted elsewhere.
ln -s ghs-rs.svg "$appdir/.DirIcon"
if [ "$(readlink "$appdir/.DirIcon")" != "ghs-rs.svg" ]; then
    die ".DirIcon is not a relative symlink to ghs-rs.svg"
fi

# Normalize directory modes; some CI images run with a restrictive umask.
find "$appdir" -type d -exec chmod 0755 {} +

desktop-file-validate "$appdir/ghs-rs.desktop"

# appimagetool 1.9.1 (released 2025-04-26). Checksums are for the exact
# upstream release assets; update both entries together when upgrading.
case "$appimage_arch" in
    x86_64)
        tool_url=https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool-x86_64.AppImage
        tool_sha256=ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0
        ;;
    aarch64)
        tool_url=https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool-aarch64.AppImage
        tool_sha256=f0837e7448a0c1e4e650a93bb3e85802546e60654ef287576f46c71c126a9158
        ;;
    *)
        die "no pinned appimagetool for architecture '$appimage_arch'"
        ;;
esac

tool=${GHS_APPIMAGETOOL:-}
if [ -z "$tool" ]; then
    cache_dir=${GHS_TOOL_CACHE:-${XDG_CACHE_HOME:-$HOME/.cache}/ghs-rs}
    mkdir -p -- "$cache_dir"
    tool="$cache_dir/appimagetool-${appimage_arch}.AppImage"
    if [ ! -f "$tool" ]; then
        partial="$tool.part"
        rm -f -- "$partial"
        printf 'Downloading %s\n' "$tool_url"
        if command -v curl >/dev/null 2>&1; then
            curl -fL --retry 3 --connect-timeout 20 -o "$partial" "$tool_url"
        else
            wget -O "$partial" "$tool_url"
        fi
        actual_sha256=$(sha256sum "$partial" | cut -d' ' -f1)
        if [ "$actual_sha256" != "$tool_sha256" ]; then
            rm -f -- "$partial"
            die "appimagetool checksum mismatch (expected $tool_sha256, got $actual_sha256)"
        fi
        chmod 0755 "$partial"
        mv -- "$partial" "$tool"
    fi
    actual_sha256=$(sha256sum "$tool" | cut -d' ' -f1)
    if [ "$actual_sha256" != "$tool_sha256" ]; then
        die "cached appimagetool checksum mismatch (expected $tool_sha256, got $actual_sha256)"
    fi
else
    [ -x "$tool" ] || die "GHS_APPIMAGETOOL is not executable: $tool"
fi

# A dedicated directory collects the tool's squashfs-root extraction when it
# runs with --appimage-extract-and-run, so the repository stays clean.
tool_run_dir="$workdir/appimagetool"
mkdir -p "$tool_run_dir"

printf 'AppDir contents:\n'
(
    cd -- "$appdir"
    find . -mindepth 1 -print | LC_ALL=C sort | sed 's/^/  /'
)

rm -f -- "$output"
(
    cd -- "$tool_run_dir"
    ARCH="$appimage_arch" VERSION="$version" "$tool" \
        --appimage-extract-and-run --no-appstream "$appdir" "$output"
)
chmod 0755 "$output"

[ -s "$output" ] || die "appimagetool produced an empty file: $output"
offset=$("$output" --appimage-offset 2>/dev/null || true)
if ! [[ "$offset" =~ ^[0-9]+$ ]]; then
    die "produced AppImage does not report a valid --appimage-offset"
fi
if command -v file >/dev/null 2>&1; then
    printf 'File type: %s\n' "$(file -b "$output")"
fi

printf 'Built %s\n' "$output"
printf 'Size: %s bytes\n' "$(wc -c <"$output" | tr -d ' ')"
printf 'SHA-256: %s\n' "$(sha256sum "$output" | cut -d' ' -f1)"
