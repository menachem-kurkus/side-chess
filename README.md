# Side Chess

Play chess against your Claude Code session in a tiny Mac window while your other sessions work. You play white; Claude plays black.

- A native Rust window that stays under 100 MB of memory (about 35 MB at its peak on a Retina Mac) and redraws only when something changes.
- A **Launch Side Chess** button above the prompt (a Claude Code mod) opens the window and tells the session you've begun.
- A `side-chess` skill teaches Claude to install the app, watch the game and answer each move.

## Install

Needs macOS, [Rust](https://rustup.rs) and Claude Code.

At the prompt of a terminal session:

```
/plugin install side-chess --marketplace menachem-kurkus/side-chess
```

Answer `y` to add the marketplace, then pick a scope (user is first).

From a shell:

```bash
claude plugin install side-chess --marketplace menachem-kurkus/side-chess
```

Older Claude Code without `--marketplace`:

```bash
claude plugin marketplace add menachem-kurkus/side-chess
claude plugin install side-chess@side-chess
```

In the desktop app, where `/plugin` does not run: click + next to the prompt box, then Plugins, Add marketplace with `menachem-kurkus/side-chess`, then Add plugin.

The first time you click **Launch Side Chess**, Claude builds the app (`app/install.sh`, a minute or two) into `~/Applications/Side Chess.app`. You can also run that script yourself.

## Play

- Click a piece, then a square. Pawns promote to a queen.
- Or type a square's letter and number, in either order (half a square tints its row or column), then the square to move to, then Enter. Typing another of your pieces switches to it; illegal squares are ignored. Esc clears.
- U takes back your last move, N starts a new game, T switches the look (pack), P saves a screenshot.
- The board shows the last move in gold, a king in check in red, and a banner for checkmate, stalemate or a draw. There are no move hints.

## How it works

The app writes the position to `~/.side-chess/state.txt` after every move (whose turn, FEN, the move list, black's legal moves and a text board). Claude watches that file and answers by writing one move to `~/.side-chess/claude_move.txt`, which the app plays within a second. An illegal move shows a note and the app waits for another.

The window is drawn in plain memory, without the graphics card: a graphics driver alone costs 60 MB or more and differs from Mac to Mac, so this keeps memory low and predictable. Each frame is handed to macOS already in the screen's own colours (`app/src/present.rs`), so macOS has nothing to convert and keeps no converted copy; the pack's colours and pieces are converted once per screen instead. The pieces and text are drawn into small images at build time (`app/build.rs`) from fonts every Mac has, so nothing from Apple's fonts is shipped and no font is loaded while playing.

For testing, `CHESS_FEN="<fen>" ~/.side-chess/build/release/side-chess` starts from any position.

## Make your own look

Looks are **packs**: a folder in `app/packs/` with one text file of colours, and optionally your own piece images. Copy `app/packs/wood`, change what you like, run `bash app/install.sh`, and press T. Details: [app/packs/README.md](app/packs/README.md).

## Contributing

Pull requests are welcome. Tests, formatting, the plugin check and a memory check (under 100 MB) must pass; GitHub runs them on a Mac for every change. See [CONTRIBUTING.md](CONTRIBUTING.md).

## Licence

GPL-3.0-or-later (see `LICENSE`). Chess rules come from [shakmaty](https://github.com/niklasf/shakmaty) (GPL-3.0); the window uses [winit](https://github.com/rust-windowing/winit) and the [objc2](https://github.com/madsmtm/objc2) macOS bindings (Apache-2.0/MIT).
