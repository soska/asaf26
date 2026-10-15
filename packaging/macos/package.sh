#!/bin/sh
# Builds a release Asaf.app for macOS:
#
#   packaging/macos/package.sh [--zip]
#
# Output: target/release/Asaf.app (and target/release/Asaf-<version>-macos.zip
# with --zip). The app is signed ad hoc only (so it runs on the machine that
# built it and on Apple Silicon); Developer ID signing and notarization for
# distribution are out of scope, see README ("Packaging").
#
# ASAF_BUNDLE_ID overrides the bundle identifier (default local.asaf.Asaf).
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
cd "$root"

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
build=$(git rev-list --count HEAD 2>/dev/null || echo 1)
bundle_id=${ASAF_BUNDLE_ID:-local.asaf.Asaf}

echo "Building asaf $version (release)…"
cargo build --release -p asaf

app=target/release/Asaf.app
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources/Licenses"
cp target/release/asaf "$app/Contents/MacOS/asaf"
sed -e "s/@VERSION@/$version/" -e "s/@BUILD@/$build/" -e "s/@BUNDLE_ID@/$bundle_id/" \
    "$here/Info.plist.in" > "$app/Contents/Info.plist"
printf 'APPL????' > "$app/Contents/PkgInfo"
plutil -lint "$app/Contents/Info.plist" >/dev/null

# Icon: every size macOS asks for, from the 1024 px placeholder.
iconset=$(mktemp -d)/Asaf.iconset
mkdir -p "$iconset"
for size in 16 32 128 256 512; do
    sips -z $size $size "$here/icon.png" --out "$iconset/icon_${size}x${size}.png" >/dev/null
    double=$((size * 2))
    sips -z $double $double "$here/icon.png" --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$app/Contents/Resources/Asaf.icns"
rm -rf "$(dirname "$iconset")"

# The bundled fonts are inside the binary; their licenses travel with the app.
for license in crates/fonts/files/*-OFL.txt; do
    cp "$license" "$app/Contents/Resources/Licenses/"
done

codesign --force --sign - "$app" >/dev/null 2>&1 || echo "warning: ad-hoc signing failed; the app may not open on Apple Silicon" >&2

if [ "${1:-}" = "--zip" ]; then
    zip="target/release/Asaf-$version-macos.zip"
    rm -f "$zip"
    ditto -c -k --keepParent "$app" "$zip"
    echo "Packed $zip"
fi
echo "Built $app"
