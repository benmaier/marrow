# TODO

## First Launch Theme Detection

On first launch (when no settings file exists), detect macOS dark/light mode preference and use that as the default theme instead of always defaulting to dark.

Implementation: Use `defaults read -g AppleInterfaceStyle` which returns "Dark" if dark mode is enabled, or returns an error if light mode is active.

## Markdown Source View Rendering

Currently, the terminal/source view for markdown files is rendered in JavaScript (`highlightMarkdown()` in script.js). This causes a brief flash of unstyled plaintext when the app opens with source view as the saved preference.

**Goal**: Move the markdown source syntax highlighting to Rust so the HTML is fully rendered before being pushed to the frontend. This would eliminate the flash entirely.

The highlighting logic needs to handle:
- HTML comments (`<!-- -->`, can be multi-line)
- Code blocks (``` with language tag)
- Headings (`#` through `######`)
- Horizontal rules (`---`, `***`, `___`)
- Blockquotes (`>`)
- Tables (`| ... |`)
- List items (`-`, `*`, `+`, `1.`)
- Inline formatting: bold (`**`), italic (`*`), code (`` ` ``), links (`[text](url)`)

Note: Rust's `regex` crate doesn't support lookbehind assertions, so the inline formatting logic needs to use manual string parsing instead of regex.
