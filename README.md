# TrayBin

A lightweight screenshot manager for Windows that lives in your system tray. Inspired by [Screenie](https://www.screenie.io/) for macOS.

![TrayBin Screenshot](docs/screenshot.png)

## Features

- **System Tray Integration** - Runs quietly in your system tray, always ready when you need it
- **Global Hotkey** - Toggle the window with a customizable keyboard shortcut (default: `Ctrl+Shift+S`)
- **GPU-Accelerated UI** - Built with [GPUI](https://gpui.rs/) (Zed's UI framework) for smooth, responsive performance
- **Smart Organization** - Screenshots automatically grouped by date (Today, Yesterday, This Week, etc.)
- **Thumbnail Gallery** - Beautiful grid view with adjustable thumbnail sizes
- **Drag & Drop** - Drag screenshots directly into other applications
- **Multi-Select** - Select multiple items with checkboxes or Ctrl+Click/Shift+Click
- **Auto-Convert** - Automatically convert PNG screenshots to WebP or JPEG to save space
- **Native Context Menu** - Right-click for Windows shell context menu (Open, Copy, Delete, etc.)
- **Clipboard Support** - Copy selected files with `Ctrl+C`

## Installation

### From Source

```bash
# Clone the repository
git clone https://github.com/ssut/traybin.git
cd traybin

# Build release version
cargo build --release

# Run
./target/release/traybin.exe
```

### Requirements

- Windows 10/11
- Rust 1.75+ (uses edition 2024)

## Usage

### Basic Controls

| Action                 | Description                             |
| ---------------------- | --------------------------------------- |
| **Left Click (Tray)**  | Toggle window visibility                |
| **Right Click (Tray)** | Open tray menu                          |
| **Global Hotkey**      | Toggle window (default: `Ctrl+Shift+S`) |
| **ESC**                | Minimize window                         |
| **Ctrl+C**             | Copy selected files to clipboard        |
| **Ctrl+A**             | Select all visible screenshots          |
| **Double Click**       | Open screenshot with default app        |
| **Right Click**        | Show context menu                       |

### Selection

- **Click checkbox** - Toggle selection (multi-select)
- **Click item** - Select single item
- **Ctrl+Click** - Add/remove from selection
- **Shift+Click** - Range selection

### Drag & Drop

Select one or more screenshots and drag them directly into:

- File Explorer
- Email clients
- Chat applications (Slack, Discord, etc.)
- Image editors
- Any application that accepts files

## Settings

Access settings by clicking the gear icon (⚙) in the header.

### General

- **Screenshot Directory** - Folder to watch for new screenshots
- **Thumbnail Size** - Adjust grid thumbnail size (80-300px)

### Conversion

- **Auto-convert Screenshots** - Automatically convert new PNG files
- **Conversion Format** - Choose WebP or JPEG
- **Quality** - Image quality (1-100)

### Hotkey

- **Enable Global Hotkey** - Toggle hotkey functionality
- **Current Hotkey** - View/record new hotkey combination

## Configuration

Settings are stored in:

```
%APPDATA%\traybin\settings.json
```

Default screenshot directory:

```
%USERPROFILE%\Pictures\Screenshots
```

## Debug Mode

Run with console output for debugging:

```bash
traybin.exe --console
```

Logs are written to `traybin_debug.log` in the current directory.

## Tech Stack

- **[GPUI](https://gpui.rs/)** - GPU-accelerated UI framework from Zed
- **[gpui-component](https://github.com/longbridge/gpui-component)** - UI component library
- **[tray-icon](https://github.com/tauri-apps/tray-icon)** - System tray support
- **[global-hotkey](https://github.com/tauri-apps/global-hotkey)** - Global keyboard shortcuts
- **[notify](https://github.com/notify-rs/notify)** - File system watching
- **[image](https://github.com/image-rs/image)** - Image processing

## Building

```bash
# Debug build
cargo build

# Release build (optimized)
cargo build --release

# Run tests
cargo test
```

## License

MIT License - see [LICENSE](LICENSE) for details.

## Acknowledgments

- Inspired by [Screenie](https://www.screenie.io/) for macOS
- Built with [GPUI](https://gpui.rs/) by the [Zed](https://zed.dev/) team
- UI components from [gpui-component](https://github.com/longbridge/gpui-component)
