use pulldown_cmark::{html, Options, Parser, HeadingLevel, Event, Tag, TagEnd};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tao::{
    dpi::LogicalSize,
    event::{Event as TaoEvent, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy, EventLoopWindowTarget},
    window::{Window, WindowBuilder, WindowId},
};
use wry::{WebView, WebViewBuilder};

#[derive(Debug)]
enum UserEvent {
    CloseWindow(WindowId),
    QuitApp,
}

const INITIAL_WIDTH: f64 = 800.0;
const HEIGHT: f64 = 900.0;

struct AppWindow {
    _window: Arc<Window>,
    _webview: WebView,
}

fn create_window(
    event_loop: &EventLoopWindowTarget<UserEvent>,
    proxy: EventLoopProxy<UserEvent>,
    path: Option<&PathBuf>,
) -> Result<(WindowId, AppWindow), Box<dyn std::error::Error>> {
    let (content, filename) = load_file(path);
    let toc = extract_toc(&content);
    let html_content = markdown_to_html(&content);
    let full_html = build_full_html(&content, &html_content, &toc, &filename);

    let window = WindowBuilder::new()
        .with_title(format!("Marrow - {}", filename))
        .with_inner_size(LogicalSize::new(INITIAL_WIDTH, HEIGHT))
        .build(event_loop)?;

    let window = Arc::new(window);
    let window_clone = Arc::clone(&window);
    let window_id = window.id();
    let proxy_clone = proxy.clone();

    let webview = WebViewBuilder::new()
        .with_html(&full_html)
        .with_ipc_handler(move |req| {
            let msg = req.body();
            if msg.starts_with("resize:") {
                // Format: "resize:width:height"
                let parts: Vec<&str> = msg.split(':').collect();
                if parts.len() == 3 {
                    if let (Ok(width), Ok(height)) = (parts[1].parse::<f64>(), parts[2].parse::<f64>()) {
                        window_clone.set_inner_size(LogicalSize::new(width, height));
                    }
                }
            } else {
                match msg.as_str() {
                    "close_window" => {
                        let _ = proxy_clone.send_event(UserEvent::CloseWindow(window_id));
                    }
                    "quit_app" => {
                        let _ = proxy_clone.send_event(UserEvent::QuitApp);
                    }
                    _ => {}
                }
            }
        })
        .with_navigation_handler(|url| {
            if url.starts_with("about:") || url.starts_with("data:") {
                return true;
            }
            if url.starts_with("http://") || url.starts_with("https://") {
                let _ = std::process::Command::new("open").arg(&url).spawn();
                return false;
            }
            true
        })
        .build(&window)?;

    Ok((window_id, AppWindow { _window: window, _webview: webview }))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let initial_path = std::env::args().nth(1).map(|arg| {
        let path = PathBuf::from(&arg);
        path.canonicalize().unwrap_or(path)
    });

    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    let mut windows: HashMap<WindowId, AppWindow> = HashMap::new();

    // Only create initial window if a file was passed via command line
    if let Some(ref path) = initial_path {
        let (id, app_window) = create_window(&event_loop, proxy.clone(), Some(path))?;
        windows.insert(id, app_window);
    }

    event_loop.run(move |event, event_loop, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            TaoEvent::Opened { urls } => {
                // Create a new window for each opened file
                for url in urls {
                    if let Ok(path) = url.to_file_path() {
                        if let Ok((id, app_window)) = create_window(event_loop, proxy.clone(), Some(&path)) {
                            windows.insert(id, app_window);
                        }
                    }
                }
            }
            TaoEvent::UserEvent(UserEvent::CloseWindow(window_id)) => {
                windows.remove(&window_id);
                if windows.is_empty() {
                    *control_flow = ControlFlow::Exit;
                }
            }
            TaoEvent::UserEvent(UserEvent::QuitApp) => {
                *control_flow = ControlFlow::Exit;
            }
            TaoEvent::WindowEvent {
                event: WindowEvent::CloseRequested,
                window_id,
                ..
            } => {
                windows.remove(&window_id);
                if windows.is_empty() {
                    *control_flow = ControlFlow::Exit;
                }
            }
            _ => {}
        }
    });
}

