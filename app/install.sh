#!/bin/bash
# Builds Side Chess and installs it as ~/Applications/Side Chess.app (macOS).
set -euo pipefail
cd "$(dirname "$0")"
if ! command -v cargo >/dev/null 2>&1; then
  echo "Side Chess needs Rust to build. Install it from https://rustup.rs, then run this again." >&2
  exit 1
fi
export CARGO_TARGET_DIR="$HOME/.side-chess/build"
cargo build --release
APP="$HOME/Applications/Side Chess.app"
mkdir -p "$APP/Contents/MacOS"
cp "$CARGO_TARGET_DIR/release/side-chess" "$APP/Contents/MacOS/side-chess"
cp Info.plist "$APP/Contents/Info.plist"
mkdir -p "$APP/Contents/Resources"
cp icon/icon.icns "$APP/Contents/Resources/icon.icns"
echo "Installed: $APP"
