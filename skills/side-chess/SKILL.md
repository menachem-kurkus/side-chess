---
name: side-chess
description: Play black against the user in Side Chess, a tiny Mac chess window, or install it. Use when the user clicks Launch Side Chess, says they have begun a Side Chess game, or asks to play chess, install Side Chess, or watch the chess game.
---

# Side Chess

The user plays white in the Side Chess window. You play black by reading one file and writing another. Everything lives in `~/.side-chess/`.

## Install (only if it isn't installed)

`~/Applications/Side Chess.app` missing means it isn't installed. Build it with the plugin's script (needs macOS and Rust; the first build takes a minute or two):

```bash
bash "<plugin root>/app/install.sh"
```

The launch button's message names the plugin root. Without it, find the script with `find ~/.claude/plugins -path "*side-chess*/app/install.sh" | head -1`. If Rust is missing, tell the user to install it from https://rustup.rs and stop.

Then open it: `open -a "Side Chess"`.

## The files

- `~/.side-chess/state.txt`, written by the app after every move:
  - `turn:` is `white (player)`, `black (Claude)` or `game over: ...`
  - `fen:`, `check:`, `moves:` (the game so far), `note:` (an error about your last move, if any)
  - `legal:` every legal move for the side to move, in standard notation
  - a text board (uppercase = white, lowercase = black)
- `~/.side-chess/claude_move.txt`: write exactly one move here, like `Nf6` or `g8f6`. The app plays it within a second and deletes the file.
- `touch ~/.side-chess/shot` makes the app save a screenshot to `~/.side-chess/screen.png` if you want to see the board.

## Watching the game

Keep exactly one watch running. Start it with the Monitor tool (30 minute timeout, re-arm when it expires while the game is still going):

```bash
F=~/.side-chess/state.txt; last=""
while true; do
  t=$(head -1 "$F" 2>/dev/null); m=$(sed -n 4p "$F" 2>/dev/null)
  if [[ "$t" != "turn: white (player)" && "$t$m" != "$last" ]]; then echo "$t | $m"; last="$t$m"; fi
  sleep 1
done
```

If Monitor isn't available, use a background Bash command that exits when the turn line becomes `turn: black (Claude)`, and start it again after each move.

## Each of your turns

1. Read `~/.side-chess/state.txt` in full.
2. Pick a move from the `legal:` list only. Play real chess: look at what the user's last move threatens, check captures and checks for both sides, keep your king safe, then develop and attack. Play to win, but play at a friendly club level, not engine level.
3. Write it: `echo "Nf6" > ~/.side-chess/claude_move.txt`.
4. Read `state.txt` again a second later. If `note:` says your move was not legal, pick another from `legal:`.
5. Say one short line to the user, like "Nf6, covering e4." No long analysis unless asked.

## Game over

When `turn:` starts with `game over:`, say the result in one line. Keep watching: the user presses N in the window for a new game, and the turn goes back to white.

## Keys in the window (for the user)

Click a piece then a square, or type a square's letter and number (either order; half a square tints its row or column), then the target square, then Enter. Esc clears, U takes back, N starts a new game, T switches the look (pack), P saves a screenshot.
