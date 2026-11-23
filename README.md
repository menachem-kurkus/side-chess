# Side Chess

Play chess against your AI assistant in a tiny window on Mac, Windows or Linux while your other sessions work. You play white; Claude Code or Codex plays black.

- A native Rust window that stays under 100 MB of memory (about 35 MB at its peak on a Retina Mac; about 8 MB private, 23 MB at its peak, on Windows 11) and redraws only when something changes.
- **Claude Code:** a **Launch Side Chess** button above the prompt (a Claude Code mod) opens the window and tells the session you've begun, and the `side-chess` skill teaches Claude to install the app, watch the game and answer each move.
- **Codex:** a Codex skill (`.agents/skills/side-chess`) plays from a Codex chat through the same game files, one move each time you ask.

## Install

The app is built on your computer, so it needs [Rust](https://rustup.rs) and:

- **macOS:** nothing else.
- **Windows 10/11:** the Visual Studio C++ Build Tools.
- **Linux:** the keyboard library libxkbcommon-x11 (without it the window can't open) and the fonts the pieces and text are drawn from, Liberation Sans and DejaVu Sans. On Debian or Ubuntu: `sudo apt install libxkbcommon-x11-0 fonts-liberation fonts-dejavu-core`. On Fedora: `sudo dnf install libxkbcommon-x11 liberation-sans-fonts dejavu-sans-fonts`. On Arch: `sudo pacman -S libxkbcommon-x11 ttf-liberation ttf-dejavu`. On Wayland the window uses libxkbcommon, which desktops already have.

### Claude Code

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

The first time you click **Launch Side Chess**, Claude builds the app (a minute or two).

### The app on its own

The install scripts build and install the app; Claude runs them for you, and you can run them yourself:

- **macOS:** `bash app/install.sh` puts it in `~/Applications/Side Chess.app`.
- **Windows:** `powershell -ExecutionPolicy Bypass -File app\install.ps1` puts it in `%LOCALAPPDATA%\Programs\Side Chess` and adds a Start Menu shortcut. It closes a running copy first.
- **Linux:** `bash app/install.sh` puts it in `~/.local/bin/side-chess` and adds a menu entry. It stops with the command to run if libxkbcommon-x11 is missing, and the app itself says the same instead of crashing.

Each script records the installed version in `~/.side-chess/app_version.txt`. Both skills read it when a game starts and rebuild (Claude) or offer to rebuild (Codex) an app that is older than they are.

### Codex

Codex finds the skill on its own when you open it in this repository. To use it anywhere, copy `.agents/skills/side-chess` into your Codex skills folder (`%USERPROFILE%\.agents\skills\side-chess` on Windows, or `~/.agents/skills/side-chess` on macOS and Linux), then restart Codex. Install the app first (above), open Side Chess from Applications, the Start Menu or your Linux app menu, and ask Codex to play Side Chess. If Codex asks for access, allow it to read and write `~/.side-chess` (on Windows, `%USERPROFILE%\.side-chess`). No API key is needed. After each of your moves, ask Codex "your move" or "continue Side Chess"; Codex does not run in the background between turns.

## Play

- Click a piece, then a square. Pawns promote to a queen.
- Or type a square's letter and number, in either order (half a square tints its row or column), then the square to move to, then Enter. Typing another of your pieces switches to it; illegal squares are ignored. Esc clears.
- Ctrl+Q (Cmd+Q on a Mac) quits, U takes back your last move, N starts a new game, T switches the look (pack), P saves a screenshot.
- The board shows the last move in gold, a king in check in red, and a banner for checkmate, stalemate or a draw. Pick a piece and faded dots show where it can go (a ring on a piece it can take). Castling (click the king, then g or c) and en passant work.

## How it works

The app writes the position to `~/.side-chess/state.txt` after every move (on Windows `~` is `%USERPROFILE%`): whose turn (`turn: black (AI)` when it's the assistant's; apps before 0.2.2 said `black (Claude)`), FEN, the move list, black's legal moves and a text board. Claude Code writes its reply to `claude_move.txt`; Codex writes to `opponent_move.txt`. The app accepts either file and plays the move within a second. Any assistant that can read and write files can play the same way.

The move file can be plain UTF-8, UTF-8 with a byte-order mark, or UTF-16 (what Windows PowerShell 5 writes with `>`). An illegal move, or a file that isn't text, is removed and shows a note, and the app waits for another move rather than stalling. A file that is still empty is given two seconds for the move to arrive.

The window is drawn in plain memory, without the graphics card: a graphics driver alone costs 60 MB or more and differs from machine to machine, so this keeps memory low and predictable. Each frame is handed to macOS already in the screen's own colours (`app/src/present.rs`), so macOS has nothing to convert and keeps no converted copy; the pack's colours and pieces are converted once per screen instead. On Windows the frame goes straight into the window's own buffer (`softbuffer`), which needs no colour conversion. The pieces and text are drawn into small images at build time (`app/build.rs`) from fonts every Mac and Windows PC has (Apple Symbols and Arial, or Segoe UI Symbol and Arial; on Linux, DejaVu Sans and Liberation Sans), so nothing from Apple's or Microsoft's fonts is shipped and no font is loaded while playing. On Linux the frame goes into the window's buffer the same way as on Windows.

For testing, `CHESS_FEN="<fen>" ~/.side-chess/build/release/side-chess` starts from any position.

## Make your own look

Looks are **packs** (press T to switch; `No Crosses` is the Blue and Gold look with no crosses on the king or bishop): a folder in `app/packs/` with one text file of colours, and optionally your own piece images. Copy `app/packs/wood`, change what you like, run the install script (`bash app/install.sh` on a Mac or Linux, `app\install.ps1` on Windows), and press T. Details: [app/packs/README.md](app/packs/README.md).

## Contributing

Pull requests are welcome. Tests, formatting, the plugin check and a memory check (under 100 MB) must pass; GitHub runs them on a Mac, on Windows and on Linux (tests and the plugin check only) for every change. See [CONTRIBUTING.md](CONTRIBUTING.md).

## Licence

GPL-3.0-or-later (see `LICENSE`). Chess rules come from [shakmaty](https://github.com/niklasf/shakmaty) (GPL-3.0); the window uses [winit](https://github.com/rust-windowing/winit) and the [objc2](https://github.com/madsmtm/objc2) macOS bindings (Apache-2.0/MIT).
