#!/bin/bash
# Construit NMEA Simulator.app et une image .dmg (à lancer sur macOS).
set -euo pipefail
cd "$(dirname "$0")/../.."
VERSION=$(cargo metadata --no-deps --format-version 1 | python3 -c "import json,sys;print([p for p in json.load(sys.stdin)['packages'] if p['name']=='nmeasim'][0]['version'])")
cargo build --release -p nmeasim
APP="target/macos/NMEA Simulator.app"
rm -rf "$APP" && mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/nmeasim "$APP/Contents/MacOS/"
sed "s/VERSION/$VERSION/g" packaging/macos/Info.plist > "$APP/Contents/Info.plist"
cp README.md "$APP/Contents/Resources/"
hdiutil create -volname "NMEA Simulator" -srcfolder "$APP" -ov -format UDZO "target/macos/nmeasim-rs-$VERSION.dmg"
echo "target/macos/nmeasim-rs-$VERSION.dmg"
