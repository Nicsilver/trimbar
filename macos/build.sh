#!/bin/bash
# Builds a universal (Apple silicon + Intel) Trimbar.app into macos/build.
# Needs the Xcode command line tools (xcode-select --install).
set -euo pipefail
cd "$(dirname "$0")"

app=build/Trimbar.app
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
for arch in arm64 x86_64; do
	swiftc -O -swift-version 5 -target "$arch-apple-macos13.0" Sources/*.swift -o "build/Trimbar-$arch"
done
lipo -create build/Trimbar-arm64 build/Trimbar-x86_64 -output "$app/Contents/MacOS/Trimbar"
rm build/Trimbar-arm64 build/Trimbar-x86_64
cp Info.plist "$app/Contents/"
cp ../assets/trimbar.icns "$app/Contents/Resources/"
codesign --force --sign - "$app"
echo "Built $PWD/$app"
