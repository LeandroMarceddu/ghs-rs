#!/usr/bin/env bash
#
# Build an unsigned macOS .app bundle for ghs-app.
#
# Usage:
#   packaging/macos/build-app.sh <binary> <version> <outdir>
#
#   <binary>   path to the release ghs-app binary
#   <version>  release version, with or without a leading v (v0.1.0, 0.1.0)
#   <outdir>   directory that receives ghs-rs.app
#
# The bundle is not code-signed and not notarized. It uses the same name,
# identifier, and resources everywhere; packaging/README.md documents the
# per-app quarantine removal for the first launch.

set -euo pipefail

prog=$(basename "$0")

die() {
    printf '%s: error: %s\n' "$prog" "$*" >&2
    exit 1
}

usage() {
    cat <<EOF
Usage: $prog <binary> <version> <outdir>

Build an unsigned ghs-rs.app bundle (no codesign, no notarization).

Arguments:
  binary    path to the release ghs-app binary
  version   release version, with or without a leading v (v0.1.0, 0.1.0)
  outdir    directory that receives ghs-rs.app
EOF
}

case "${1:-}" in
    -h | --help)
        usage
        exit 0
        ;;
esac

if [ "$#" -ne 3 ]; then
    usage >&2
    exit 2
fi

binary=$1
version=${2#v}
outdir=$3

[ -f "$binary" ] || die "binary not found: $binary"
[ -r "$binary" ] || die "binary is not readable: $binary"

# The tag charset is restricted so the values are safe inside the XML plist.
if ! [[ "$version" =~ ^[0-9][0-9A-Za-z.+~-]*$ ]]; then
    die "invalid version '$version' (expected something like 0.1.0 or 0.1.0-rc1)"
fi

# CFBundleVersion must be period-separated integers; keep the numeric core of
# the version and leave any prerelease suffix in CFBundleShortVersionString.
bundle_version=${version%%-*}
bundle_version=${bundle_version//[^0-9.]/}
bundle_version=${bundle_version#.}
bundle_version=${bundle_version%.}
[ -n "$bundle_version" ] || bundle_version=0

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
repo_root=$(cd "$script_dir/../.." && pwd)

required_files=(
    "$repo_root/README.md"
    "$repo_root/LICENSE-APACHE"
    "$repo_root/LICENSE-MIT"
    "$repo_root/NOTICE"
    "$repo_root/packaging/README.md"
)
for file in "${required_files[@]}"; do
    [ -f "$file" ] || die "missing documentation file: $file"
done

mkdir -p "$outdir"
outdir=$(cd "$outdir" && pwd)
app="$outdir/ghs-rs.app"
contents="$app/Contents"
resources="$contents/Resources"

rm -rf "$app"
mkdir -p "$contents/MacOS" "$resources"

install -m 0755 "$binary" "$contents/MacOS/ghs-app"
install -m 0644 "$repo_root/README.md" "$resources/README.md"
install -m 0644 "$repo_root/LICENSE-APACHE" "$resources/LICENSE-APACHE"
install -m 0644 "$repo_root/LICENSE-MIT" "$resources/LICENSE-MIT"
install -m 0644 "$repo_root/NOTICE" "$resources/NOTICE"
install -m 0644 "$repo_root/packaging/README.md" "$resources/PACKAGING.md"

cat >"$contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>en</string>
    <key>CFBundleDisplayName</key>
    <string>grblHAL Sender</string>
    <key>CFBundleExecutable</key>
    <string>ghs-app</string>
    <key>CFBundleIdentifier</key>
    <string>org.ghs-rs.sender</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>grblHAL Sender</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>${version}</string>
    <key>CFBundleVersion</key>
    <string>${bundle_version}</string>
    <key>LSApplicationCategoryType</key>
    <string>public.app-category.utilities</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>NSPrincipalClass</key>
    <string>NSApplication</string>
</dict>
</plist>
EOF
chmod 0644 "$contents/Info.plist"

# Traditional bundle marker; harmless on modern macOS and expected by some
# file managers and packaging tools.
printf 'APPL????' >"$contents/PkgInfo"
chmod 0644 "$contents/PkgInfo"

if command -v plutil >/dev/null 2>&1; then
    plutil -lint "$contents/Info.plist"
fi

printf 'Bundle: %s\n' "$app"
printf 'Version: %s (CFBundleVersion %s)\n' "$version" "$bundle_version"
printf 'Contents:\n'
(
    cd "$app"
    find . -print | LC_ALL=C sort | sed 's/^/  /'
)
