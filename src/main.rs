use pulldown_cmark::{html, Options, Parser, HeadingLevel, Event, Tag, TagEnd};
use std::path::PathBuf;
use std::sync::Arc;
use tao::{
    dpi::LogicalSize,
    event::{Event as TaoEvent, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};
use wry::WebViewBuilder;

const WIDTH_WITHOUT_TOC: f64 = 750.0;
const WIDTH_WITH_TOC: f64 = 1000.0;
const HEIGHT: f64 = 700.0;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file_path = std::env::args().nth(1).map(|arg| {
        let path = PathBuf::from(&arg);
        path.canonicalize().unwrap_or(path)
    });

    let (content, filename) = if let Some(ref path) = file_path {
        match std::fs::read_to_string(path) {
            Ok(c) => (c, path.file_name().and_then(|n| n.to_str()).unwrap_or("untitled").to_string()),
            Err(e) => (format!("# Error\n\nCould not load file: {}", e), "Error".to_string()),
        }
    } else {
        ("# Welcome to Marrow\n\nOpen a markdown file to get started.\n\nUse `Cmd+O` or drag and drop a `.md` file.".to_string(), "Marrow".to_string())
    };

    let toc = extract_toc(&content);
    let html_content = markdown_to_html(&content);
    let full_html = build_full_html(&content, &html_content, &toc, &filename);

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title(format!("Marrow - {}", filename))
        .with_inner_size(LogicalSize::new(WIDTH_WITHOUT_TOC, HEIGHT))
        .build(&event_loop)?;

    let window = Arc::new(window);
    let window_clone = Arc::clone(&window);

    let _webview = WebViewBuilder::new()
        .with_html(&full_html)
        .with_ipc_handler(move |req| {
            let msg = req.body();
            match msg.as_str() {
                "toc_show" => {
                    window_clone.set_inner_size(LogicalSize::new(WIDTH_WITH_TOC, HEIGHT));
                }
                "toc_hide" => {
                    window_clone.set_inner_size(LogicalSize::new(WIDTH_WITHOUT_TOC, HEIGHT));
                }
                _ => {}
            }
        })
        .build(&window)?;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        if let TaoEvent::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } = event
        {
            *control_flow = ControlFlow::Exit;
        }
    });
}

fn extract_toc(markdown: &str) -> Vec<(usize, String)> {
    let options = Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_TASKLISTS;

    let parser = Parser::new_ext(markdown, options);
    let mut toc = Vec::new();
    let mut in_heading = false;
    let mut current_level = 0;
    let mut current_text = String::new();

    for event in parser {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                in_heading = true;
                current_level = match level {
                    HeadingLevel::H1 => 1,
                    HeadingLevel::H2 => 2,
                    HeadingLevel::H3 => 3,
                    HeadingLevel::H4 => 4,
                    HeadingLevel::H5 => 5,
                    HeadingLevel::H6 => 6,
                };
                current_text.clear();
            }
            Event::End(TagEnd::Heading(_)) => {
                if in_heading && !current_text.is_empty() {
                    toc.push((current_level, current_text.clone()));
                }
                in_heading = false;
            }
            Event::Text(text) if in_heading => {
                current_text.push_str(&text);
            }
            Event::Code(code) if in_heading => {
                current_text.push_str(&code);
            }
            _ => {}
        }
    }

    toc
}

fn markdown_to_html(markdown: &str) -> String {
    let options = Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_TASKLISTS;

    let parser = Parser::new_ext(markdown, options);
    let mut html_output = String::new();
    html::push_html(&mut html_output, parser);
    html_output
}

