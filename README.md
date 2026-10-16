# Swift Explorer

[![Release](https://img.shields.io/github/v/release/misutesu-desu/swift-explorer)](../../releases/latest)
[![Build](https://github.com/misutesu-desu/swift-explorer/actions/workflows/build.yml/badge.svg)](../../actions/workflows/build.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue)](LICENSE)

[Türkçe](README.tr.md) · [简体中文](README.zh-CN.md) · [日本語](README.ja.md)

A file manager for Windows and Linux that looks and behaves like the Windows 11 File Explorer, written in Rust. Folders are read on background threads, so large ones open without freezing the window, and search runs across subfolders in parallel.

![Large icons, grouped by date](docs/large-icons.png)

## Download

Get `SwiftExplorer.exe` from the [Releases](../../releases) page. It is a single file with no installer. Made for Windows 11; Windows 10 should work but hasn't been tested.

With [Scoop](https://scoop.sh):

```powershell
scoop bucket add misutesu https://github.com/misutesu-desu/scoop-bucket
scoop install swift-explorer
```

On Linux, install it for your user with:

```bash
curl -fsSL https://raw.githubusercontent.com/misutesu-desu/swift-explorer/main/install.sh | sh
```

This puts `swift-explorer` in `~/.local/bin` and adds it to the app menu. To remove it, run the same command with `-s -- --uninstall` after `sh`. You can also download `swift-explorer-linux-x86_64.tar.gz` and run `swift-explorer` from it without installing.

Releases after 0.2.0 also include `swift-explorer.flatpak`, which installs with `flatpak install --user swift-explorer.flatpak`.

## What it does

- Same layout as Explorer: tabs in the title bar, address bar with breadcrumbs, command bar, navigation pane, Home and This PC pages.
- Uses Windows' own icons, thumbnails and file type names.
- Every Explorer shortcut works the same way. Press F1 for the full list.
- Typing in the search box filters the current folder instantly. Press Enter to search all subfolders; `*` and `?` wildcards work.
- Typing `cmd`, `powershell`, `pwsh` or `wt` in the address bar opens that terminal in the current folder, and `terminal` opens Windows Terminal. Press Ctrl+Shift+Enter instead of Enter to run it as administrator. On Linux, `terminal`, `cmd`, `powershell` and `pwsh` open your default terminal, and a terminal's name (`konsole`, `kitty`…) opens that one.
- Copy, cut, paste and drag and drop work in both directions with Explorer and the desktop.
- Shift + right-click opens the full Windows context menu, so 7-Zip, WinRAR and Send to are there.
- Ctrl+Z undoes renames, moves, copies and deletes to the Recycle Bin.
- Besides System, Light and Dark, View > Theme has Catppuccin, Gruvbox, Everforest, Nord, Dracula, Tokyo Night, Rosé Pine and Solarized.
- A copy or move counts as done only after the files are written to the disk, like running `sync` in a terminal. Once the progress bar is gone, a USB stick can be pulled out. When moving to another drive, the originals are deleted only after that.
- When a file with the same name already exists, it asks whether to replace, skip or keep both.
- Folders update live when files change on disk.
- Downloads is grouped by date modified (Today, Yesterday, Last week…), like in Explorer. Other folders can be grouped from Sort > Group by.
- Interface in English, Turkish, German, Spanish, French, Brazilian Portuguese, Russian, Simplified Chinese and Japanese. The language follows Windows and can be changed from View > Language.
- Chinese, Japanese and Korean file names display with the system's CJK fonts.

| Details | This PC |
| --- | --- |
| ![Details view](docs/details.png) | ![This PC](docs/this-pc.png) |

| Light theme | Turkish |
| --- | --- |
| ![Light theme](docs/light.png) | ![Turkish interface](docs/turkish.png) |

The Type column and the Shift + right-click menu come from Windows, so they stay in the Windows display language even when the app is set to English.

## Linux

The Linux version is new and less tested than the Windows one. It keeps the same look and shortcuts, with these differences:

- Icons come from your desktop's icon theme. Thumbnails are shown for PNG, JPEG, GIF, WebP and BMP.
- There is no Windows context menu, so Shift + right-click and Shift+F10 open the app's own menu.
- Copied files go to the clipboard as a list of paths. Pasting paths or `file://` links copied from other file managers works; pasting into other file managers may not.
- Dragging files out of the window to other apps doesn't work yet. Dropping files into it works on X11.
- "This PC" lists `/` and disks mounted under `/media`, `/run/media` and `/mnt`.
- Properties and "Show in file manager" use your file manager over D-Bus (Nautilus, Dolphin, Nemo and Thunar support it).

## Building

You need Rust 1.82 or newer.

```bash
cargo build --release
```

The executable ends up in `target/release/swift-explorer.exe` on Windows and `target/release/swift-explorer` on Linux.

## Translations

Each language is one file in [`src/i18n/`](src/i18n). To add one, copy `de.rs`, translate the second string on every line, then add a `mod` line and entries in `LANGS`, `tables()` and `system_lang()` in [`src/i18n.rs`](src/i18n.rs). The [German commit](../../commits/main/src/i18n/de.rs) shows every change needed. Fixes to the existing translations are welcome too; native speakers haven't checked them yet.

## Settings

Settings are stored in `%APPDATA%\SwiftExplorer\config.json` on Windows and `~/.config/SwiftExplorer/config.json` on Linux. Setting the `SWIFT_EXPLORER_LANG` environment variable to a language code (`en`, `tr`, `de`, `es`, `fr`, `pt`, `ru`, `zh`, `ja`) overrides the language setting.

## Known limitations

- In Details view, the selection rectangle can only be started from the empty area below the rows.
- Windows can't move files with very long paths (around 260+ characters) to the Recycle Bin, so they are deleted directly and Ctrl+Z can't bring them back.
- Network locations open in Windows File Explorer.

## License

[MIT](LICENSE). The Linux build includes a few icons from Microsoft's [Fluent UI System Icons](https://github.com/microsoft/fluentui-system-icons) ([MIT](assets/LICENSE-fluent-icons.txt)).