fn load_file(path: Option<&PathBuf>) -> (String, String) {
    if let Some(path) = path {
        match std::fs::read_to_string(path) {
            Ok(c) => (c, path.file_name().and_then(|n| n.to_str()).unwrap_or("untitled").to_string()),
            Err(e) => (format!("# Error\n\nCould not load file: {}", e), "Error".to_string()),
        }
    } else {
        ("# Welcome to Marrow\n\nOpen a markdown file to get started.\n\nDrag and drop a `.md` file or open one with Marrow.".to_string(), "Marrow".to_string())
    }
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
            format!(
                r##"<a href="#" onclick="scrollToHeading('{}'); return false;" class="toc-item toc-level-{}">{}</a>"##,
                slug, level, html_escape(text)
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
            {}
        </nav>
    </div>
    <div class="search-bar hidden" id="search-bar">
        <input type="text" id="search-input" placeholder="Search..." />
        <span id="search-count"></span>
        <button onclick="searchPrev()">↑</button>
        <button onclick="searchNext()">↓</button>
        <button onclick="closeSearch()">✕</button>
    </div>
    <div class="hotkey-bar">
        <span><kbd>Tab</kbd> Toggle View</span>
        <span><kbd>C</kbd> Contents</span>
        <span><kbd>⌘F</kbd> Search</span>
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
    height: calc(100vh - 34px);
}

.toc {
    width: 200px;
    min-width: 150px;
    background: var(--bg-secondary);
    border-left: 1px solid var(--border-color);
    overflow-y: auto;
    padding: 16px 0;
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
    background: var(--bg-secondary);
    border-top: 1px solid var(--border-color);
    display: flex;
    align-items: center;
    justify-content: flex-start;
    padding: 7px 16px 9px;
    gap: 20px;
    font-size: 11px;
    color: var(--text-muted);
}

.hotkey-bar kbd {
    background: var(--bg-tertiary);
    border: 1px solid var(--border-color);
    border-radius: 3px;
    padding: 2px 6px;
    font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, monospace;
    font-size: 10px;
    margin-right: 5px;
}

.search-bar {
    position: fixed;
    top: 10px;
    right: 10px;
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: 6px;
    padding: 8px 12px;
    display: flex;
    align-items: center;
    gap: 8px;
    z-index: 1000;
    box-shadow: 0 4px 12px rgba(0,0,0,0.3);
}

.search-bar.hidden {
    display: none;
}

.search-bar input {
    background: var(--bg-tertiary);
    border: 1px solid var(--border-color);
    border-radius: 4px;
    padding: 6px 10px;
    color: var(--text-primary);
    font-size: 13px;
    width: 200px;
    outline: none;
}

.search-bar input:focus {
    border-color: var(--accent-color);
}

.search-bar button {
    background: var(--bg-tertiary);
    border: 1px solid var(--border-color);
    border-radius: 4px;
    padding: 4px 8px;
    color: var(--text-secondary);
    cursor: pointer;
    font-size: 12px;
}

.search-bar button:hover {
    background: var(--border-color);
    color: var(--text-primary);
}

.search-bar #search-count {
    color: var(--text-muted);
    font-size: 11px;
    min-width: 50px;
}

mark.search-highlight {
    background: #5c4d1a;
    color: var(--text-primary);
    border-radius: 2px;
}

mark.search-highlight.current {
    background: #7a6520;
}

.toc-item {
    display: block;
    padding: 5px 14px;
    color: var(--text-secondary);
    text-decoration: none;
    font-size: 12px;
    border-right: 2px solid transparent;
    transition: all 0.15s ease;
}

.toc-item:hover {
    color: var(--text-primary);
    background: var(--bg-tertiary);
    border-right-color: var(--accent-color);
}

.toc-item.active {
    color: var(--text-primary);
    background: rgba(88, 166, 255, 0.1);
    border-right-color: var(--accent-color);
}