fn slugify(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn build_full_html(content: &str, rendered_html: &str, toc: &[(usize, String)], _filename: &str) -> String {
    let toc_html: String = toc
        .iter()
        .map(|(level, text)| {
            let slug = slugify(text);
            let indent = (*level as f32 - 1.0) * 12.0;
            format!(
                r##"<a href="#" onclick="scrollToHeading('{}'); return false;" class="toc-item toc-level-{}" style="padding-left: {}px;">{}</a>"##,
                slug, level, indent, html_escape(text)
            )
        })
        .collect();

    let content_with_ids = add_heading_ids(rendered_html);
    let raw_markdown_escaped = html_escape(content);

    format!(
        r##"<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <link rel="stylesheet" href="https://cdnjs.cloudflare.com/ajax/libs/highlight.js/11.9.0/styles/github-dark.min.css">
    <style>{}</style>
</head>
<body>
    <div class="container">
        <main class="content github" id="content">
            <div id="github-view" class="view-content">{}</div>
            <pre id="terminal-view" class="view-content" style="display:none;"><code>{}</code></pre>
        </main>
        <nav class="toc hidden" id="toc">
            <div class="toc-header">Contents</div>
            {}
        </nav>
    </div>
    <div class="hotkey-bar">
        <span><kbd>G</kbd> GitHub</span>
        <span><kbd>T</kbd> Terminal</span>
        <span><kbd>C</kbd> Contents</span>
    </div>
    <script src="https://cdnjs.cloudflare.com/ajax/libs/highlight.js/11.9.0/highlight.min.js"></script>
    <script>{}</script>
</body>
</html>"##,
        CSS,
        content_with_ids,
        raw_markdown_escaped,
        toc_html,
        JS
    )
}

fn add_heading_ids(html: &str) -> String {
    let mut result = html.to_string();
    for tag in ["h1", "h2", "h3", "h4", "h5", "h6"] {
        let open_tag = format!("<{}>", tag);
        let close_tag = format!("</{}>", tag);

        let mut new_result = String::new();
        let mut remaining = result.as_str();

        while let Some(start) = remaining.find(&open_tag) {
            new_result.push_str(&remaining[..start]);
            remaining = &remaining[start + open_tag.len()..];

            if let Some(end) = remaining.find(&close_tag) {
                let heading_text = &remaining[..end];
                let slug = slugify(&strip_html_tags(heading_text));
                new_result.push_str(&format!(r#"<{} id="{}">{}</{}>"#, tag, slug, heading_text, tag));
                remaining = &remaining[end + close_tag.len()..];
            }
        }
        new_result.push_str(remaining);
        result = new_result;
    }
    result
}

fn strip_html_tags(s: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        if c == '<' {
            in_tag = true;
        } else if c == '>' {
            in_tag = false;
        } else if !in_tag {
            result.push(c);
        }
    }
    result
}

fn html_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

const CSS: &str = r##"
* {
    margin: 0;
    padding: 0;
    box-sizing: border-box;
}

:root {
    --bg-primary: #0d1117;
    --bg-secondary: #161b22;
    --bg-tertiary: #21262d;
    --text-primary: #e6edf3;
    --text-secondary: #8b949e;
    --text-muted: #6e7681;
    --border-color: #30363d;
    --accent-color: #58a6ff;
    --code-bg: #161b22;
}

body {
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Helvetica, Arial, sans-serif;
    background: var(--bg-primary);
    color: var(--text-primary);
    line-height: 1.6;
    overflow: hidden;
    font-size: 15px;
}

.container {
    display: flex;
    height: calc(100vh - 20px);
}

.toc {
    width: 200px;
    min-width: 150px;
    background: var(--bg-secondary);
    border-left: 1px solid var(--border-color);
    overflow-y: auto;
    padding: 12px 0;
    order: 2;
}

.toc.hidden {
    display: none;
}

.hotkey-bar {
    position: fixed;
    bottom: 0;
    left: 0;
    right: 0;
    height: 20px;
    background: var(--bg-secondary);
    border-top: 1px solid var(--border-color);
    display: flex;
    align-items: center;
    padding: 0 12px;
    gap: 16px;
    font-size: 10px;
    color: var(--text-muted);
}

.hotkey-bar kbd {
    background: var(--bg-tertiary);
    border: 1px solid var(--border-color);
    border-radius: 3px;
    padding: 1px 4px;
    font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, monospace;
    font-size: 9px;
    margin-right: 4px;
}

.toc-header {
    padding: 0 12px 8px;
    font-weight: 600;
    font-size: 11px;
    color: var(--text-primary);
    border-bottom: 1px solid var(--border-color);
    margin-bottom: 6px;
}

.toc-item {
    display: block;
    padding: 4px 12px;
    color: var(--text-secondary);
    text-decoration: none;
    font-size: 10px;
    border-right: 2px solid transparent;
    transition: all 0.15s ease;
}

.toc-item:hover {
    color: var(--text-primary);
    background: var(--bg-tertiary);
    border-right-color: var(--accent-color);
}

.toc-level-1 { font-weight: 600; }
.toc-level-2 { font-weight: 500; }
.toc-level-3, .toc-level-4, .toc-level-5, .toc-level-6 { font-weight: 400; }

.content {
    flex: 1;
    overflow-y: auto;
    padding: 32px 48px;
    max-width: 900px;
}

/* GitHub Mode Styles */
.github h1, .github h2, .github h3, .github h4, .github h5, .github h6 {
    margin-top: 24px;
    margin-bottom: 16px;
    font-weight: 600;
    line-height: 1.25;
}

.github h1 { font-size: 2em; padding-bottom: 0.3em; border-bottom: 1px solid var(--border-color); }
.github h2 { font-size: 1.5em; padding-bottom: 0.3em; border-bottom: 1px solid var(--border-color); }
.github h3 { font-size: 1.25em; }
.github h4 { font-size: 1em; }
.github h5 { font-size: 0.875em; }
.github h6 { font-size: 0.85em; color: var(--text-secondary); }

.github p { margin-bottom: 16px; }

.github a { color: var(--accent-color); text-decoration: none; }
.github a:hover { text-decoration: underline; }

.github code {
    padding: 0.2em 0.4em;
    margin: 0;
    font-size: 85%;
    background: var(--bg-tertiary);
    border-radius: 6px;
    font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, monospace;
}

.github pre {
    padding: 0;
    overflow: auto;
    font-size: 85%;
    line-height: 1.45;
    background: var(--code-bg);
    border-radius: 6px;
    margin-bottom: 16px;
    position: relative;
}

.github pre .code-header {
    display: block;
    padding: 8px 16px;
    background: var(--bg-tertiary);
    border-bottom: 1px solid var(--border-color);
    border-radius: 6px 6px 0 0;
    font-size: 12px;
    color: var(--text-secondary);
    font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, monospace;
}

.github pre code {
    display: block;
    padding: 16px;
    background: transparent;
    border-radius: 0;
    font-size: 100%;
    overflow-x: auto;
}

.github pre .code-footer {
    display: none;
}

.github blockquote {
    padding: 0 1em;
    color: var(--text-secondary);
    border-left: 4px solid var(--border-color);
    margin-bottom: 16px;
}

.github ul, .github ol { padding-left: 2em; margin-bottom: 16px; }
.github li { margin-bottom: 4px; }

.github hr {
    height: 4px;
    padding: 0;
    margin: 24px 0;
    background: var(--border-color);
    border: 0;
    border-radius: 2px;
}

.github table { border-collapse: collapse; margin-bottom: 16px; width: 100%; }
.github th, .github td { padding: 6px 13px; border: 1px solid var(--border-color); }
.github th { font-weight: 600; background: var(--bg-secondary); }
.github tr:nth-child(2n) { background: var(--bg-secondary); }

.github strong { font-weight: 600; color: var(--text-primary); }
.github em { font-style: italic; }

/* Terminal Mode Styles - Raw markdown with syntax highlighting */
#terminal-view {
    font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, monospace;
    font-size: 11px;
    line-height: 1.5;
    background: transparent;
    margin: 0;
    padding: 0;
    white-space: pre-wrap;
    word-wrap: break-word;
    color: var(--text-primary);
}

#terminal-view code {
    font-family: inherit;
    font-size: inherit;
    background: transparent;
    padding: 0;
}

