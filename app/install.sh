#!/bin/bash
# Builds Side Chess and installs it as ~/Applications/Side Chess.app (macOS),
# or as ~/.local/bin/side-chess with a menu entry (Linux).
set -euo pipefail
cd "$(dirname "$0")"
if ! command -v cargo >/dev/null 2>&1; then
  echo "Side Chess needs Rust to build. Install it from https://rustup.rs, then run this again." >&2
  exit 1
fi
if [ "$(uname)" = Linux ]; then
  # The window needs the keyboard library (libxkbcommon-x11 on X11, libxkbcommon
  # on Wayland); without it the app crashes at launch.
  if ! ldconfig -p 2>/dev/null | grep -q 'libxkbcommon-x11\.so\.0'; then
    echo "Side Chess needs libxkbcommon-x11 to open its window on Linux. Install it, then run this again:" >&2
    echo "  Debian/Ubuntu: sudo apt install libxkbcommon-x11-0" >&2
    echo "  Fedora:        sudo dnf install libxkbcommon-x11" >&2
    echo "  Arch:          sudo pacman -S libxkbcommon-x11" >&2
    exit 1
  fi
fi
export CARGO_TARGET_DIR="$HOME/.side-chess/build"
cargo build --release
if [ "$(uname)" = Linux ]; then
  BIN="$HOME/.local/bin/side-chess"
  mkdir -p "$HOME/.local/bin" "$HOME/.local/share/applications" "$HOME/.side-chess"
  cp "$CARGO_TARGET_DIR/release/side-chess" "$BIN.new" && mv -f "$BIN.new" "$BIN"
  printf '[Desktop Entry]\nType=Application\nName=Side Chess\nComment=Play chess against your AI session\nExec=%s\nTerminal=false\nCategories=Game;BoardGame;\n' "$BIN" \
    > "$HOME/.local/share/applications/side-chess.desktop"
  sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1 > "$HOME/.side-chess/app_version.txt"
  echo "Installed: $BIN"
  exit 0
fi
APP="$HOME/Applications/Side Chess.app"
mkdir -p "$APP/Contents/MacOS"
cp "$CARGO_TARGET_DIR/release/side-chess" "$APP/Contents/MacOS/side-chess"
cp Info.plist "$APP/Contents/Info.plist"
mkdir -p "$APP/Contents/Resources"
cp icon/icon.icns "$APP/Contents/Resources/icon.icns"
# The installed version, so the skills can tell when the app is out of date
mkdir -p "$HOME/.side-chess"
sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1 > "$HOME/.side-chess/app_version.txt"
echo "Installed: $APP"