.toc-level-1 {
    font-weight: 600;
    color: var(--text-primary);
    border-bottom: 1px solid var(--border-color);
    padding-bottom: 8px;
    margin-bottom: 4px;
}
.toc-level-2 { font-weight: 500; padding-left: 14px !important; }
.toc-level-3 { font-weight: 400; padding-left: 26px !important; }
.toc-level-4, .toc-level-5, .toc-level-6 { font-weight: 400; padding-left: 38px !important; }

.content {
    flex: 1;
    overflow-y: auto;
    padding: 32px 48px;
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
const TOC_WIDTH = 200;
let fontSizeLevel = 0;  // -3 to +5 range
const BASE_FONT_SIZE = 15;
const TERMINAL_BASE_SIZE = 11;

function adjustFontSize(delta) {
    fontSizeLevel = Math.max(-3, Math.min(5, fontSizeLevel + delta));
    applyFontSize();
    try { localStorage.setItem('marrow-fontsize', fontSizeLevel); } catch(e) {}
}

function resetFontSize() {
    fontSizeLevel = 0;
    applyFontSize();
    try { localStorage.setItem('marrow-fontsize', fontSizeLevel); } catch(e) {}
}

function applyFontSize() {
    const scale = 1 + (fontSizeLevel * 0.1);  // Each level is 10%
    document.body.style.fontSize = (BASE_FONT_SIZE * scale) + 'px';
    document.getElementById('terminal-view').style.fontSize = (TERMINAL_BASE_SIZE * scale) + 'px';
}

function getCurrentHeadingId() {
    const content = document.getElementById('content');
    const activeView = currentMode === 'github' ? '#github-view' : '#terminal-view';
    const headings = document.querySelectorAll(activeView + ' h1, ' + activeView + ' h2, ' + activeView + ' h3, ' + activeView + ' h4, ' + activeView + ' h5, ' + activeView + ' h6, ' + activeView + ' [id].md-heading');

    let currentHeading = null;
    const scrollTop = content.scrollTop;

    for (const heading of headings) {
        if (heading.offsetTop <= scrollTop + 100) {
            currentHeading = heading;
        } else {
            break;
        }
    }

    return currentHeading ? currentHeading.id : null;
}

function setMode(mode, scrollToId) {
    currentMode = mode;
    const content = document.getElementById('content');
    content.className = 'content ' + mode;

    // Toggle views
    document.getElementById('github-view').style.display = mode === 'github' ? 'block' : 'none';
    document.getElementById('terminal-view').style.display = mode === 'terminal' ? 'block' : 'none';

    // Scroll to the same section in the new view
    if (scrollToId) {
        const newView = mode === 'github' ? '#github-view' : '#terminal-view';
        const el = document.querySelector(newView + ' #' + CSS.escape(scrollToId));
        if (el) {
            el.scrollIntoView({ block: 'start' });
        }
    }

    try { localStorage.setItem('marrow-mode', mode); } catch(e) {}
}

function toggleToc() {
    const currentWidth = window.innerWidth;
    const currentHeight = window.innerHeight;

    tocVisible = !tocVisible;
    const toc = document.getElementById('toc');
    toc.classList.toggle('hidden', !tocVisible);

    // Resize window via IPC - send the new absolute size
    if (window.ipc) {
        let newWidth;
        if (tocVisible) {
            newWidth = currentWidth + TOC_WIDTH;
        } else {
            newWidth = currentWidth - TOC_WIDTH;
        }
        window.ipc.postMessage('resize:' + newWidth + ':' + currentHeight);
    }

    try { localStorage.setItem('marrow-toc', tocVisible ? 'visible' : 'hidden'); } catch(e) {}
}

function scrollToHeading(slug) {
    // Find element in the currently active view
    const activeView = currentMode === 'github' ? '#github-view' : '#terminal-view';
    const el = document.querySelector(activeView + ' #' + slug);
    if (el) {
        el.scrollIntoView({ behavior: 'smooth', block: 'start' });
    }
}

// Keyboard shortcuts
document.addEventListener('keydown', function(e) {
    // Cmd+F for search
    if ((e.metaKey || e.ctrlKey) && e.key === 'f') {
        e.preventDefault();
        openSearch();
        return;
    }

    // Cmd+W to close window
    if (e.metaKey && e.key === 'w') {
        e.preventDefault();
        if (window.ipc) window.ipc.postMessage('close_window');
        return;
    }

    // Cmd+Q to quit app
    if (e.metaKey && e.key === 'q') {
        e.preventDefault();
        if (window.ipc) window.ipc.postMessage('quit_app');
        return;
    }

    // Cmd+A to select all
    if (e.metaKey && e.key === 'a') {
        e.preventDefault();
        const content = document.getElementById('content');
        const range = document.createRange();
        range.selectNodeContents(content);
        const selection = window.getSelection();
        selection.removeAllRanges();
        selection.addRange(range);
        return;
    }

    // Cmd+Plus to increase font size
    if (e.metaKey && (e.key === '=' || e.key === '+')) {
        e.preventDefault();
        adjustFontSize(1);
        return;
    }

    // Cmd+Minus to decrease font size
    if (e.metaKey && e.key === '-') {
        e.preventDefault();
        adjustFontSize(-1);
        return;
    }

    // Cmd+0 to reset font size
    if (e.metaKey && e.key === '0') {
        e.preventDefault();
        resetFontSize();
        return;
    }

    // Escape to close search
    if (e.key === 'Escape') {
        closeSearch();
        return;
    }

    // Ignore other shortcuts if typing in input
    if (e.target.tagName === 'INPUT' || e.target.tagName === 'TEXTAREA') return;

    // Tab to toggle view (before modifier check)
    if (e.key === 'Tab') {
        e.preventDefault();
        const headingId = getCurrentHeadingId();
        setMode(currentMode === 'github' ? 'terminal' : 'github', headingId);
        return;
    }

    if (e.metaKey || e.ctrlKey || e.altKey) return;

    switch(e.key.toLowerCase()) {
        case 'c':
            toggleToc();
            break;
    }
});

// Search functionality
let searchMatches = [];
let currentMatchIndex = -1;

function openSearch() {
    const searchBar = document.getElementById('search-bar');
    searchBar.classList.remove('hidden');
    document.getElementById('search-input').focus();
}

function closeSearch() {
    const searchBar = document.getElementById('search-bar');
    searchBar.classList.add('hidden');
    clearHighlights();
    document.getElementById('search-input').value = '';
    document.getElementById('search-count').textContent = '';
}

function clearHighlights() {
    document.querySelectorAll('mark.search-highlight').forEach(mark => {
        const parent = mark.parentNode;
        parent.replaceChild(document.createTextNode(mark.textContent), mark);
        parent.normalize();
    });
    searchMatches = [];
    currentMatchIndex = -1;
}

function performSearch(query) {
    clearHighlights();
    if (!query || query.length < 2) {
        document.getElementById('search-count').textContent = '';
        return;
    }

    const activeView = currentMode === 'github' ? document.getElementById('github-view') : document.getElementById('terminal-view');
    const walker = document.createTreeWalker(activeView, NodeFilter.SHOW_TEXT, null, false);
    const textNodes = [];

    while (walker.nextNode()) {
        textNodes.push(walker.currentNode);
    }

    const regex = new RegExp(query.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), 'gi');

    textNodes.forEach(node => {
        const text = node.textContent;
        if (regex.test(text)) {
            regex.lastIndex = 0;
            const fragment = document.createDocumentFragment();
            let lastIndex = 0;
            let match;

            while ((match = regex.exec(text)) !== null) {
                if (match.index > lastIndex) {
                    fragment.appendChild(document.createTextNode(text.slice(lastIndex, match.index)));
                }
                const mark = document.createElement('mark');
                mark.className = 'search-highlight';
                mark.textContent = match[0];
                fragment.appendChild(mark);
                lastIndex = regex.lastIndex;
            }

            if (lastIndex < text.length) {
                fragment.appendChild(document.createTextNode(text.slice(lastIndex)));
            }

            node.parentNode.replaceChild(fragment, node);
        }
    });

    searchMatches = Array.from(document.querySelectorAll('mark.search-highlight'));
    currentMatchIndex = searchMatches.length > 0 ? 0 : -1;
    updateSearchCount();
    highlightCurrentMatch();
}

