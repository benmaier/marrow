# Marrow - Project Notes

## Overview
Marrow is a native macOS markdown viewer built with Rust (wry/tao). It has dual rendering modes (GitHub-style HTML and Terminal raw markdown), smart copy, TOC, search, and light/dark themes.

## Project Structure
- `src/main.rs` - Rust: window management, settings, IPC handlers
- `src/script.js` - JavaScript: UI logic, keyboard shortcuts, theme/mode switching
- `src/style.css` - CSS: all styling including light/dark themes
- `src/template.html` - HTML template with placeholders
- Files are embedded at compile time via `include_str!()`

## Build Commands
Always use full cargo path or source env (non-interactive bash doesn't have cargo in PATH):

```bash
# Build only
/Users/bfmaier/.cargo/bin/cargo build --release

# Or with make (needs PATH)
PATH="$HOME/.cargo/bin:$PATH" make install
```

Make targets:
- `make build` - Build release binary
- `make bundle` - Create Marrow.app bundle
- `make install` - Bundle and copy to /Applications

## Updating the App Icon
1. Place new PNG in `icon/` (e.g., `icon/marrow4.png`)
2. Generate iconset and install:
```bash
# Generate all icon sizes
mkdir -p icon/icon.iconset
sips -z 1024 1024 icon/marrow4.png --out icon/icon.iconset/icon_512x512@2x.png
sips -z 512 512 icon/marrow4.png --out icon/icon.iconset/icon_512x512.png
sips -z 512 512 icon/marrow4.png --out icon/icon.iconset/icon_256x256@2x.png
sips -z 256 256 icon/marrow4.png --out icon/icon.iconset/icon_256x256.png
sips -z 256 256 icon/marrow4.png --out icon/icon.iconset/icon_128x128@2x.png
sips -z 128 128 icon/marrow4.png --out icon/icon.iconset/icon_128x128.png
sips -z 64 64 icon/marrow4.png --out icon/icon.iconset/icon_32x32@2x.png
sips -z 32 32 icon/marrow4.png --out icon/icon.iconset/icon_32x32.png
sips -z 32 32 icon/marrow4.png --out icon/icon.iconset/icon_16x16@2x.png
sips -z 16 16 icon/marrow4.png --out icon/icon.iconset/icon_16x16.png

# Convert to icns
iconutil -c icns icon/icon.iconset -o icon/icon.icns

# Copy to app and refresh icon cache
cp icon/icon.icns /Applications/Marrow.app/Contents/Resources/icon.icns
touch /Applications/Marrow.app
killall Finder Dock
```

## Settings
Stored at: `~/Library/Application Support/com.marrow.app/settings.json`

Fields: `window_width`, `window_height`, `toc_visible`, `view_mode`, `font_size_level`, `theme`

## Keyboard Shortcuts
- `Tab` - Toggle GitHub/Terminal view
- `C` - Toggle Table of Contents
- `L` - Toggle Light/Dark theme
- `Cmd+F` - Search
- `Cmd+C` - Copy as markdown (GitHub) / plain text (Terminal)
- `Shift+Cmd+C` - Copy formatted HTML
- `Cmd+Plus/Minus/0` - Adjust/reset font size
- `Cmd+W` - Close window
- `Cmd+Q` - Quit app

## IPC Messages (JS → Rust)
- `resize:width:height` - Resize window
- `clipboard:text` - Copy text to clipboard
- `save_settings:{json}` - Save settings to file
- `close_window` - Close current window
- `quit_app` - Quit application
