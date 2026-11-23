# Contributing to Side Chess

Thanks for helping. Anyone can open an issue or a pull request.

## The rules

1. **Every change goes in as a pull request.** Nobody pushes to `main` directly; the owner reviews and merges.
2. **All checks must pass** before a pull request is merged. GitHub runs them on a Mac, on Windows and on Linux (tests and the plugin check only) for every push (`.github/workflows/checks.yml`). Run them yourself first:

   ```bash
   cd app && cargo fmt --check                                   # formatting
   CARGO_TARGET_DIR="$HOME/.side-chess/build" cargo test --release  # tests
   cd .. && bash app/install.sh && scripts/check-memory.sh       # memory
   claude plugin validate .                                      # plugin, mod and skill
   ```

   On Windows (PowerShell), from the repo folder:

   ```powershell
   cd app; cargo fmt --check; cargo test --release; cd ..
   powershell -ExecutionPolicy Bypass -File app\install.ps1
   powershell -ExecutionPolicy Bypass -File scripts\check-memory.ps1 -Limit 30
   ```

   If cargo cannot download crates and reports a revocation error (some proxies and antivirus), set `CARGO_HTTP_CHECK_REVOKE=false` for that shell.

3. **Memory stays under 100 MB.** `scripts/check-memory.sh` launches the app, has Claude play a move and saves a screenshot, and fails if macOS reports 100 MB or more at any point in a minute (Activity Monitor's "Memory" column). It measures about 35 MB at its peak on an Apple silicon Mac with a Retina screen. On Windows `scripts/check-memory.ps1` does the same with the private working set (Task Manager's "Memory" column): about 8 MB on Windows 11, and CI holds it under 30 MB. Keep that headroom:
   - Draw with `canvas.rs` (plain memory). Don't add a GPU or graphics library: the graphics driver alone costs 60 MB or more and varies by machine.
   - Draw straight into the window's buffer; don't keep a second copy of the screen (each copy is about 7 MB on a Retina screen).
   - Hand frames to macOS in the screen's own colours (`present.rs`). Colours that reach the canvas must come from the pack, converted with `load_pack_for`; a colour written straight into the code would show slightly wrong on wide-colour screens.
   - Don't load font files at run time. Text and pieces are baked into images by `app/build.rs`.
   - Redraw only when something changes (see `about_to_wait` in `main.rs`).
4. **New behaviour comes with a test.** Rules and input live in `game.rs`, looks in `pack.rs`; both have tests at the bottom.
5. **Keep the board's hints to the selected piece.** The board highlights the selected piece and its legal moves (faded dots, a ring on a capture), the half-typed row or column, the move waiting for Enter, the last move and check. It never suggests which move is best.
6. **Licence.** Side Chess is GPL-3.0-or-later (its chess rules library, shakmaty, is GPL). By sending a pull request you agree your change is shared under the same licence. Don't paste code from projects with incompatible licences.
7. **Only things you may share.** No keys, tokens or personal data. Images in a pack must be your own or under a licence that allows sharing (say which in the pack's folder).

## Where things are

| Path | What it is |
|---|---|
| `app/src/game.rs` | Rules, typed and clicked moves, the files shared with the AI (`state.txt`, the move files) |
| `app/src/draw.rs` | What the window shows (layout) |
| `app/src/pack.rs` | Loading a pack's colours and pieces |
| `app/src/canvas.rs` | Drawing in plain memory |
| `app/src/present.rs` | Showing frames in the screen's own colours; converting pack colours per screen |
| `app/src/text.rs` | Text from the baked atlas |
| `app/src/main.rs` | The window, keys and mouse |
| `app/build.rs` | Bakes pieces and text from the system's fonts at build time |
| `app/packs/` | Looks ("packs"); see `app/packs/README.md` |
| `hooks/register.tsx` | The Launch Side Chess button (a Claude Code mod) |
| `skills/side-chess/SKILL.md` | How Claude installs the app and plays |
| `.agents/skills/side-chess/SKILL.md` | How Codex plays (same files, one move per request) |
| `scripts/check-memory.sh` | The memory check |

## Adding a look

See `app/packs/README.md`. A pack is a folder with one text file; no code changes.
