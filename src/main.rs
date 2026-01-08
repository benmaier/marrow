use pulldown_cmark::{Options, Parser, HeadingLevel, Event, Tag, TagEnd, CodeBlockKind};
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

fn truncate_end(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max - 1).collect::<String>())
    }
}

fn truncate_middle(s: &str, max: usize) -> String {
    let len = s.chars().count();
    if len <= max {
        s.to_string()
    } else {
        let keep = max - 1; // -1 for the ellipsis
        let left = keep / 2;
        let right = keep - left;
        let left_part: String = s.chars().take(left).collect();
        let right_part: String = s.chars().skip(len - right).collect();
        format!("{}…{}", left_part, right_part)
    }
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

    // Build window title: "First Heading · filename · Marrow 🦴"
    let first_heading = toc.first().map(|(_, text)| truncate_end(text, 20));
    let short_filename = truncate_middle(&filename, 20);
    let title = match first_heading {
        Some(heading) => format!("{} · {} · Marrow 🦴", heading, short_filename),
        None => format!("{} · Marrow 🦴", short_filename),
    };

    let window = WindowBuilder::new()
        .with_title(title)
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
            } else if msg.starts_with("clipboard:") {
                // Format: "clipboard:text_to_copy"
                let text = &msg[10..];
                if let Ok(mut clipboard) = arboard::Clipboard::new() {
                    let _ = clipboard.set_text(text);
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

/// Convert a byte offset in the source to a 1-based line number
fn byte_offset_to_line(markdown: &str, byte_offset: usize) -> usize {
    markdown[..byte_offset.min(markdown.len())]
        .chars()
        .filter(|&c| c == '\n')
        .count() + 1
}

fn markdown_to_html(markdown: &str) -> String {
    let options = Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_TASKLISTS;

    let parser = Parser::new_ext(markdown, options).into_offset_iter();
    let mut html_output = String::new();

    // Track current block's line range
    let mut block_start_line: Option<usize> = None;
    let mut pending_block_tag: Option<String> = None;

    // Heading-specific tracking: collect content and plain text for slug
    let mut in_heading: Option<String> = None; // The heading tag (h1, h2, etc.)
    let mut heading_start_line: usize = 0;
    let mut heading_html_content = String::new();
    let mut heading_plain_text = String::new();

    // Stack to handle nested elements
    let mut tag_stack: Vec<String> = Vec::new();

    for (event, range) in parser {
        let start_line = byte_offset_to_line(markdown, range.start);
        let end_line = byte_offset_to_line(markdown, range.end);

        match event {
            Event::Start(Tag::Paragraph) => {
                block_start_line = Some(start_line);
                pending_block_tag = Some("p".to_string());
                tag_stack.push("p".to_string());
            }
            Event::End(TagEnd::Paragraph) => {
                if let (Some(start), Some(_)) = (block_start_line, &pending_block_tag) {
                    html_output.push_str(&format!(r#"<p data-lines="{}-{}">"#, start, end_line));
                }
                html_output.push_str("</p>\n");
                block_start_line = None;
                pending_block_tag = None;
                tag_stack.pop();
            }

            Event::Start(Tag::Heading { level, .. }) => {
                let tag = match level {
                    HeadingLevel::H1 => "h1",
                    HeadingLevel::H2 => "h2",
                    HeadingLevel::H3 => "h3",
                    HeadingLevel::H4 => "h4",
                    HeadingLevel::H5 => "h5",
                    HeadingLevel::H6 => "h6",
                };
                in_heading = Some(tag.to_string());
                heading_start_line = start_line;
                heading_html_content.clear();
                heading_plain_text.clear();
                tag_stack.push(tag.to_string());
            }
            Event::End(TagEnd::Heading(level)) => {
                let tag = match level {
                    HeadingLevel::H1 => "h1",
                    HeadingLevel::H2 => "h2",
                    HeadingLevel::H3 => "h3",
                    HeadingLevel::H4 => "h4",
                    HeadingLevel::H5 => "h5",
                    HeadingLevel::H6 => "h6",
                };
                let slug = slugify(&heading_plain_text);
                html_output.push_str(&format!(
                    r#"<{} id="{}" data-lines="{}-{}">{}</{}>"#,
                    tag, slug, heading_start_line, end_line, heading_html_content, tag
                ));
                html_output.push('\n');
                in_heading = None;
                tag_stack.pop();
            }

            Event::Start(Tag::BlockQuote(_)) => {
                html_output.push_str(&format!(r#"<blockquote data-lines="{}-__BQ_END__">"#, start_line));
                tag_stack.push("blockquote".to_string());
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                if let Some(pos) = html_output.rfind("__BQ_END__") {
                    html_output.replace_range(pos..pos + 10, &end_line.to_string());
                }
                html_output.push_str("</blockquote>\n");
                tag_stack.pop();
            }

            Event::Start(Tag::CodeBlock(kind)) => {
                let lang = match &kind {
                    CodeBlockKind::Fenced(lang) if !lang.is_empty() => Some(lang.as_ref()),
                    _ => None,
                };
                if let Some(lang) = lang {
                    html_output.push_str(&format!(r#"<pre data-lines="{}-__PRE_END__"><code class="language-{}">"#, start_line, lang));
                } else {
                    html_output.push_str(&format!(r#"<pre data-lines="{}-__PRE_END__"><code>"#, start_line));
                }
                tag_stack.push("pre".to_string());
            }
            Event::End(TagEnd::CodeBlock) => {
                html_output.push_str("</code></pre>\n");
                if let Some(pos) = html_output.rfind("__PRE_END__") {
                    // Add 1 to include the closing ``` fence line
                    html_output.replace_range(pos..pos + 11, &(end_line + 1).to_string());
                }
                tag_stack.pop();
            }

            Event::Start(Tag::List(first_item)) => {
                if first_item.is_some() {
                    html_output.push_str(&format!(r#"<ol data-lines="{}-__OL_END__">"#, start_line));
                    tag_stack.push("ol".to_string());
                } else {
                    html_output.push_str(&format!(r#"<ul data-lines="{}-__UL_END__">"#, start_line));
                    tag_stack.push("ul".to_string());
                }
            }
            Event::End(TagEnd::List(ordered)) => {
                let (tag, placeholder) = if ordered { ("ol", "__OL_END__") } else { ("ul", "__UL_END__") };
                if let Some(pos) = html_output.rfind(placeholder) {
                    html_output.replace_range(pos..pos + placeholder.len(), &end_line.to_string());
                }
                html_output.push_str(&format!("</{}>", tag));
                tag_stack.pop();
            }

            Event::Start(Tag::Item) => {
                html_output.push_str(&format!(r#"<li data-lines="{}-__LI_END__">"#, start_line));
                tag_stack.push("li".to_string());
            }
            Event::End(TagEnd::Item) => {
                if let Some(pos) = html_output.rfind("__LI_END__") {
                    html_output.replace_range(pos..pos + 10, &end_line.to_string());
                }
                html_output.push_str("</li>\n");
                tag_stack.pop();
            }

            Event::Start(Tag::Table(_)) => {
                // Use placeholder for end line, replace when table ends
                html_output.push_str(&format!(r#"<table data-lines="{}-__TABLE_END__">"#, start_line));
                tag_stack.push("table".to_string());
            }
            Event::End(TagEnd::Table) => {
                // Replace the placeholder with actual end line
                if let Some(pos) = html_output.rfind("__TABLE_END__") {
                    html_output.replace_range(pos..pos + 13, &end_line.to_string());
                }
                html_output.push_str("</table>\n");
                tag_stack.pop();
            }
            Event::Start(Tag::TableHead) => {
                html_output.push_str("<thead><tr>");
                tag_stack.push("thead".to_string());
            }
            Event::End(TagEnd::TableHead) => {
                html_output.push_str("</tr></thead>");
                tag_stack.pop();
            }
            Event::Start(Tag::TableRow) => {
                html_output.push_str("<tr>");
            }
            Event::End(TagEnd::TableRow) => {
                html_output.push_str("</tr>");
            }
            Event::Start(Tag::TableCell) => {
                // Use <th> in thead, <td> elsewhere
                if tag_stack.iter().any(|t| t == "thead") {
                    html_output.push_str("<th>");
                } else {
                    html_output.push_str("<td>");
                }
            }
            Event::End(TagEnd::TableCell) => {
                if tag_stack.iter().any(|t| t == "thead") {
                    html_output.push_str("</th>");
                } else {
                    html_output.push_str("</td>");
                }
            }

            // Inline elements - route to heading buffer if inside a heading
            Event::Start(Tag::Emphasis) => {
                if in_heading.is_some() {
                    heading_html_content.push_str("<em>");
                } else {
                    if pending_block_tag.is_some() {
                        flush_pending_tag(&mut html_output, &pending_block_tag, block_start_line, end_line);
                        pending_block_tag = None;
                    }
                    html_output.push_str("<em>");
                }
            }
            Event::End(TagEnd::Emphasis) => {
                if in_heading.is_some() {
                    heading_html_content.push_str("</em>");
                } else {
                    html_output.push_str("</em>");
                }
            }
            Event::Start(Tag::Strong) => {
                if in_heading.is_some() {
                    heading_html_content.push_str("<strong>");
                } else {
                    if pending_block_tag.is_some() {
                        flush_pending_tag(&mut html_output, &pending_block_tag, block_start_line, end_line);
                        pending_block_tag = None;
                    }
                    html_output.push_str("<strong>");
                }
            }
            Event::End(TagEnd::Strong) => {
                if in_heading.is_some() {
                    heading_html_content.push_str("</strong>");
                } else {
                    html_output.push_str("</strong>");
                }
            }
            Event::Start(Tag::Strikethrough) => {
                if in_heading.is_some() {
                    heading_html_content.push_str("<del>");
                } else {
                    if pending_block_tag.is_some() {
                        flush_pending_tag(&mut html_output, &pending_block_tag, block_start_line, end_line);
                        pending_block_tag = None;
                    }
                    html_output.push_str("<del>");
                }
            }
            Event::End(TagEnd::Strikethrough) => {
                if in_heading.is_some() {
                    heading_html_content.push_str("</del>");
                } else {
                    html_output.push_str("</del>");
                }
            }
            Event::Start(Tag::Link { dest_url, title, .. }) => {
                let link_html = if title.is_empty() {
                    format!(r#"<a href="{}">"#, dest_url)
                } else {
                    format!(r#"<a href="{}" title="{}">"#, dest_url, title)
                };
                if in_heading.is_some() {
                    heading_html_content.push_str(&link_html);
                } else {
                    if pending_block_tag.is_some() {
                        flush_pending_tag(&mut html_output, &pending_block_tag, block_start_line, end_line);
                        pending_block_tag = None;
                    }
                    html_output.push_str(&link_html);
                }
            }
            Event::End(TagEnd::Link) => {
                if in_heading.is_some() {
                    heading_html_content.push_str("</a>");
                } else {
                    html_output.push_str("</a>");
                }
            }
            Event::Start(Tag::Image { dest_url, title, .. }) => {
                let mut img_html = format!(r#"<img src="{}" alt=""#, dest_url);
                if !title.is_empty() {
                    img_html.push_str(&format!(r#"" title="{}""#, title));
                }
                if in_heading.is_some() {
                    heading_html_content.push_str(&img_html);
                } else {
                    if pending_block_tag.is_some() {
                        flush_pending_tag(&mut html_output, &pending_block_tag, block_start_line, end_line);
                        pending_block_tag = None;
                    }
                    html_output.push_str(&img_html);
                }
            }
            Event::End(TagEnd::Image) => {
                if in_heading.is_some() {
                    heading_html_content.push_str(r#"" />"#);
                } else {
                    html_output.push_str(r#"" />"#);
                }
            }

            Event::Text(text) => {
                if in_heading.is_some() {
                    heading_html_content.push_str(&html_escape(&text));
                    heading_plain_text.push_str(&text);
                } else {
                    if pending_block_tag.is_some() {
                        flush_pending_tag(&mut html_output, &pending_block_tag, block_start_line, end_line);
                        pending_block_tag = None;
                    }
                    html_output.push_str(&html_escape(&text));
                }
            }
            Event::Code(code) => {
                if in_heading.is_some() {
                    heading_html_content.push_str(&format!("<code>{}</code>", html_escape(&code)));
                    heading_plain_text.push_str(&code);
                } else {
                    if pending_block_tag.is_some() {
                        flush_pending_tag(&mut html_output, &pending_block_tag, block_start_line, end_line);
                        pending_block_tag = None;
                    }
                    html_output.push_str(&format!("<code>{}</code>", html_escape(&code)));
                }
            }
            Event::SoftBreak => {
                if in_heading.is_some() {
                    heading_html_content.push('\n');
                } else {
                    html_output.push('\n');
                }
            }
            Event::HardBreak => {
                if in_heading.is_some() {
                    heading_html_content.push_str("<br />\n");
                } else {
                    html_output.push_str("<br />\n");
                }
            }
            Event::Rule => {
                html_output.push_str(&format!(r#"<hr data-lines="{}-{}" />"#, start_line, end_line));
            }

            Event::Html(html) => {
                html_output.push_str(&html);
            }

            Event::FootnoteReference(name) => {
                html_output.push_str(&format!(r##"<sup class="footnote-ref"><a href="#fn-{}">[{}]</a></sup>"##, name, name));
            }

            Event::TaskListMarker(checked) => {
                if checked {
                    html_output.push_str(r#"<input type="checkbox" checked disabled /> "#);
                } else {
                    html_output.push_str(r#"<input type="checkbox" disabled /> "#);
                }
            }

            _ => {}
        }
    }

    html_output
}

fn flush_pending_tag(output: &mut String, tag: &Option<String>, start_line: Option<usize>, end_line: usize) {
    if let (Some(tag), Some(start)) = (tag, start_line) {
        output.push_str(&format!(r#"<{} data-lines="{}-{}">"#, tag, start, end_line));
    }
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

    // Headings already have IDs from markdown_to_html, no need to add them
    let raw_markdown_escaped = html_escape(content);

    // Create JSON array of markdown lines for copy handler
    let markdown_lines_json: String = content
        .lines()
        .map(|line| {
            // Escape for JSON string
            let escaped = line
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('\t', "\\t");
            format!("\"{}\"", escaped)
        })
        .collect::<Vec<_>>()
        .join(",");

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
            <div id="terminal-view" class="view-content" style="display:none;">{}</div>
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
        <span><kbd>⌘C</kbd> Copy MD</span>
        <span><kbd>⇧⌘C</kbd> Copy Formatted</span>
    </div>
    <script src="https://cdnjs.cloudflare.com/ajax/libs/highlight.js/11.9.0/highlight.min.js"></script>
    <script>const markdownLines = [{}];</script>
    <script>{}</script>
</body>
</html>"##,
        CSS,
        rendered_html,
        raw_markdown_escaped,
        toc_html,
        markdown_lines_json,
        JS
    )
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

.github table { border-collapse: collapse; margin-bottom: 16px; }
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
    color: var(--text-primary);
}

#terminal-view .line {
    white-space: pre-wrap;
    word-wrap: break-word;
    min-height: 1.5em;
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
    background: rgba(255, 255, 255, 0.04);
    border-radius: 4px;
    padding: 8px 12px;
}

.md-code-block-wrapper pre {
    margin: 0;
    white-space: pre-wrap;
    word-wrap: break-word;
    font-family: inherit;
    font-size: inherit;
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
    const el = document.querySelector(activeView + ' #' + CSS.escape(slug));
    if (el) {
        el.scrollIntoView({ behavior: 'smooth', block: 'start' });
    }
}

// Extract markdown for selection (used by Cmd+C)
function extractMarkdownForSelection() {
    const selection = window.getSelection();
    if (!selection.rangeCount || selection.isCollapsed) return null;

    try {
        const range = selection.getRangeAt(0);
        let minLine = Infinity;
        let maxLine = 0;

        let node = range.startContainer;
        while (node && node !== document.body) {
            if (node.nodeType === 1 && node.getAttribute && node.getAttribute('data-lines')) {
                const lines = node.getAttribute('data-lines');
                const [start, end] = lines.split('-').map(Number);
                if (start && start < minLine) minLine = start;
                if (end && end > maxLine) maxLine = end;
            }
            node = node.parentNode;
        }

        node = range.endContainer;
        while (node && node !== document.body) {
            if (node.nodeType === 1 && node.getAttribute && node.getAttribute('data-lines')) {
                const lines = node.getAttribute('data-lines');
                const [start, end] = lines.split('-').map(Number);
                if (start && start < minLine) minLine = start;
                if (end && end > maxLine) maxLine = end;
            }
            node = node.parentNode;
        }

        if (minLine !== Infinity && maxLine > 0 && typeof markdownLines !== 'undefined' && markdownLines.length > 0) {
            const markdownBlock = markdownLines.slice(minLine - 1, maxLine).join('\n');
            const selectedText = selection.toString();
            let extracted = markdownBlock;

            if (selectedText.trim()) {
                const words = selectedText.trim().split(/\s+/);
                const firstWords = words.slice(0, 3).join(' ');
                const lastWords = words.slice(-3).join(' ');

                let startIndex = markdownBlock.indexOf(firstWords);
                if (startIndex === -1 && words.length > 0) {
                    startIndex = markdownBlock.indexOf(words[0]);
                }

                if (startIndex !== -1) {
                    let endIndex = markdownBlock.lastIndexOf(lastWords);
                    if (endIndex === -1 && words.length > 0) {
                        endIndex = markdownBlock.lastIndexOf(words[words.length - 1]);
                    }
                    if (endIndex !== -1) {
                        endIndex += (endIndex === markdownBlock.lastIndexOf(lastWords) ? lastWords.length : words[words.length - 1].length);
                    } else {
                        endIndex = markdownBlock.length;
                    }

                    const syntaxChars = /[*_`#|\[\]()>~-]/;
                    let expandedStart = startIndex;
                    while (expandedStart > 0 && syntaxChars.test(markdownBlock[expandedStart - 1])) expandedStart--;
                    while (expandedStart > 0 && markdownBlock[expandedStart - 1] === ' ') expandedStart--;
                    while (expandedStart > 0 && syntaxChars.test(markdownBlock[expandedStart - 1])) expandedStart--;

                    let expandedEnd = endIndex;
                    while (expandedEnd < markdownBlock.length && syntaxChars.test(markdownBlock[expandedEnd])) expandedEnd++;
                    while (expandedEnd < markdownBlock.length && markdownBlock[expandedEnd] === ' ') expandedEnd++;
                    while (expandedEnd < markdownBlock.length && syntaxChars.test(markdownBlock[expandedEnd])) expandedEnd++;

                    // Check for closing code fence - only include if selection reached end of code content
                    const remaining = markdownBlock.substring(expandedEnd);
                    // Only add fence if remaining is ONLY whitespace + fence (nothing else between)
                    const fenceOnlyMatch = remaining.match(/^[\s\n]*```\s*$/);
                    if (fenceOnlyMatch) {
                        expandedEnd = markdownBlock.length;
                    }

                    const candidate = markdownBlock.substring(expandedStart, expandedEnd);
                    if (candidate.length >= selectedText.length * 0.5) {
                        extracted = candidate;
                    }
                }
            }
            return extracted;
        }
    } catch (err) {}
    return null;
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

    // Shift+Cmd+C to copy formatted (HTML)
    if (e.shiftKey && e.metaKey && e.key === 'c') {
        document.execCommand('copy');
        return;
    }

    // Cmd+C to copy markdown source (GitHub view) or plain text (terminal view)
    if (e.metaKey && e.key === 'c') {
        if (currentMode === 'github') {
            const markdown = extractMarkdownForSelection();
            if (markdown && window.ipc) {
                window.ipc.postMessage('clipboard:' + markdown);
                return;
            }
        }
        // Let default copy happen for terminal view or if no markdown found
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
    // Get raw text from initial content (stored in data attribute or parsed from escaped HTML)
    let text = terminalView.textContent;
    let html = '';
    let inCodeBlock = false;
    let codeBlockLang = '';
    let codeBlockLines = [];

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

    // Helper to create a line div with optional padding
    function makeLine(content, indent) {
        const style = indent > 0 ? ' style="padding-left:' + indent + 'ch"' : '';
        return '<div class="line"' + style + '>' + content + '</div>';
    }

    for (let i = 0; i < lines.length; i++) {
        let line = lines[i];

        // Code block start/end
        if (line.match(/^```/)) {
            if (!inCodeBlock) {
                inCodeBlock = true;
                codeBlockLang = line.slice(3).trim();
                html += makeLine('<span class="md-code-fence">' + escapeHtml(line) + '</span>', 0);
                codeBlockLines = [];
            } else {
                // End of code block - render collected content
                if (codeBlockLines.length > 0) {
                    let codeContent;
                    if (codeBlockLang && typeof hljs !== 'undefined') {
                        try {
                            const highlighted = hljs.highlight(codeBlockLines.join('\n'), { language: codeBlockLang, ignoreIllegals: true });
                            codeContent = highlighted.value;
                        } catch (e) {
                            codeContent = escapeHtml(codeBlockLines.join('\n'));
                        }
                    } else {
                        codeContent = escapeHtml(codeBlockLines.join('\n'));
                    }
                    html += '<div class="line md-code-block-wrapper"><pre>' + codeContent + '</pre></div>';
                }
                html += makeLine('<span class="md-code-fence">' + escapeHtml(line) + '</span>', 0);
                inCodeBlock = false;
                codeBlockLang = '';
            }
            continue;
        }

        if (inCodeBlock) {
            codeBlockLines.push(line);
            continue;
        }

        // Check for indentation before escaping
        const indentMatch = line.match(/^(\s+)/);
        const indent = indentMatch ? indentMatch[1].length : 0;
        const content = indent > 0 ? line.slice(indent) : line;

        let processed = escapeHtml(content);

        // Headings - add ID for navigation (use unescaped content for slug to match Rust)
        if (content.match(/^#{1,6}\s/)) {
            const headingText = content.replace(/^#+\s*/, '');
            const slug = slugify(headingText);
            html += makeLine('<span class="md-heading" id="' + slug + '">' + processed + '</span>', indent);
            continue;
        }

        // Horizontal rules
        if (processed.match(/^(-{3,}|\*{3,}|_{3,})$/)) {
            html += makeLine('<span class="md-hr">' + processed + '</span>', indent);
            continue;
        }

        // Blockquotes
        if (processed.match(/^&gt;\s?/)) {
            html += makeLine('<span class="md-blockquote">' + processed + '</span>', indent);
            continue;
        }

        // Table rows
        if (processed.match(/^\|.*\|$/)) {
            if (processed.match(/^\|[\s\-|]+\|$/)) {
                html += makeLine('<span class="md-table-sep">' + processed + '</span>', indent);
            } else {
                let tableLine = processed;
                tableLine = tableLine.replace(/\*\*(.+?)\*\*/g, '<span class="md-bold">&#42;&#42;$1&#42;&#42;</span>');
                tableLine = tableLine.replace(/__(.+?)__/g, '<span class="md-bold">&#95;&#95;$1&#95;&#95;</span>');
                tableLine = tableLine.replace(/\*(.+?)\*/g, '<span class="md-italic">&#42;$1&#42;</span>');
                tableLine = tableLine.replace(/(?<!\w)_(.+?)_(?!\w)/g, '<span class="md-italic">&#95;$1&#95;</span>');
                tableLine = highlightInlineCode(tableLine);
                tableLine = tableLine.replace(/\[([^\]]+)\]\(([^)]+)\)/g, '<a href="$2" class="md-link">[$1]($2)</a>');
                html += makeLine('<span class="md-table">' + tableLine + '</span>', indent);
            }
            continue;
        }

        // List items
        processed = processed.replace(/^([-*+]|\d+\.)\s/, '<span class="md-list-marker">$1</span> ');

        // Inline formatting (use HTML entities for markers to prevent re-matching)
        processed = processed.replace(/\*\*(.+?)\*\*/g, '<span class="md-bold">&#42;&#42;$1&#42;&#42;</span>');
        processed = processed.replace(/__(.+?)__/g, '<span class="md-bold">&#95;&#95;$1&#95;&#95;</span>');
        processed = processed.replace(/\*(.+?)\*/g, '<span class="md-italic">&#42;$1&#42;</span>');
        processed = processed.replace(/(?<!\w)_(.+?)_(?!\w)/g, '<span class="md-italic">&#95;$1&#95;</span>');
        processed = highlightInlineCode(processed);
        processed = processed.replace(/\[([^\]]+)\]\(([^)]+)\)/g, '<a href="$2" class="md-link">[$1]($2)</a>');

        html += makeLine(processed, indent);
    }

    terminalView.innerHTML = html;
}

function highlightInlineCode(text) {
    // Parse backticks properly: count opening backticks, find matching closing sequence
    let result = '';
    let i = 0;
    while (i < text.length) {
        if (text[i] === '\u0060') {
            // Count consecutive backticks
            let backtickCount = 0;
            let start = i;
            while (i < text.length && text[i] === '\u0060') {
                backtickCount++;
                i++;
            }
            // Look for matching closing backticks
            const closer = '\u0060'.repeat(backtickCount);
            const closeIdx = text.indexOf(closer, i);
            if (closeIdx !== -1) {
                // Found matching closer
                const codeContent = text.slice(i, closeIdx);
                result += '<span class="md-code">' + closer + codeContent + closer + '</span>';
                i = closeIdx + backtickCount;
            } else {
                // No matching closer, output backticks as-is
                result += text.slice(start, i);
            }
        } else {
            result += text[i];
            i++;
        }
    }
    return result;
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
