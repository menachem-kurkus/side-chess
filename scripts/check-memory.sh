#!/bin/bash
# Launches Side Chess, makes it work (Claude plays a move, a screenshot is saved),
# and fails if macOS reports 100 MB or more at any point in a minute.
# The figure is the process's physical footprint: Activity Monitor's "Memory" column.
# Usage: scripts/check-memory.sh [path to the side-chess binary]
set -euo pipefail
LIMIT_MB=100
BIN="${1:-$HOME/.side-chess/build/release/side-chess}"
[ -x "$BIN" ] || { echo "No binary at $BIN: run app/install.sh first" >&2; exit 2; }

HOME_DIR=$(mktemp -d)               # keep the test game away from a real one
LIVE="$HOME_DIR/.side-chess"
# start with black to move, so the test can play Claude's side
HOME="$HOME_DIR" CHESS_FEN="rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1" "$BIN" >/dev/null 2>&1 &
PID=$!
trap 'kill $PID 2>/dev/null || true; wait $PID 2>/dev/null || true; rm -rf "$HOME_DIR"' EXIT

footprint_mb() {
  footprint -p "$PID" 2>/dev/null | awk '/Footprint:/ {
    n = $(NF-5); u = $(NF-4)
    if (u == "KB") n = n / 1024; else if (u == "GB") n = n * 1024
    printf "%d", n + 0.5; exit }'
}

worst=0
for i in $(seq 1 12); do            # one minute
  sleep 5
  kill -0 $PID 2>/dev/null || { echo "Side Chess exited early" >&2; exit 1; }
  [ "$i" -eq 2 ] && echo "c5" > "$LIVE/claude_move.txt"   # Claude moves
  [ "$i" -eq 3 ] && touch "$LIVE/shot"                    # screenshot
  mb=$(footprint_mb)
  echo "after $((i * 5))s: ${mb} MB"
  [ "$mb" -gt "$worst" ] && worst=$mb
done

grep -q "^moves: .*c5" "$LIVE/state.txt" || { echo "FAIL: the move file was not played" >&2; exit 1; }
if [ -f "$LIVE/screen.png" ]; then
  width=$(python3 -c "import struct,sys;d=open(sys.argv[1],'rb').read(24);print(struct.unpack('>I',d[16:20])[0])" "$LIVE/screen.png" 2>/dev/null || echo "?")
  echo "screen scale: $([ "$width" = "1200" ] && echo "2x (Retina)" || echo "1x or other (${width} px wide)")"
fi
if [ "$worst" -ge "$LIMIT_MB" ]; then
  echo "FAIL: peaked at ${worst} MB (limit ${LIMIT_MB} MB)"
  exit 1
fi
echo "PASS: peaked at ${worst} MB (limit ${LIMIT_MB} MB)"
