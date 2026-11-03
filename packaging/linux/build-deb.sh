#!/usr/bin/env bash
#
# Build an unsigned Debian package for ghs-app without dpkg-deb.
#
# Usage:
#   packaging/linux/build-deb.sh <binary> <version> <arch> <outdir>
#
#   <binary>   path to the release ghs-app binary
#   <version>  release version, with or without a leading v (v0.1.0, 0.1.0)
#   <arch>     source architecture: x86_64/amd64 or aarch64/arm64
#   <outdir>   directory that receives ghs-rs_<version>_<debian-arch>.deb
#
# The archive is assembled with ar, tar and gzip, so dpkg-deb is not needed.
# The package is unsigned; it is not built with dpkg-buildpackage and has no
# source package. Run this from a Linux host; GNU tar is required.

set -euo pipefail

prog=$(basename -- "$0")

die() {
    printf '%s: error: %s\n' "$prog" "$*" >&2
    exit 1
}

usage() {
    cat <<EOF
Usage: $prog <binary> <version> <arch> <outdir>

Build an unsigned Debian package for ghs-app using ar and tar.

Arguments:
  binary    path to the release ghs-app binary
  version   release version, with or without a leading v (v0.1.0, 0.1.0)
  arch      x86_64/amd64 or aarch64/arm64
  outdir    directory that receives ghs-rs_<version>_<arch>.deb

Environment:
  GHS_DEB_MAINTAINER  Maintainer field value
                      (default: Leandro Marceddu <leandro.marceddu@users.noreply.github.com>)
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
version=${2#v}
source_arch=$3
outdir=$4

[ -f "$binary" ] || die "binary not found: $binary"
[ -r "$binary" ] || die "binary is not readable: $binary"

if ! [[ "$version" =~ ^[0-9][0-9A-Za-z.+~-]*$ ]]; then
    die "invalid Debian version '$version' (expected something like 0.1.0 or 0.1.0-rc1)"
fi

case "$source_arch" in
    x86_64 | amd64)
        deb_arch=amd64
        ;;
    aarch64 | arm64)
        deb_arch=arm64
        ;;
    *)
        die "unsupported architecture '$source_arch' (expected x86_64/amd64 or aarch64/arm64)"
        ;;
esac

for tool in ar tar gzip md5sum du find; do
    command -v "$tool" >/dev/null 2>&1 || die "required tool not found: $tool"
done

if ! tar --version 2>/dev/null | grep -q 'GNU tar'; then
    die "GNU tar is required to build a reproducible Debian data archive"
fi

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
repo_root=$(cd -- "$script_dir/../.." && pwd)

doc_sources=(
    "$repo_root/README.md"
    "$repo_root/LICENSE-APACHE"
    "$repo_root/LICENSE-MIT"
    "$repo_root/NOTICE"
    "$repo_root/packaging/README.md"
    "$script_dir/ghs-rs.desktop"
    "$script_dir/ghs-rs.svg"
)
for source in "${doc_sources[@]}"; do
    [ -f "$source" ] || die "missing documentation file: $source"
done

maintainer=${GHS_DEB_MAINTAINER:-Leandro Marceddu <leandro.marceddu@users.noreply.github.com>}
homepage=https://github.com/LeandroMarceddu/ghs-rs

mkdir -p -- "$outdir"
outdir=$(cd -- "$outdir" && pwd)
deb="$outdir/ghs-rs_${version}_${deb_arch}.deb"

workdir=$(mktemp -d "${TMPDIR:-/tmp}/ghs-rs-deb.XXXXXX")
cleanup() {
    rm -rf -- "$workdir"
}
trap cleanup EXIT

# Payload: /usr/bin, the desktop entry and its icon, and Debian-style docs.
root="$workdir/root"
docdir="$root/usr/share/doc/ghs-rs"
iconsdir="$root/usr/share/icons/hicolor/scalable/apps"
mkdir -p "$root/usr/bin" "$root/usr/share/applications" "$iconsdir" "$docdir"

install -m 0755 -- "$binary" "$root/usr/bin/ghs-app"
install -m 0644 -- "$script_dir/ghs-rs.desktop" \
    "$root/usr/share/applications/ghs-rs.desktop"
