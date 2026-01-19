# Marrow - Project Notes

## Overview
Marrow is a native macOS markdown and Jupyter notebook viewer built with Rust (wry/tao). It has dual rendering modes (GitHub-style HTML and Terminal raw markdown), smart copy, TOC, search, light/dark themes, and per-extension settings.

## Project Structure
```
src/
├── main.rs        (~1450 lines) - Rust core, organized with section markers
├── script.js      (~920 lines)  - UI logic, organized with section markers
├── style.css      (~640 lines)  - Styling, organized with section markers
└── template.html  (~75 lines)   - HTML shell

vendor/            - Vendored JS dependencies (highlight.js, KaTeX)
icon/              - Only marrow5.png and marrow5.afdesign tracked in git
sandbox/           - Test files (test.md, test.ipynb, test_math.ipynb)
```

Files are embedded at compile time via `include_str!()`.

### Code Navigation
All source files have clear section markers (search for `====`):

**main.rs sections:**
- IMPORTS & TYPES
- SETTINGS PERSISTENCE
- WINDOW MANAGEMENT
- NOTEBOOK RENDERING
- MARKDOWN RENDERING
- HTML TEMPLATE BUILDING

**script.js sections:**
- STATE & CONSTANTS
- SETTINGS & PREFERENCES
- VIEW SWITCHING & NAVIGATION
- COPY HANDLING (Cmd+C)
- KEYBOARD SHORTCUTS
- SEARCH
- NOTEBOOK: CELL COLLAPSE & IMAGE EXPAND
- MARKDOWN: CODE BLOCKS & TERMINAL VIEW
- TOC & INITIALIZATION

**style.css sections:**
- BASE & THEME VARIABLES
- LAYOUT: CONTAINER, TOC, HOTKEY BAR
- GITHUB MODE (Rendered Markdown)
- TERMINAL MODE (Raw Markdown)
- NOTEBOOK MODE

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
- `make update-vendor` - Update vendored JS dependencies
- `make clean` - Remove build artifacts

The Makefile handles:
- Icon generation from `icon/marrow5.png` → `icon/icon.icns`
- Document type associations (.md, .ipynb)
- UTI declarations for file type recognition

## Updating the App Icon
1. Replace `icon/marrow5.png` with new icon (1024x1024 recommended)
2. Run `make install` (icon is auto-generated)

## Releasing
```bash
# Create zip and release
cd target/release/bundle/osx
ditto -c -k --keepParent Marrow.app Marrow.app.zip
gh release create v0.x.x --title "Marrow v0.x.x" --notes "..." Marrow.app.zip
```

Note: App is not code-signed. Users must right-click → Open on first launch.

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
- `C` - Collapse/expand all code cells (notebook only)
- `O` - Collapse/expand all outputs (notebook only)
- `W` - Toggle output line wrapping (notebook only)
- `Cmd+F` - Search
- `Cmd+C` - Copy as markdown (GitHub view) / formatted (notebook)
- `Shift+Cmd+C` - Copy formatted HTML
- `Cmd+Plus/Minus/0` - Adjust/reset font size
- `Cmd+W` - Close window
- `Cmd+Q` - Quit app

Hotkeys were changed to avoid vim navigation keys (H/J/K/L).

## IPC Messages (JS → Rust)
- `resize:width:height` - Resize window
- `clipboard:text` - Copy text to clipboard
- `save_settings:ext:{json}` - Save settings for extension
- `close_window` - Close current window
- `quit_app` - Quit application

## Key Implementation Details

### Live Reload
- Uses `notify` crate with FSEvents backend on macOS
- File changes trigger re-rendering via `webview.evaluate_script()`
- Debounced to avoid excessive reloads on rapid file changes
- Preserves scroll position and view state during reload

### Image Expand
- Both markdown and notebook images scale to max-width: 100%
- Click any image to expand in overlay
- Press Escape or click overlay to close
- Inline onclick handlers don't work (WebView CSP blocks them) - must use addEventListener

### Notebook Rendering
- Native HTML rendering (not markdown conversion)
- KaTeX only renders in `.nb-markdown-cell` elements (not code outputs)
- Code collapse (`C`) affects `.nb-input`, outputs stay visible
- Output collapse (`O`) reduces `.nb-output-content` to 60px with fade overlay
- ANSI escape codes in error tracebacks converted to colored HTML spans
- HTML outputs: strip outer `<pre>` wrapper, keep inline styles

### Copy Behavior
- Markdown GitHub mode: Cmd+C extracts markdown source via data-lines attributes
- Notebook mode: Cmd+C uses document.execCommand('copy') for formatted copy
- Shift+Cmd+C always copies formatted HTML

### Vendored Dependencies
- `vendor/manifest.json` - versions and SHA256 checksums
- `vendor/update-vendor.sh` - download and verify script
- highlight.js, KaTeX (CSS + JS + auto-render)

## Git/Repo Notes
- Cargo.lock is tracked (intentional - reproducible builds for binary)
- `icon/icon.icns` is gitignored (generated from marrow5.png)
- Old icon versions kept on disk but gitignored
- `sandbox/` contains test files, tracked in git

## TODO
See `TODO.md` in project root for detailed technical todos.

- **First launch theme detection**: On first launch (no settings file exists), detect macOS dark/light mode preference and use that as the default theme. Use `defaults read -g AppleInterfaceStyle` (returns "Dark" if dark mode, error if light mode).
- **Rust-side markdown source highlighting**: Move terminal view syntax highlighting from JS to Rust to eliminate flash on open (see TODO.md for details).
