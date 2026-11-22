# Contributing to Side Chess

Thanks for helping. Anyone can open an issue or a pull request.

## The rules

1. **Every change goes in as a pull request.** Nobody pushes to `main` directly; the owner reviews and merges.
2. **All checks must pass** before a pull request is merged. GitHub runs them on a Mac for every push (`.github/workflows/checks.yml`). Run them yourself first:

   ```bash
   cd app && cargo fmt --check                                   # formatting
   CARGO_TARGET_DIR="$HOME/.side-chess/build" cargo test --release  # tests
   cd .. && bash app/install.sh && scripts/check-memory.sh       # memory
   claude plugin validate .                                      # plugin, mod and skill
   ```

3. **Memory stays under 100 MB.** `scripts/check-memory.sh` launches the app, has Claude play a move and saves a screenshot, and fails if macOS reports 100 MB or more at any point in a minute (Activity Monitor's "Memory" column). It measures about 50 MB at its peak on an Apple silicon Mac with a Retina screen. Keep that headroom:
   - Draw with `canvas.rs` (plain memory). Don't add a GPU or graphics library: the graphics driver alone costs 60 MB or more and varies by Mac.
   - Draw straight into the window's buffer; don't keep a second copy of the screen (each copy is about 7 MB on a Retina screen).
   - Don't load font files at run time. Text and pieces are baked into images by `app/build.rs`.
   - Redraw only when something changes (see `about_to_wait` in `main.rs`).
4. **New behaviour comes with a test.** Rules and input live in `game.rs`, looks in `pack.rs`; both have tests at the bottom.
5. **Keep the board free of move hints.** The board highlights only the selected piece, the half-typed row or column, the move waiting for Enter, the last move and check.
6. **Licence.** Side Chess is GPL-3.0-or-later (its chess rules library, shakmaty, is GPL). By sending a pull request you agree your change is shared under the same licence. Don't paste code from projects with incompatible licences.
7. **Only things you may share.** No keys, tokens or personal data. Images in a pack must be your own or under a licence that allows sharing (say which in the pack's folder).

## Where things are

| Path | What it is |
|---|---|
| `app/src/game.rs` | Rules, typed and clicked moves, the files shared with Claude |
| `app/src/draw.rs` | What the window shows (layout) |
| `app/src/pack.rs` | Loading a pack's colours and pieces |
| `app/src/canvas.rs` | Drawing in plain memory |
| `app/src/text.rs` | Text from the baked atlas |
| `app/src/main.rs` | The window, keys and mouse |
| `app/build.rs` | Bakes pieces and text from the Mac's own fonts at build time |
| `app/packs/` | Looks ("packs"); see `app/packs/README.md` |
| `hooks/register.tsx` | The Launch Side Chess button (a Claude Code mod) |
| `skills/side-chess/SKILL.md` | How Claude installs the app and plays |
| `scripts/check-memory.sh` | The memory check |

## Adding a look

See `app/packs/README.md`. A pack is a folder with one text file; no code changes.
