# Packs

A pack is one look for Side Chess: a folder here with a `pack.txt`. Every folder is built into the app; **T** in the window switches between them and the choice is remembered.

## Make your own

1. Copy a pack: `cp -r wood my-pack`.
2. Edit `my-pack/pack.txt`. Change `name` and any colours (ordinary #RRGGBB, as on the web; the app matches them to each screen itself). A pack lists only what it changes: anything left out comes from `blue-gold/pack.txt`, which lists every key with a comment.
   - Colours are `#RRGGBB`, or `#RRGGBBAA` to be see-through (`AA` from `00` clear to `FF` solid).
   - Lines starting with `#` are comments.
3. Rebuild: `bash app/install.sh`. The tests check that every pack loads (`cargo test`).

## Your own pieces

By default a pack's pieces are drawn at build time from `piece_font` (Apple Symbols), filled with `piece_white` and `piece_black`.

To use your own art instead, add `pieces.png` to the pack folder:
- 768 x 256 pixels, transparent background
- six 128 x 128 tiles across: King, Queen, Rook, Bishop, Knight, Pawn
- white pieces in the top row, black in the bottom row

Only use art you made or that's licensed for sharing, and say where it came from in a `CREDITS.md` in the pack folder.

## What each colour is for

| Key | Used for |
|---|---|
| `background`, `accent`, `title`, `text`, `muted`, `error` | The window, the bar beside the title, the title, the move list, quiet text, a note about an illegal move |
| `status_your_move`, `status_waiting`, `status_over` | The status at the top right |
| `board_border`, `light_square`, `dark_square` | The board |
| `last_from`, `last_to`, `last_outline` | The last move: where it left, where it landed, its outline |
| `selected` | Your selected piece |
| `target`, `target_outline` | The move waiting for Enter |
| `half_typed` | The row or column after typing half a square |
| `check`, `check_outline` | A king in check |
| `shade`, `banner`, `banner_title`, `banner_result` | The game-over banner |
