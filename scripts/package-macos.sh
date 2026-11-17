#!/bin/bash
# Build a self-contained application bundle. Run on macOS with Xcode Command Line Tools and Rust stable.
set -euo pipefail
repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"
export PATH="${HOME}/.cargo/bin:${PATH}"
export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-12.0}"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-4}"
skip_build=0
target="$(rustc -vV | sed -n 's/^host: //p')"
output="$repo_root/dist"
while [ "$#" -gt 0 ]; do
    case "$1" in
        --skip-build) skip_build=1; shift ;;
        --target) target="$2"; shift 2 ;;
        --output-dir) output="$2"; shift 2 ;;
        *) echo "Usage: $0 [--skip-build] [--target aarch64-apple-darwin|x86_64-apple-darwin] [--output-dir PATH]" >&2; exit 2 ;;
    esac
done
case "$target" in
    aarch64-apple-darwin) arch=arm64 ;;
    x86_64-apple-darwin) arch=x86_64 ;;
    *) echo "Unsupported macOS target: $target" >&2; exit 2 ;;
esac
version="$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -1)"
export QINGYU_BUILD_LABEL="${QINGYU_BUILD_LABEL:-${version}-macos.1}"
if [ "$skip_build" -eq 0 ]; then
    cargo build --locked --release --bin qingyu --target "$target"
fi
target_root="${CARGO_TARGET_DIR:-$repo_root/target}"
executable="$target_root/$target/release/qingyu"
[ -f "$executable" ] || { echo "Missing executable: $executable" >&2; exit 1; }
lipo "$executable" -verify_arch "$arch"
mkdir -p "$output"
output="$(cd "$output" && pwd)"
staging="$(mktemp -d "$output/.macos-package.XXXXXX")"
trap 'rm -rf "$staging"' EXIT
app="$staging/轻屿.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$executable" "$app/Contents/MacOS/qingyu"
chmod 755 "$app/Contents/MacOS/qingyu"
cp LICENSE OPEN_SOURCE_NOTICES.md THIRD_PARTY_NOTICES.txt "$app/Contents/Resources/"
cp assets/qingyu/status-icon.png "$app/Contents/Resources/qingyu-status.png"
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>com.qingyu.macos</string>
<key>CFBundleName</key><string>轻屿</string>
<key>CFBundleDisplayName</key><string>轻屿</string>
<key>CFBundleExecutable</key><string>qingyu</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>$version</string>
<key>CFBundleVersion</key><string>$version</string>
<key>CFBundleIconFile</key><string>qingyu</string>
<key>LSMinimumSystemVersion</key><string>$MACOSX_DEPLOYMENT_TARGET</string>
<key>LSUIElement</key><true/>
<key>NSHighResolutionCapable</key><true/>
<key>NSPrincipalClass</key><string>NSApplication</string>
<key>NSAppleEventsUsageDescription</key><string>轻屿读取 Apple Music 和 Spotify 的歌曲信息，并在你点击时控制播放。</string>
<key>NSHumanReadableCopyright</key><string>Qingyu · MIT License</string>
</dict></plist>
PLIST
iconset="$staging/qingyu.iconset"
mkdir -p "$iconset"
for size in 16 32 128 256 512; do
    sips -z "$size" "$size" assets/qingyu/app-icon.png --out "$iconset/icon_${size}x${size}.png" >/dev/null
    double=$((size * 2))
    sips -z "$double" "$double" assets/qingyu/app-icon.png --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$app/Contents/Resources/qingyu.icns"
plutil -lint "$app/Contents/Info.plist"
local_identity=0
if [ -z "${QINGYU_SIGNING_IDENTITY:-}" ]; then
    # Preserve local Keychain trust across debug, installed and rebuilt apps.
    # This also prepares/upgrades the mutually authenticated helper in place.
    identity="$(bash scripts/macos-dev-runner.sh --signing-identity)"
    local_identity=1
else
    identity="$QINGYU_SIGNING_IDENTITY"
fi
if [ "$identity" = - ]; then
    codesign --force --deep --sign - --timestamp=none "$app"
elif [ "$local_identity" -eq 1 ]; then
    codesign --force --deep --sign "$identity" --timestamp=none "$app"
else
    codesign --force --deep --sign "$identity" --options runtime --timestamp --entitlements installer/macos.entitlements "$app"
fi
codesign --verify --deep --strict "$app"
# All contents here are generated output; user data lives in Application Support.
final_app="$output/轻屿.app"
if [ -e "$final_app" ]; then mv "$final_app" "$staging/previous.app"; fi
mv "$app" "$final_app"
archive="$output/Qingyu-v${version}-macos-${arch}.zip"
ditto -c -k --sequesterRsrc --keepParent "$final_app" "$archive"
(cd "$output" && shasum -a 256 "$(basename "$archive")" > "SHA256SUMS-macos-${arch}.txt")
echo "$final_app"
echo "$archive"
