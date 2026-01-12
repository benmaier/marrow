![Marrow icon](icon/marrow5-128.png)

# Marrow

A fast, native macOS viewer for Markdown with raw/formatted view modes, smart copy, and a distraction-free reading experience. Experimental Jupyter notebook view mode included.

⬇️ **[Download Marrow.app.zip](https://github.com/benmaier/marrow/releases/download/v0.1.2/Marrow.app.zip)**

If you find Marrow useful, consider [buying me a coffee](https://buymeacoffee.com/benmaier) ☕

## Why Marrow?

I increasingly treat `.md` files like `.pdf`s; LLMs generate them, I just read them and copy what I need. Same for Jupyter notebooks: I want to check an old analysis, not rerun it.

But macOS Quick Look shows raw markdown (or worse, raw JSON for notebooks). Double-clicking opens a clunky IDE that takes seconds to boot, shows an edit view, and risks accidental changes. I don't want to edit; I just want to read.

So I had Claude Code build Marrow in Rust+JS. It opens instantly, renders beautifully, and is view-only.

**Bonus features:**
- **Smart copy** maps your selection back to markdown source, so tables and formatting survive pasting
- **Table of Contents** lets you navigate and track your position

<!-- There are plenty of markdown *editors*. But what if you just want to *read* markdown or notebooks?

Marrow is a viewer, not an editor. No syntax panes, no live preview splits, no project management. Just:

- **Render markdown** with GitHub-style dark theme
- **View Jupyter notebooks** with native rendering and collapsible cells
- **Copy as markdown** when you select and copy
- **Navigate with a TOC** generated from headings

That's it. Lightweight, fast, native. -->

## Features

### Dual Rendering Modes

**GitHub Mode** (default)
- Polished, dark-themed HTML rendering matching GitHub's markdown style
- Full GFM (GitHub Flavored Markdown) support: tables, task lists, strikethrough, footnotes
- Syntax-highlighted code blocks with language labels
- Clickable links that open in your default browser

**Terminal Mode**
- Raw markdown with syntax highlighting
- Perfect for copy-pasting into terminals, editors, or chat apps
- Monospace font with clear visual hierarchy
- Tables auto-formatted for alignment

Press `Tab` to toggle between modes. Your scroll position syncs between views.

### Smart Copy

Marrow's copy behavior is context-aware:

| Shortcut | GitHub Mode | Terminal Mode |
|----------|-------------|---------------|
| `Cmd+C` | Copies **markdown source** | Copies plain text |
| `Shift+Cmd+C` | Copies **formatted HTML** | Copies plain text |

**Precise Selection**: When you select text in GitHub mode, Marrow extracts the exact markdown source for your selection—including surrounding syntax like `**bold**` or `` `code` ``. Select a table cell, get the markdown table row. Select a code block, get the fenced code block with language tag.

### Table of Contents

Auto-generated navigation panel from document headings:

- Press `T` to toggle the TOC sidebar
- Click any heading to jump to it
- Current section highlights as you scroll
- Hierarchical indentation (H1 → H6)

### Search

Press `Cmd+F` to search within the document:

- Real-time highlighting as you type
- `Enter` / `Shift+Enter` to navigate between matches
- Match counter shows current position
- `Esc` to close

### Additional Features

- **Font Size Control**: `Cmd+Plus` / `Cmd+Minus` / `Cmd+0` to zoom
- **Multi-Window**: Open multiple documents, each in its own window
- **File Associations**: Set Marrow as your default `.md` or `.ipynb` viewer
- **Persistent Preferences**: View mode, TOC state, and font size remembered per file type
- **Smart Window Titles**: Shows first heading + filename

### Jupyter Notebook Support

Marrow renders `.ipynb` files natively—no markdown conversion, no external dependencies.

**Native Rendering**
- Code cells with syntax highlighting and `In[n]:` prompts
- Markdown cells rendered as GitHub-style HTML
- Output cells including text, images, and HTML
- Error tracebacks with ANSI color support

**Collapsible Cells**
- Press `C` to collapse/expand all code cells
- Click the `▼` button to toggle individual cells
- Only code input collapses—outputs stay visible

**Rich Output**
- Images displayed inline (click to expand)
- HTML output preserved with inline styles
- KaTeX math rendering in markdown cells (not in code outputs)

**Per-Extension Settings**
- Separate preferences for `.md` and `.ipynb` files
- Each file type remembers its own theme, window size, and TOC state

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `Tab` | Toggle GitHub/Terminal view (markdown only) |
| `T` | Toggle Table of Contents |
| `D` | Toggle Dark/Light theme |
| `C` | Collapse/expand all cells (notebook only) |
| `W` | Toggle output line wrapping (notebook only) |
| `Cmd+F` | Open search |
| `Enter` | Next search match |
| `Shift+Enter` | Previous search match |
| `Esc` | Close search |
| `Cmd+C` | Copy (markdown source in GitHub mode, formatted in notebook) |
| `Shift+Cmd+C` | Copy formatted HTML |
| `Cmd+A` | Select all content |
| `Cmd+Plus` | Increase font size |
| `Cmd+Minus` | Decrease font size |
| `Cmd+0` | Reset font size |
| `Cmd+W` | Close window |
| `Cmd+Q` | Quit app |

## Installation

### Pre-built App

Download `Marrow.app` from [Releases](../../releases) and drag to `/Applications`.

### Build from Source

Requires Rust toolchain. Install via [rustup](https://rustup.rs/).

```bash
# Clone the repo
git clone https://github.com/benmaier/marrow.git
cd marrow

# Build and install
make install
```

This creates `Marrow.app` in `/Applications`.

### Set as Default Markdown Viewer

1. Right-click any `.md` file in Finder
2. Select "Get Info"
3. Under "Open with", choose Marrow
4. Click "Change All..."

## Usage

### From Command Line

```bash
# Open markdown
open -a Marrow path/to/file.md

# Open notebook
open -a Marrow path/to/notebook.ipynb
```

### From Finder

Double-click any `.md` or `.ipynb` file (if Marrow is set as default), or:

1. Right-click the file
2. Open With → Marrow

### Drag and Drop

Drag markdown or notebook files onto the Marrow icon in Dock or Applications.

## Supported Markdown

Marrow uses [pulldown-cmark](https://github.com/raphlinus/pulldown-cmark) with full GFM extensions:

- **Headings** (ATX style: `# H1` through `###### H6`)
- **Emphasis** (`*italic*`, `**bold**`, `***bold italic***`)
- **Strikethrough** (`~~deleted~~`)
- **Code** (inline `` `code` `` and fenced blocks with syntax highlighting)
- **Links** (`[text](url)` and `[text](url "title")`)
- **Images** (`![alt](url)`)
- **Blockquotes** (`> quoted text`)
- **Lists** (ordered, unordered, nested)
- **Task Lists** (`- [x] done`, `- [ ] todo`)
- **Tables** (GFM pipe tables with alignment)
- **Footnotes** (`[^1]` references)
- **Horizontal Rules** (`---`, `***`, `___`)
- **Raw HTML** (passed through in GitHub mode)

### Syntax Highlighting

Code blocks support 180+ languages with syntax highlighting. Specify the language after the opening fence:

````markdown
```python
def hello():
    print("Hello, World!")
```
````

## Dev Guide

### Requirements

- Rust 1.70+ (install via [rustup](https://rustup.rs/))
- macOS 10.15+ (Catalina or later)
- Xcode Command Line Tools

### Make Targets

```bash
make build          # Build release binary
make bundle         # Create Marrow.app bundle
make install        # Bundle and copy to /Applications
make clean          # Remove build artifacts
make update-vendor  # Update vendored JS dependencies
```

### Manual Build

```bash
# Install cargo-bundle if needed
cargo install cargo-bundle

# Build the app bundle
cargo bundle --release

# Binary location
ls target/release/marrow

# App bundle location
ls target/release/bundle/osx/Marrow.app
```

## Architecture

```
src/
├── main.rs        (1400 lines) - Rust core
│   ├── Window management (tao/wry)
│   ├── Markdown parsing (pulldown-cmark)
│   ├── Notebook parsing & native HTML rendering
│   ├── ANSI-to-HTML conversion for error tracebacks
│   ├── Per-extension settings persistence
│   └── IPC handlers (clipboard, resize, settings)
│
├── script.js      (900 lines) - UI logic
│   ├── View switching (GitHub/Terminal)
│   ├── Notebook cell collapse/expand
│   ├── TOC navigation & scroll tracking
│   ├── Search with highlighting
│   ├── Smart copy (markdown extraction)
│   └── Figure expand overlay
│
├── style.css      (600 lines) - Styling
│   ├── Light/dark themes (CSS variables)
│   ├── GitHub markdown styles
│   ├── Notebook cell styles (.nb-*)
│   └── Hotkey bar, TOC, search bar
│
└── template.html  (75 lines) - HTML shell
```

### How Smart Copy Works

Each HTML element includes a `data-lines` attribute mapping to original markdown line numbers. When you copy:

1. JavaScript finds which elements intersect your selection
2. Extracts the corresponding markdown lines
3. Matches your selected text within those lines
4. Expands to include surrounding markdown syntax
5. Sends to clipboard via native IPC (arboard crate)

This approach preserves formatting markers (`**`, `` ` ``, `#`, etc.) that browsers would normally strip.

## Dependencies

| Crate | Purpose |
|-------|---------|
| [wry](https://github.com/tauri-apps/wry) | Cross-platform WebView (WebKit on macOS) |
| [tao](https://github.com/tauri-apps/tao) | Window management |
| [pulldown-cmark](https://github.com/raphlinus/pulldown-cmark) | Markdown parsing with GFM |
| [arboard](https://github.com/1Password/arboard) | Cross-platform clipboard |
| [directories](https://github.com/dirs-dev/directories-rs) | Platform config paths |
| [serde](https://serde.rs/) | Serialization |

## Vendored Dependencies

Marrow works fully offline. JavaScript dependencies are vendored in the `vendor/` directory rather than loaded from CDNs.

**Why vendoring instead of npm?**

With only a few JS dependencies, the overhead of npm tooling isn't justified. Instead:

- `vendor/manifest.json` tracks versions and SHA256 checksums
- `vendor/update-vendor.sh` downloads and verifies dependencies
- Files are embedded at compile time via `include_str!()`

This approach provides version tracking and checksum verification without requiring Node.js to build.

**Updating dependencies:**

```bash
# Edit version in vendor/manifest.json, then:
make update-vendor

# Or run directly with --verify to check checksums:
./vendor/update-vendor.sh --verify
```

## Support

If you find Marrow useful, consider [buying me a coffee](https://buymeacoffee.com/benmaier).

## License

MIT

## Contributing

Marrow is a tool that I personally needed. It's been built with Claude Code within a few hours of total dev time. While I'm happy to hear about issues at [github.com/benmaier/marrow](https://github.com/benmaier/marrow) so I can improve the tool *mainly for myself*, I probably won't incorporate features that I don't find sensible. Also, I won't have time for PRs. If you like Marrow but want it to behave differently, you're welcome to fork it and tell Claude Code how it should be changed for you.
