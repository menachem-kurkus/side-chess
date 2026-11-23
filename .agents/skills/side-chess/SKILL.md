---
name: side-chess
description: Play as the black opponent in the Side Chess desktop app when the user starts or continues a Side Chess game.
---

# Side Chess

The user plays white in the Side Chess window. You play black when asked to play or continue. The app stores the current position in `~/.side-chess/state.txt` (on Windows, `%USERPROFILE%\.side-chess\state.txt`) and reads your move from `opponent_move.txt` in that same folder. This skill expects app version 0.2.2 or later. No API key or network connection is needed during play.

## Check the app version

When a game starts, read `~/.side-chess/app_version.txt` (written by the install script). If it is missing or older than 0.2.2, the installed app is out of date: it says `black (Claude)` instead of `black (AI)` and does not read `opponent_move.txt`. Tell the user, and offer to rebuild it with the repository's `app/install.sh` (macOS or Linux) or `powershell -ExecutionPolicy Bypass -File app\install.ps1` (Windows) if the repository is available; quit Side Chess on a Mac first. On Linux the app needs libxkbcommon-x11 (libxkbcommon on Wayland); if the install script or the app says it is missing, give the user the command it prints, such as `sudo apt install libxkbcommon-x11-0 fonts-liberation fonts-dejavu-core`. Until it is updated, write your moves to `claude_move.txt` instead of `opponent_move.txt`, since every version of the app reads that file.

## Take a turn

1. Read the full `state.txt` whenever the user starts a game or asks for your move. Use the current `fen:`, move list, board, and `legal:` list; never rely on a position from an earlier turn.
2. If `turn:` is `game over: ...`, report the result. If it is `white (player)`, tell the user it is their turn. Play only when the line starts with `turn: black` (`black (AI)`, or `black (Claude)` from apps before 0.2.2).
3. Choose a legal move from `legal:`. Play sound, friendly chess at a club level. Prefer a move that responds to the user's threats and leaves your king safe.
4. Write exactly one SAN move, such as `Nf6`, to `~/.side-chess/opponent_move.txt`. On Windows, use an ASCII write so the file has no byte-order mark:

   ```powershell
   $path = Join-Path $env:USERPROFILE '.side-chess\opponent_move.txt'
   [IO.File]::WriteAllText($path, 'Nf6', [Text.Encoding]::ASCII)
   ```

   On macOS or Linux:

   ```sh
   printf 'Nf6\n' > ~/.side-chess/opponent_move.txt
   ```

5. Read `state.txt` again to confirm the move was accepted. If `note:` says the move was illegal, choose a move from the refreshed `legal:` list and try once more. Then give the user one short line with your move and a brief reason.

The state folder is outside most project workspaces. If file access is denied, request access to that folder through the normal approval flow. Do not copy game state into the project or attempt to bypass access controls.

Codex does not watch the game while a turn is idle. After the user makes a move, they can ask “your move” or “continue Side Chess” to have you read the new position and reply.
