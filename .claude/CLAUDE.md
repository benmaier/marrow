# Marrow - Project Notes

## Overview
Marrow is a native macOS markdown and Jupyter notebook viewer built with Rust (wry/tao). It has dual rendering modes (GitHub-style HTML and Terminal raw markdown), smart copy, TOC, search, light/dark themes, and per-extension settings.

## Project Structure
- `src/main.rs` - Rust: window management, settings, IPC handlers, ipynb parsing
- `src/script.js` - JavaScript: UI logic, keyboard shortcuts, theme/mode switching
- `src/style.css` - CSS: all styling including light/dark themes
- `src/template.html` - HTML template with placeholders
- `vendor/` - Vendored dependencies (highlight.js, KaTeX)
- Files are embedded at compile time via `include_str!()`

## Build Commands
**ALWAYS use the Makefile for building and installing.** Never use cargo directly.

```bash
PATH="$HOME/.cargo/bin:$PATH" make install
```

Make targets:
- `make build` - Build release binary only
- `make bundle` - Create Marrow.app bundle (includes icon generation)
- `make install` - Bundle and copy to /Applications
- `make icon` - Regenerate icon from `icon/marrow5.png`

The Makefile handles:
- Icon generation from `icon/marrow5.png`
- Document type associations (.md, .ipynb)
- UTI declarations for file type recognition

## Updating the App Icon
1. Replace `icon/marrow5.png` with new icon
2. Run `make install` (icon is auto-generated from marrow5.png)

## Settings
Stored at: `~/Library/Application Support/com.marrow.app/settings.json`

Settings are per-extension (e.g., .md and .ipynb can have different themes/layouts).

Structure:
```json
{
  "default": { ... },
  "extensions": {
    "md": { "window_width": 800, "theme": "dark", ... },
    "ipynb": { "window_width": 900, "theme": "light", ... }
  }
}
```

Fields per extension: `window_width`, `window_height`, `toc_visible`, `view_mode`, `font_size_level`, `theme`

## Keyboard Shortcuts
- `Tab` - Toggle GitHub/Terminal view (markdown only)
- `T` - Toggle Table of Contents
- `D` - Toggle Light/Dark theme
- `C` - Collapse/expand all cells (notebook only)
- `Cmd+F` - Search
- `Cmd+C` - Copy as markdown (GitHub view) / formatted (notebook)
- `Shift+Cmd+C` - Copy formatted HTML
- `Cmd+Plus/Minus/0` - Adjust/reset font size
- `Cmd+W` - Close window
- `Cmd+Q` - Quit app

## IPC Messages (JS → Rust)
- `resize:width:height` - Resize window
- `clipboard:text` - Copy text to clipboard
- `save_settings:ext:{json}` - Save settings for extension (e.g., `save_settings:md:{...}`)
- `close_window` - Close current window
- `quit_app` - Quit application

## TODO
- **First launch theme detection**: On first launch (no settings file exists), detect macOS dark/light mode preference and use that as the default theme. Use `defaults read -g AppleInterfaceStyle` (returns "Dark" if dark mode, error if light mode).