/* Markdown syntax highlighting - muted colors */
.md-heading { color: #e0c080; }
.md-bold { color: #d4a056; }
.md-italic { color: #a0a0d0; font-style: italic; }
.md-code { color: #80b080; }
.md-code-fence { color: #666; }
.md-link-text { color: #6a9fcf; }
.md-link-url { color: #5a5a5a; }
.md-list-marker { color: #b07070; }
.md-blockquote { color: #888; }
.md-hr { color: #444; }
.md-table { color: #999; }
.md-table-sep { color: #555; }
.md-link { color: #6a9fcf; text-decoration: none; }
.md-link:hover { text-decoration: underline; }

/* Code block background wrapper */
.md-code-block-wrapper {
    display: block;
    background: rgba(255, 255, 255, 0.04);
    border-radius: 4px;
    padding: 8px 12px;
}
"##;

const JS: &str = r##"
let currentMode = 'github';
let tocVisible = false;

function setMode(mode) {
    currentMode = mode;
    const content = document.getElementById('content');
    content.className = 'content ' + mode;

    // Toggle views
    document.getElementById('github-view').style.display = mode === 'github' ? 'block' : 'none';
    document.getElementById('terminal-view').style.display = mode === 'terminal' ? 'block' : 'none';

    try { localStorage.setItem('marrow-mode', mode); } catch(e) {}
}

function toggleToc() {
    tocVisible = !tocVisible;
    const toc = document.getElementById('toc');
    toc.classList.toggle('hidden', !tocVisible);

    // Resize window via IPC
    if (window.ipc) {
        window.ipc.postMessage(tocVisible ? 'toc_show' : 'toc_hide');
    }

    try { localStorage.setItem('marrow-toc', tocVisible ? 'visible' : 'hidden'); } catch(e) {}
}

function scrollToHeading(slug) {
    const el = document.getElementById(slug);
    if (el) {
        el.scrollIntoView({ behavior: 'smooth', block: 'start' });
    }
}

// Keyboard shortcuts
document.addEventListener('keydown', function(e) {
    // Ignore if typing in input
    if (e.target.tagName === 'INPUT' || e.target.tagName === 'TEXTAREA') return;

    switch(e.key.toLowerCase()) {
        case 'g':
            setMode('github');
            break;
        case 't':
            setMode('terminal');
            break;
        case 'c':
            toggleToc();
            break;
    }
});

function initCodeBlocks() {
    // GitHub view: add language labels to code blocks
    document.querySelectorAll('#github-view pre code').forEach((codeBlock) => {
        const classes = codeBlock.className.split(' ');
        let lang = '';
        for (const cls of classes) {
            if (cls.startsWith('language-')) {
                lang = cls.replace('language-', '');
                break;
            }
        }

        const pre = codeBlock.parentElement;
        if (!pre.querySelector('.code-header') && lang) {
            const header = document.createElement('div');
            header.className = 'code-header';
            header.textContent = lang;
            pre.insertBefore(header, codeBlock);
        }

        // Apply syntax highlighting
        if (typeof hljs !== 'undefined') {
            hljs.highlightElement(codeBlock);
        }
    });

    // Terminal view: custom markdown syntax highlighting
    highlightMarkdown();
}

function formatTable(tableLines) {
    // Parse table into cells
    const rows = tableLines.map(line => {
        const cells = line.split('|').slice(1, -1).map(c => c.trim());
        return cells;
    });

    if (rows.length < 2) return tableLines;

    // Find max width for each column
    const colWidths = [];
    for (const row of rows) {
        for (let i = 0; i < row.length; i++) {
            const cellLen = row[i].replace(/[-:]/g, m => m === '-' ? '-' : '').length;
            colWidths[i] = Math.max(colWidths[i] || 0, cellLen, 3);
        }
    }

    // Format each row
    return rows.map((row, rowIndex) => {
        const cells = row.map((cell, i) => {
            const width = colWidths[i] || 3;
            if (rowIndex === 1 && cell.match(/^[-:]+$/)) {
                // Separator row
                return '-'.repeat(width);
            }
            return cell.padEnd(width);
        });
        return '| ' + cells.join(' | ') + ' |';
    });
}

function highlightMarkdown() {
    const terminalView = document.getElementById('terminal-view');
    const code = terminalView.querySelector('code');
    if (!code) return;

    let text = code.textContent;
    let html = '';
    let inCodeBlock = false;
    let codeBlockLang = '';
    let codeBlockContent = [];

    // Pre-process: format tables
    let lines = text.split('\n');
    let formattedLines = [];
    let tableBuffer = [];

    for (let i = 0; i < lines.length; i++) {
        const line = lines[i];
        if (line.trim().startsWith('|') && line.trim().endsWith('|')) {
            tableBuffer.push(line);
        } else {
            if (tableBuffer.length > 0) {
                formattedLines.push(...formatTable(tableBuffer));
                tableBuffer = [];
            }
            formattedLines.push(line);
        }
    }
    if (tableBuffer.length > 0) {
        formattedLines.push(...formatTable(tableBuffer));
    }

    lines = formattedLines;

    for (let i = 0; i < lines.length; i++) {
        let line = lines[i];

        // Code block start/end
        if (line.match(/^```/)) {
            if (!inCodeBlock) {
                inCodeBlock = true;
                codeBlockLang = line.slice(3).trim();
                html += '<span class="md-code-fence">' + escapeHtml(line) + '</span>\n<span class="md-code-block-wrapper">';
                codeBlockContent = [];
            } else {
                // End of code block - apply syntax highlighting to collected content
                if (codeBlockContent.length > 0 && codeBlockLang && typeof hljs !== 'undefined') {
                    try {
                        const highlighted = hljs.highlight(codeBlockContent.join('\n'), { language: codeBlockLang, ignoreIllegals: true });
                        html += highlighted.value;
                    } catch (e) {
                        // Fallback if language not supported
                        html += escapeHtml(codeBlockContent.join('\n'));
                    }
                } else if (codeBlockContent.length > 0) {
                    html += escapeHtml(codeBlockContent.join('\n'));
                }
                html += '</span><span class="md-code-fence">' + escapeHtml(line) + '</span>\n';
                inCodeBlock = false;
                codeBlockLang = '';
            }
            continue;
        }

        if (inCodeBlock) {
            codeBlockContent.push(line);
            continue;
        }

        line = escapeHtml(line);

        // Headings
        if (line.match(/^#{1,6}\s/)) {
            html += '<span class="md-heading">' + line + '</span>\n';
            continue;
        }

        // Horizontal rules
        if (line.match(/^(-{3,}|\*{3,}|_{3,})$/)) {
            html += '<span class="md-hr">' + line + '</span>\n';
            continue;
        }

        // Blockquotes
        if (line.match(/^&gt;\s?/)) {
            html += '<span class="md-blockquote">' + line + '</span>\n';
            continue;
        }

        // Table rows
        if (line.match(/^\|.*\|$/)) {
            if (line.match(/^\|[\s\-|]+\|$/)) {
                // Separator row
                html += '<span class="md-table-sep">' + line + '</span>\n';
            } else {
                // Apply inline formatting to table content
                let tableLine = line;
                tableLine = tableLine.replace(/(\*\*|__)(.+?)\1/g, '<span class="md-bold">$1$2$1</span>');
                tableLine = tableLine.replace(/(\*|(?<!\w)_)(.+?)\1(?!\w)/g, '<span class="md-italic">$1$2$1</span>');
                tableLine = tableLine.replace(/`([^`]+)`/g, '<span class="md-code">`$1`</span>');
                tableLine = tableLine.replace(/\[([^\]]+)\]\(([^)]+)\)/g, '<a href="$2" class="md-link" target="_blank">[$1]($2)</a>');
                html += '<span class="md-table">' + tableLine + '</span>\n';
            }
            continue;
        }

        // List items
        line = line.replace(/^(\s*)([-*+]|\d+\.)\s/, '$1<span class="md-list-marker">$2</span> ');

        // Inline formatting (order matters)
        // Bold **text** or __text__
        line = line.replace(/(\*\*|__)(.+?)\1/g, '<span class="md-bold">$1$2$1</span>');

        // Italic *text* or _text_ (but not inside words for _)
        line = line.replace(/(\*|(?<!\w)_)(.+?)\1(?!\w)/g, '<span class="md-italic">$1$2$1</span>');

        // Inline code
        line = line.replace(/`([^`]+)`/g, '<span class="md-code">`$1`</span>');

        // Links [text](url) - make them clickable
        line = line.replace(/\[([^\]]+)\]\(([^)]+)\)/g, '<a href="$2" class="md-link" target="_blank">[$1]($2)</a>');

        html += line + '\n';
    }

    code.innerHTML = html.slice(0, -1); // Remove trailing newline
}

function escapeHtml(text) {
    return text
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;');
}

// Initialize
document.addEventListener('DOMContentLoaded', function() {
    initCodeBlocks();

    try {
        const savedMode = localStorage.getItem('marrow-mode');
        if (savedMode) setMode(savedMode);

        const savedToc = localStorage.getItem('marrow-toc');
        if (savedToc === 'visible') {
            tocVisible = true;
            document.getElementById('toc').classList.remove('hidden');
            if (window.ipc) {
                window.ipc.postMessage('toc_show');
            }
        }
    } catch(e) {}
});
"##;