function updateSearchCount() {
    const count = searchMatches.length;
    const current = currentMatchIndex + 1;
    document.getElementById('search-count').textContent = count > 0 ? `${current}/${count}` : 'No results';
}

function highlightCurrentMatch() {
    searchMatches.forEach((m, i) => {
        m.classList.toggle('current', i === currentMatchIndex);
    });
    if (searchMatches[currentMatchIndex]) {
        searchMatches[currentMatchIndex].scrollIntoView({ behavior: 'smooth', block: 'center' });
    }
}

function searchNext() {
    if (searchMatches.length === 0) return;
    currentMatchIndex = (currentMatchIndex + 1) % searchMatches.length;
    updateSearchCount();
    highlightCurrentMatch();
}

function searchPrev() {
    if (searchMatches.length === 0) return;
    currentMatchIndex = (currentMatchIndex - 1 + searchMatches.length) % searchMatches.length;
    updateSearchCount();
    highlightCurrentMatch();
}

// Search input handler
document.getElementById('search-input').addEventListener('input', function(e) {
    performSearch(e.target.value);
});

document.getElementById('search-input').addEventListener('keydown', function(e) {
    if (e.key === 'Enter') {
        e.shiftKey ? searchPrev() : searchNext();
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

        // Headings - add ID for navigation
        if (line.match(/^#{1,6}\s/)) {
            const headingText = line.replace(/^#+\s*/, '');
            const slug = slugify(headingText);
            html += '<span class="md-heading" id="' + slug + '">' + line + '</span>\n';
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
                tableLine = tableLine.replace(/\[([^\]]+)\]\(([^)]+)\)/g, '<a href="$2" class="md-link">[$1]($2)</a>');
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
        line = line.replace(/\[([^\]]+)\]\(([^)]+)\)/g, '<a href="$2" class="md-link">[$1]($2)</a>');

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

function slugify(text) {
    return text.toLowerCase()
        .split('')
        .map(c => /[a-z0-9]/.test(c) ? c : '-')
        .join('')
        .split('-')
        .filter(s => s.length > 0)
        .join('-');
}

// TOC scroll tracking
function updateTocHighlight() {
    const content = document.getElementById('content');
    const activeView = currentMode === 'github' ? '#github-view' : '#terminal-view';
    const headings = document.querySelectorAll(activeView + ' h1, ' + activeView + ' h2, ' + activeView + ' h3, ' + activeView + ' h4, ' + activeView + ' h5, ' + activeView + ' h6, ' + activeView + ' [id].md-heading');

    let currentHeading = null;
    const scrollTop = content.scrollTop;

    for (const heading of headings) {
        if (heading.offsetTop <= scrollTop + 100) {
            currentHeading = heading;
        } else {
            break;
        }
    }

    // Update TOC highlighting
    document.querySelectorAll('.toc-item').forEach(item => item.classList.remove('active'));

    if (currentHeading && currentHeading.id) {
        const tocItem = document.querySelector('.toc-item[onclick*="' + currentHeading.id + '"]');
        if (tocItem) {
            tocItem.classList.add('active');
        }
    }
}

// Initialize
document.addEventListener('DOMContentLoaded', function() {
    initCodeBlocks();

    // Add scroll listener for TOC highlighting
    document.getElementById('content').addEventListener('scroll', updateTocHighlight);

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

        const savedFontSize = localStorage.getItem('marrow-fontsize');
        if (savedFontSize) {
            fontSizeLevel = parseInt(savedFontSize, 10);
            applyFontSize();
        }
    } catch(e) {}

    // Initial highlight
    updateTocHighlight();
});
"##;
