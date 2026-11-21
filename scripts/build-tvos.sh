#!/bin/sh
# One-command Apple TV build (TVOS.md): cargo (engine static library) → XcodeGen → xcodebuild → install.
#
#   scripts/build-tvos.sh sim            # build for the tvOS Simulator, install + launch on a booted/created simulator
#   scripts/build-tvos.sh sim --data     # …and copy ./game-data into the simulator app's Caches/game first
#   scripts/build-tvos.sh device         # build for a real Apple TV (needs OU_TEAM_ID; installs to the paired device)
#
# Environment: OU_BUNDLE_ID (default org.openunleashed.tvos), OU_TEAM_ID (signing team, device builds),
# OU_SIM_NAME (default "Apple TV 4K (4th generation)"), OU_DEVICE (device id for `devicectl`, default: the first Apple TV).
# GPL-3.0-or-later.
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
[ -x "$HOME/homebrew/bin/brew" ] && eval "$("$HOME/homebrew/bin/brew" shellenv)"

KIND="${1:-sim}"
COPY_DATA=0
[ "${2:-}" = "--data" ] && COPY_DATA=1
: "${OU_BUNDLE_ID:=org.openunleashed.tvos}"
: "${OU_TEAM_ID:=}"
: "${OU_SIM_NAME:=Apple TV 4K (4th generation)}"
export OU_BUNDLE_ID OU_TEAM_ID
# the Rust objects and SDL (cc/cmake) must target the app's minimum tvOS, not the SDK's version
export TVOS_DEPLOYMENT_TARGET=17.0

case "$KIND" in
  sim) TRIPLE=aarch64-apple-tvos-sim; SDK=appletvsimulator; DEST="generic/platform=tvOS Simulator" ;;
  device) TRIPLE=aarch64-apple-tvos; SDK=appletvos; DEST="generic/platform=tvOS" ;;
  *) echo "usage: $0 sim|device [--data]" >&2; exit 2 ;;
esac

echo "== cargo: engine static library for $TRIPLE (release)"
CARGO_TARGET_DIR="$ROOT/target/tvos" cargo rustc --release --locked --target "$TRIPLE" -p open-unleashed --lib --crate-type staticlib

echo "== xcodegen"
platform/tvos/configure.sh

echo "== xcodebuild ($SDK)"
DERIVED="$ROOT/target/tvos/xcode"
SIGN=""
if [ "$KIND" = sim ]; then
  SIGN="CODE_SIGNING_ALLOWED=NO"
elif [ -z "$OU_TEAM_ID" ]; then
  echo "device build: set OU_TEAM_ID to your signing team (Xcode › Settings › Accounts)" >&2; exit 2
fi
xcodebuild -project platform/tvos/OpenUnleashed.xcodeproj -scheme OpenUnleashed -configuration Release \
  -sdk "$SDK" -destination "$DEST" -derivedDataPath "$DERIVED" -allowProvisioningUpdates $SIGN build | \
  grep -E '^(\*\*|error|warning: .*OpenUnleashed|ld: )' || true
APP="$DERIVED/Build/Products/Release-$SDK/OpenUnleashed.app"
[ -d "$APP" ] || { echo "build failed: no $APP" >&2; exit 1; }
echo "built $APP"

if [ "$KIND" = sim ]; then
  UDID=$(xcrun simctl list devices available | grep -F "$OU_SIM_NAME (" | head -1 | sed -E 's/.*\(([0-9A-F-]{36})\).*/\1/')
  if [ -z "$UDID" ]; then
    RUNTIME=$(xcrun simctl list runtimes | grep -o 'com.apple.CoreSimulator.SimRuntime.tvOS-[0-9-]*' | tail -1)
    UDID=$(xcrun simctl create "$OU_SIM_NAME" "com.apple.CoreSimulator.SimDeviceType.Apple-TV-4K-4th-generation-4K" "$RUNTIME")
  fi
  xcrun simctl boot "$UDID" 2>/dev/null || true
  open -a "$(xcode-select -p)/Applications/Simulator.app" --args -CurrentDeviceUDID "$UDID" || true
  xcrun simctl install "$UDID" "$APP"
  if [ "$COPY_DATA" = 1 ]; then
    DATA=$(xcrun simctl get_app_container "$UDID" "$OU_BUNDLE_ID" data)
    mkdir -p "$DATA/Library/Caches/game"
    echo "== copying game-data/ into the simulator's Caches/game (never into git, never into the app bundle)"
    rsync -a --delete "$ROOT/game-data/" "$DATA/Library/Caches/game/"
  fi
  xcrun simctl terminate "$UDID" "$OU_BUNDLE_ID" 2>/dev/null || true
  xcrun simctl launch "$UDID" "$OU_BUNDLE_ID"
  echo "launched on simulator $UDID"
else
  DEV="${OU_DEVICE:-$(xcrun devicectl list devices 2>/dev/null | awk '/Apple TV/ {print $3; exit}')}"
  [ -n "$DEV" ] || { echo "no paired Apple TV found (xcrun devicectl list devices)" >&2; exit 1; }
  xcrun devicectl device install app --device "$DEV" "$APP"
  xcrun devicectl device process launch --device "$DEV" "$OU_BUNDLE_ID"
fi
