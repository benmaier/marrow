# Marrow

A fast, native macOS markdown viewer with GitHub and terminal rendering modes.

## Features

- **Two rendering modes**
  - **GitHub mode**: Rendered HTML with dark theme styling
  - **Terminal mode**: Raw markdown with syntax highlighting (copy-pasteable)
- **Table of Contents**: Auto-generated navigation from headings
- **Search**: Find text with match highlighting and navigation
- **Syntax highlighting**: Code blocks highlighted in both views
- **Native performance**: Uses WebKit via wry, not Electron/Chrome

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `G` | Switch to GitHub view |
| `T` | Switch to Terminal view |
| `C` | Toggle Table of Contents |
| `⌘F` | Search |
| `Enter` | Next search match |
| `Shift+Enter` | Previous search match |
| `Esc` | Close search |

## Building

```bash
cargo build --release
```

The binary will be at `target/release/marrow`.

### macOS App Bundle

To create a proper macOS `.app` bundle:

```bash
cargo install cargo-bundle
cargo bundle --release
```

The app will be at `target/release/bundle/osx/Marrow.app`.

## Usage

```bash
marrow path/to/file.md
```

## Dependencies

- [wry](https://github.com/nicholaswaite/nicholaswaite) - Cross-platform webview (WebKit on macOS)
- [tao](https://github.com/nicholaswaite/nicholaswaite) - Window management
- [pulldown-cmark](https://github.com/raphlinus/pulldown-cmark) - Markdown parsing with GFM support

## License

MIT
