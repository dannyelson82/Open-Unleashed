#!/bin/sh
# Builds the Apple TV app for the device (release) and packages it as an .ipa for sideloading with atvloadly
# (TVOS.md, "Sideloading with atvloadly"). atvloadly signs the app with its own Apple ID when it installs it,
# so this build is unsigned and needs no Apple ID or team on this Mac.
#
#   scripts/export-ipa.sh                 # → target/tvos/OpenUnleashed.ipa
#   OU_BUNDLE_ID=com.you.openunleashed scripts/export-ipa.sh
#
# The .ipa never contains game files (the app downloads them from the OpenUnleashed server). GPL-3.0-or-later.
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
[ -x "$HOME/homebrew/bin/brew" ] && eval "$("$HOME/homebrew/bin/brew" shellenv)"
: "${OU_BUNDLE_ID:=org.openunleashed.tvos}"
: "${OU_TEAM_ID:=}"
export OU_BUNDLE_ID OU_TEAM_ID
export TVOS_DEPLOYMENT_TARGET=17.0

echo "== cargo: engine static library for aarch64-apple-tvos (release)"
CARGO_TARGET_DIR="$ROOT/target/tvos" cargo rustc --release --locked --target aarch64-apple-tvos -p open-unleashed --lib --crate-type staticlib

echo "== xcodegen"
platform/tvos/configure.sh

echo "== xcodebuild (appletvos, unsigned)"
DERIVED="$ROOT/target/tvos/xcode"
xcodebuild -project platform/tvos/OpenUnleashed.xcodeproj -scheme OpenUnleashed -configuration Release \
  -sdk appletvos -destination "generic/platform=tvOS" -derivedDataPath "$DERIVED" \
  CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO CODE_SIGN_IDENTITY="" build | \
  grep -E '^(\*\*|error|ld: )' || true
APP="$DERIVED/Build/Products/Release-appletvos/OpenUnleashed.app"
[ -d "$APP" ] || { echo "build failed: no $APP" >&2; exit 1; }

# never ship a game file by accident
if find "$APP" \( -iname GameData -o -iname FEData -o -iname '*.viv' -o -iname '*.crp' -o -iname '*.fsh' \) | grep -q .; then
  echo "refusing: the app bundle contains game files" >&2; exit 1
fi

echo "== package"
STAGE="$ROOT/target/tvos/ipa"
OUT="$ROOT/target/tvos/OpenUnleashed.ipa"
mkdir -p "$STAGE"
find "$STAGE" -mindepth 1 -maxdepth 1 -name Payload -exec rm -rf {} +
mkdir -p "$STAGE/Payload"
cp -R "$APP" "$STAGE/Payload/"
# strip debug symbol files that only bloat the upload
find "$STAGE/Payload" -name '*.dSYM' -prune -exec rm -rf {} +
[ -f "$OUT" ] && mv -f "$OUT" "$OUT.prev"
(cd "$STAGE" && /usr/bin/zip -qry "$OUT" Payload)
/usr/bin/plutil -extract CFBundleIdentifier raw "$STAGE/Payload/OpenUnleashed.app/Info.plist" | sed 's/^/bundle id: /'
ls -lh "$OUT" | awk '{print "ipa: " $NF " (" $5 ")"}'
