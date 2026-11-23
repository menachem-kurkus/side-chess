---
name: side-chess
description: Play against the user in Side Chess, a tiny Mac, Windows or Linux chess window, or install it. Use when the user clicks Launch Side Chess, says they have begun a Side Chess game, or asks to play chess, install Side Chess, or watch the chess game.
---

# Side Chess

The user plays white in the Side Chess window by default, and you play black; the user can press S to swap between games (never mid-game), so you play white and the user plays black. You play whichever side `state.txt` marks `(AI)`, by reading one file and writing another. Everything lives in `~/.side-chess/` (on Windows that is `%USERPROFILE%\.side-chess`, the same folder from Git Bash).

## Install or update

Check this every time a game starts. On a Mac the app is `~/Applications/Side Chess.app`; on Windows it is `%LOCALAPPDATA%\Programs\Side Chess\side-chess.exe`; on Linux it is `~/.local/bin/side-chess`. The install script records the installed version in `~/.side-chess/app_version.txt`; the plugin's version is the `version =` line of `<plugin root>/app/Cargo.toml`. Run the install script when the app is missing, when `app_version.txt` is missing (apps before 0.2.2 don't write it), or when the two versions differ. Tell the user in one line that you are updating it. Build with the plugin's script (needs Rust; the first build takes a minute or two):

```bash
bash "<plugin root>/app/install.sh"                                                  # macOS or Linux
powershell -NoProfile -ExecutionPolicy Bypass -File "<plugin root>/app/install.ps1"  # Windows
```

The launch button's message names the plugin root. Without it, find the script with `find ~/.claude/plugins -path "*side-chess*/app/install.*" | head -2`. If Rust is missing, tell the user to install it from https://rustup.rs (on Windows it also needs the Visual Studio C++ Build Tools) and stop. On Linux the app needs libxkbcommon-x11 (libxkbcommon on Wayland) plus the Liberation Sans and DejaVu Sans fonts; if the script or the build says one is missing, give the user the install command it prints (for example `sudo apt install libxkbcommon-x11-0 fonts-liberation fonts-dejavu-core`) and stop.

On Windows the script closes a running copy first. On a Mac, quit a running copy (`osascript -e 'quit app "Side Chess"'`) before updating. Then open it: `open -a "Side Chess"` on a Mac, on Windows `powershell -NoProfile -Command Start-Process "$env:LOCALAPPDATA\Programs\Side Chess\side-chess.exe"`, or on Linux `nohup ~/.local/bin/side-chess >/dev/null 2>&1 &`.

## The files

- `~/.side-chess/state.txt`, written by the app after every move:
  - `turn:` is `white (player)`, `black (AI)`, `black (player)`, `white (AI)` or `game over: ...`. It is your turn whenever it says `(AI)` (apps before 0.2.2 say `black (Claude)`, also your turn). Before 0.3.0 you were always black; if the app is older than 0.3.0, play black and tell a user who wants to swap to update the app
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
  t=$(head -1 "$F" 2>/dev/null); m=$(sed -n 4p "$F" 2>/dev/null); n=$(sed -n 5p "$F" 2>/dev/null)
  if [[ "$t" != *"(player)"* && "$t$m$n" != "$last" ]]; then echo "$t | $m | $n"; last="$t$m$n"; fi
  sleep 1
done
```

The `note:` part matters: when a move of yours is rejected the moves line does not change, only the note does, so without it the watch stays silent and the game stalls waiting for you. If an event arrives for a turn you already answered and its note is not empty, your move was refused: pick again from `legal:`.

If Monitor isn't available, use a background Bash command that exits when the turn line contains `(AI)`, and start it again after each move.

## Each of your turns

1. Read `~/.side-chess/state.txt` in full, every turn, and work from its `fen:` and text board. Never from your memory of earlier moves: that is how a piece you think you still have turns out to be gone.
2. Pick a move from the `legal:` list only. Play real chess: look at what the user's last move threatens, check captures and checks for both sides, keep your king safe, then develop and attack. Play to win, but play at a friendly club level, not engine level.
3. Write it: `echo "Nf6" > ~/.side-chess/claude_move.txt`.
4. Read `state.txt` again a second later. If `note:` says your move was not legal, pick another from `legal:` straight away (and tell the user in a few words that you misjudged the position).
5. Say one short line to the user, like "Nf6, covering e4." No long analysis unless asked.

## Game over

When `turn:` starts with `game over:`, say the result in one line. Keep watching: the user presses N in the window for a new game (same sides), or S to swap sides and start a new game (S only works on a new or finished game; mid-game the app refuses it with a message in its window, and nothing changes for you). If the AI is white, you move first, so `turn:` is `white (AI)` straight away and the watch fires at once.

## Keys in the window (for the user)

Click a piece then a square, or type a square's letter and number (either order; half a square tints its row or column), then the target square, then Enter. Esc clears, U takes back, N starts a new game, S swaps sides (you play black, the AI white) and starts a new game, T switches the look (pack), P saves a screenshot, K and L open, close or bring back the two thin side windows beside the board (keys on the left, look on the right).
