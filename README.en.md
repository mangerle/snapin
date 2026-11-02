# Snapin

Snapin is a pure-Rust screenshot and pinboard utility for Linux, Windows, and macOS.

The screenshot overlay uses winit plus tiny-skia software rasterization. On Linux, the GNOME Shell extension provides native capture, precise pin placement, tray integration, and optional cursor capture. Windows and macOS use native screen capture and global hotkeys.

## Features

- Area, window, fullscreen, delayed, and color-picker capture
- Fixed ratio and fixed-size selection, window cycling, and pixel nudging
- Rectangle, ellipse, arrow, pen, highlighter, text, badge, mosaic, blur, and spotlight annotations
- Editable latest annotation, undo/redo, selection history, and annotation toolbar tooltips
- Local OCR with PP-OCRv6 and editable OCR result rows
- Smart redaction preview for all recognized text, with mosaic or blur rendering
- QR detection with safe HTTP/HTTPS link opening
- Pinboard workspace with groups, scaling, rotation, opacity, mouse-through, OCR, and translation
- Clipboard history for text, colors, and images
- Configurable beautified output with rounded corners, shadows, gradients, watermark, and social sizes
- 10 configurable global shortcuts
- Chinese and English user interface
- Slint settings center and strict TOML configuration import/export

## Installation

Download the latest package from [Releases](https://github.com/mangerle/snapin/releases). Debian packages are recommended on Linux.

For source builds:

```bash
cargo build --release
```

Linux build dependencies are listed in [README.md](README.md). Install the GNOME extension with:

```bash
snapin install-extension
```

After installing or upgrading the extension, sign out and back in once.

## Usage

Default global shortcuts:

| Action | Shortcut |
| --- | --- |
| Area capture | `Ctrl+Alt+A` |
| Capture translation | `Ctrl+Alt+D` |
| Pin clipboard | `F1` |
| Toggle pins | `Ctrl+F3` |
| Switch pin group | `Shift+F3` |
| Smart redaction | `Ctrl+Alt+R` |
| QR recognition | `Ctrl+Alt+Q` |
| Beautified export | `Ctrl+Alt+B` |
| Pin workspace | `Ctrl+Alt+W` |
| Clipboard history | `Ctrl+Shift+V` |

Screenshot actions:

| Action | Key |
| --- | --- |
| Copy | `Ctrl+C` |
| Save | `Ctrl+S` |
| Pin | `Enter` or `F3` |
| OCR | `O` |
| Reopen OCR results | `Shift+O` |
| Smart redaction | `R` |
| QR recognition | `Q` |
| Translate | `T` |
| Selection ratio presets | `1` to `6` |
| Cycle windows | `Tab` / `Shift+Tab` |
| Nudge selection | Arrow keys / `Shift` + arrow keys |

## CLI

```text
snapin snip [--auto-copy] [--custom] [--translate] [--fullscreen] [--redact] [--qr] [--beautify] [--ocr-panel] [--delay SECONDS]
snapin pick-color
snapin paste
snapin toggle
snapin switch-group [ID]
snapin workspace
snapin clipboard-history
snapin clear-history
snapin prefs
snapin daemon
snapin exit
snapin install-extension
snapin sync-hotkeys
snapin pin-file <PATH> [X] [Y]
```

## Configuration

- Configuration: `~/.config/snapin/config.toml`
- Cache: `~/.cache/snapin/`
- Runtime files: `$XDG_RUNTIME_DIR/snapin/`
- Clipboard history: `~/.config/snapin/clipboard-history/`

Configuration may contain translation API credentials. Do not publish it or paste it into issue reports.

## Verification

```bash
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo check --workspace --all-targets
cargo test --workspace
```

## License

Snapin source code is licensed under Apache-2.0. Third-party notices are listed in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