install -m 0644 -- "$script_dir/ghs-rs.svg" "$iconsdir/ghs-rs.svg"
install -m 0644 -- "$repo_root/README.md" "$docdir/README.md"
install -m 0644 -- "$repo_root/LICENSE-APACHE" "$docdir/LICENSE-APACHE"
install -m 0644 -- "$repo_root/LICENSE-MIT" "$docdir/LICENSE-MIT"
install -m 0644 -- "$repo_root/NOTICE" "$docdir/NOTICE"
install -m 0644 -- "$repo_root/packaging/README.md" "$docdir/PACKAGING.md"

cat >"$docdir/copyright" <<EOF
Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/
Upstream-Name: ghs-rs
Upstream-Contact: $maintainer
Source: $homepage

Files: *
Copyright: 2026 Leandro Marceddu
License: MIT or Apache-2.0
 The full license texts are installed in this directory as LICENSE-MIT and
 LICENSE-APACHE. The application is provided "as is", without warranty of
 any kind.
Comment:
 ghs-rs is an independent Rust implementation. ioSender is a behavioral
 reference only; see NOTICE for third-party attribution.
EOF
chmod 0644 "$docdir/copyright"

# A binary package is expected to carry a Debian-format changelog.
changelog_date=$(LC_ALL=C date -u '+%a, %d %b %Y %H:%M:%S +0000')
cat >"$workdir/changelog.Debian" <<EOF
ghs-rs ($version) unstable; urgency=medium

  * Release v$version.

 -- $maintainer  $changelog_date
EOF
gzip -9n -c "$workdir/changelog.Debian" >"$docdir/changelog.Debian.gz"
chmod 0644 "$docdir/changelog.Debian.gz"

installed_size=$(du -sk "$root" | cut -f1)
md5sums="$workdir/control/md5sums"
mkdir -p "$workdir/control"
(
    cd -- "$root"
    find usr -type f -print0 | LC_ALL=C sort -z | xargs -0 md5sum
) >"$md5sums"

cat >"$workdir/control/control" <<EOF
Package: ghs-rs
Version: $version
Architecture: $deb_arch
Maintainer: $maintainer
Installed-Size: $installed_size
Depends: libc6, libwayland-client0, libxkbcommon0, libfontconfig1, libfreetype6, libvulkan1, libxcb1, libxkbcommon-x11-0, libudev1
Section: utils
Priority: optional
Homepage: $homepage
Description: grblHAL G-code sender
 ghs-rs is a cross-platform G-code sender for grblHAL controllers
 connected over a serial port.
 .
 It provides a touch-first mill control UI, job streaming, probing and
 height-map workflows, and an in-process simulator for offline testing.
 .
 This package is unsigned and has not been hardware-validated.
EOF

# Debian members are plain tar.gz streams with ./ entries and no leading
# directory, packed in the order debian-binary, control.tar.gz, data.tar.gz.
tar_flags=(
    --format=gnu
    --sort=name
    --mtime=@0
    --owner=0
    --group=0
    --numeric-owner
)

printf '2.0\n' >"$workdir/debian-binary"
tar "${tar_flags[@]}" -czf "$workdir/control.tar.gz" -C "$workdir/control" ./
tar "${tar_flags[@]}" -czf "$workdir/data.tar.gz" -C "$root" ./

rm -f -- "$deb"
(
    cd -- "$workdir"
    ar rc "$deb" debian-binary control.tar.gz data.tar.gz
)

members=$(ar t "$deb")
expected_members=$'debian-binary\ncontrol.tar.gz\ndata.tar.gz'
if [ "$members" != "$expected_members" ]; then
    die "unexpected archive members in $deb: ${members//$'\n'/, }"
fi

printf 'Built %s\n' "$deb"
printf 'Members:\n'
printf '%s\n' "$members" | sed 's/^/  /'
printf 'Size: %s bytes\n' "$(wc -c <"$deb" | tr -d ' ')"
printf 'SHA-256: %s\n' "$(sha256sum "$deb" | cut -d' ' -f1)"
