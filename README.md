# weeb-tui

A Netflix-style TUI for watching anime and reading manga, all from the terminal. One binary, two halves glued together.

![Version](https://img.shields.io/badge/version-1.0.0-blue.svg)
![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-lightgrey.svg)
![License](https://img.shields.io/badge/license-MIT-green.svg)

## What this is

I'm a student. This is my first real project in Rust and my first time publishing something I built end-to-end.

I didn't write the anime half or the manga half from scratch. This project merges two existing codebases:

- **ani-tui** by [silent9669](https://github.com/silent9669) — the anime streaming foundation (cover art, watch history, multiple sources)
- **manga-tui** by [josueBarretogit](https://github.com/josueBarretogit) — the manga reading foundation (Mangadex, page preloading, reading progress)

Both projects shared the same bones: image rendering pipeline, SQLite database, key handling, similar UI. Maintaining two separate apps that did 80% the same thing felt pointless. So I merged them. Press `m` and you flip between anime mode and manga mode. Same binary, same config, same keybinds.

I called it **weeb-tui** because that's what it is. I built it because I wanted one tool that does both things — free anime streaming and manga reading — without leaving the terminal. It's not trying to compete with anything. It's just the tool I wanted for myself.

There are rough edges. I'm still learning Rust. But it works, and I use it daily.

### What it does

**Anime side** (from ani-tui)
- Stream anime and films from multiple sources (AllAnime, KKPhim, OPhim)
- English and Vietnamese subtitles
- Continue watching from where you stopped
- Cover image previews in the terminal

**Manga side** (from manga-tui)
- Read manga from Mangadex (WeebCentral and MangaPill also available via HTML scraping)
- Popular manga listing
- Reading history with auto-bookmark — remembers the chapter and page you were on, per manga
- Inline page rendering with preloading so turning pages isn't a slideshow

Both sides share the same image renderer, which supports Kitty, iTerm2, Sixel, and WezTerm graphics protocols, with a halfblock fallback so it still works in a plain terminal.


## Prerequisites

- **mpv** — needed for video playback on the anime side
  - macOS: `brew install mpv`
  - Windows: the installer handles it, or grab it yourself
  - Linux: `sudo apt install mpv`
- **chafa** (optional) — image previews in terminals that don't speak a graphics protocol natively
  - macOS: `brew install chafa`
  - Linux: `sudo apt install chafa`

## Usage

```bash
# just run it
weeb-tui

# jump straight into a search
weeb-tui -q "Attack on Titan"

# check for a newer release
weeb-tui --check-update

# update in place (if you installed the binary, not Homebrew/Scoop)
weeb-tui --update
```

## Key bindings

| Key       | Action                                  |
| --------- | --------------------------------------- |
| `↑/↓`     | Navigate lists                          |
| `Enter`   | Select / open / resume reading          |
| `Esc`     | Back                                    |
| `m`       | Toggle anime ↔ manga mode               |
| `Tab`     | Switch panel (Popular ↔ Last Read)      |
| `Shift+S` | Change source                           |
| `Shift+R` | View activity logs                      |
| `q`       | Quit                                    |

In the manga reader, `←/→` (or `h/l`, or space) turn pages, `w`/`b` jump chapters.

## Config

A config file lives in your platform's app data dir under `weeb-tui/config.toml`. Copy `ani-tui/config.toml.example` to get started and tweak sources, player, theme, and cache limits. See that file for the full set of options.

## Supported terminals

| Terminal               | Image support                              |
| ---------------------- | ------------------------------------------ |
| iTerm2 (macOS)         | Full                                       |
| Kitty                  | Full                                       |
| Warp                   | Full                                       |
| WezTerm                | Full                                       |
| Windows Terminal       | Halfblock fallback (stable, not pretty)    |
| Terminal.app           | Text only                                  |

On Windows, Kitty or WezTerm give you the nicest image previews. Windows Terminal falls back to halfblocks by default. You can force a renderer with `ANI_TUI_IMAGE_PROTOCOL=kitty|iterm2|sixel|halfblocks|auto` if autodetection picks wrong.

## Building from source

You need the Rust toolchain (rustup is the easy way) and a C compiler, since `rusqlite` builds SQLite from source.

```bash
git clone https://github.com/Al1enL1me67/weeb-tui.git
cd weeb-tui/ani-tui
cargo run
```

For a release build (what gets shipped):

```bash
cargo build --release
# the binary lands in target/release/weeb-tui
```

Linux needs OpenSSL headers for some of the HTTP stack: `sudo apt install libssl-dev pkg-config`. macOS and Windows generally just work.

### Cross-compiling

The release pipeline builds these targets, and you can build any of them locally once you add the target with `rustup target add`:

- `x86_64-apple-darwin` (Intel Macs)
- `aarch64-apple-darwin` (Apple Silicon)
- `x86_64-pc-windows-msvc` (Windows)
- `x86_64-unknown-linux-gnu` (Linux)

```bash
rustup target add aarch64-apple-darwin
cargo build --release --target aarch64-apple-darwin
```

## Releasing a new version

Releases are driven by git tags. Push a `v*` tag and GitHub Actions builds binaries for all four targets and publishes a release automatically. Here's the full routine.

**1. Bump the version** in `ani-tui/Cargo.toml`:

```toml
version = "1.1.0"
```

**2. Add a CHANGELOG entry** under a new `## [1.1.0] - YYYY-MM-DD` heading in `ani-tui/docs/CHANGELOG.md`.

**3. Commit and tag.**

```bash
git add ani-tui/Cargo.toml ani-tui/docs/CHANGELOG.md
git tag v1.1.0
```

**4. Push the tag.** This is what kicks off the build.

```bash
git push origin v1.1.0
```

**5. Watch the workflow.** Go to the **Actions** tab. The `Release` workflow will:
- build for `x86_64-apple-darwin`, `aarch64-apple-darwin`, Windows, and Linux
- zip/tar each binary
- create a GitHub Release tagged `v1.1.0` with all four archives attached
- (on tag pushes) update the Homebrew tap formula in `Al1enL1me67/homebrew-tap`

When it finishes, the Release page has downloadable archives. The install scripts in the README point at `releases/latest/download/...`, so once the tag is pushed, `brew`/Scoop/curl installs all pick up the new version.

**Manual release (if you ever need to).** Build each target, zip the binary, upload it to a draft GitHub Release, and publish. The archives need to be named `weeb-tui-<target>.zip` (or `.tar.gz` for Linux) to match what the installers expect.

## Credits

This project exists because of two existing codebases:

- **ani-tui** by [silent9669](https://github.com/silent9669) — the anime streaming foundation
- **manga-tui** by [josueBarretogit](https://github.com/josueBarretogit) — the manga reading foundation

I (Al1enL1me67) merged them, fixed bugs that showed up when combining them (manga mode toggle, cover image bleeding, reading past page 3-4), added the Last Read panel with auto-bookmark, and packaged it as one binary. The architecture — shared image pipeline, shared database, shared event loop — was already there in both projects. I just made them live in the same house.

## Documentation

- [Image rendering](ani-tui/IMAGE_RENDERING.md)

## Contributing

It's a hobby project by a student learning Rust. Issues and PRs are welcome, but I'm mostly building what I want to use. If you hit a bug, the `Shift+R` activity log usually tells you which provider or step choked, and that's the most useful thing to paste into an issue alongside what you did and what terminal you're on.

## License

MIT — see [LICENSE](LICENSE).

This project incorporates code from ani-tui (MIT, silent9669) and manga-tui (MIT, josueBarretogit). Their licenses are preserved in the respective source directories.