use pulldown_cmark::{Options, Parser, HeadingLevel, Event, Tag, TagEnd, CodeBlockKind};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tao::{
    dpi::{LogicalSize, PhysicalPosition},
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

#[derive(Serialize, Deserialize, Clone)]
struct Settings {
    window_width: f64,
    window_height: f64,
    toc_visible: bool,
    view_mode: String,
    font_size_level: i32,
    theme: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            window_width: 800.0,
            window_height: 900.0,
            toc_visible: false,
            view_mode: "github".to_string(),
            font_size_level: 0,
            theme: "dark".to_string(),
        }
    }
}

fn get_settings_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("com", "marrow", "app")
        .map(|dirs| dirs.config_dir().join("settings.json"))
}

fn load_settings() -> Settings {
    get_settings_path()
        .and_then(|path| std::fs::read_to_string(&path).ok())
        .and_then(|contents| serde_json::from_str(&contents).ok())
        .unwrap_or_default()
}

fn save_settings(settings: &Settings) {
    if let Some(path) = get_settings_path() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(settings) {
            let _ = std::fs::write(&path, json);
        }
    }
}

struct AppWindow {
    window: Arc<Window>,
    _webview: WebView,
    file_path: Option<PathBuf>,
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

fn calculate_cascade_position(existing: &HashMap<WindowId, AppWindow>) -> Option<PhysicalPosition<i32>> {
    if existing.is_empty() {
        return None;
    }

    // Find the window furthest down-right and offset from it
    let mut max_offset: i32 = 0;
    let mut base_pos: Option<(i32, i32)> = None;

    for app_window in existing.values() {
        if let Ok(pos) = app_window.window.outer_position() {
            let offset = pos.x + pos.y;
            if offset >= max_offset {
                max_offset = offset;
                base_pos = Some((pos.x, pos.y));
            }
        }
    }

    // Offset right and down (50 physical pixels each)
    base_pos.map(|(x, y)| PhysicalPosition::new(x + 50, y + 50))
}

fn find_window_for_path(windows: &HashMap<WindowId, AppWindow>, path: &PathBuf) -> Option<WindowId> {
    for (id, app_window) in windows {
        if let Some(ref existing_path) = app_window.file_path {
            if existing_path == path {
                return Some(*id);
            }
        }
    }
    None
}

fn create_window(
    event_loop: &EventLoopWindowTarget<UserEvent>,
    proxy: EventLoopProxy<UserEvent>,
    path: Option<&PathBuf>,
    settings: &Arc<Mutex<Settings>>,
    existing_windows: &HashMap<WindowId, AppWindow>,
) -> Result<(WindowId, AppWindow), Box<dyn std::error::Error>> {
    let current_settings = settings.lock().unwrap().clone();

    let (content, filename) = load_file(path);
    let base_dir = path.and_then(|p| p.parent());
    let toc = extract_toc(&content);
    let html_content = markdown_to_html(&content, base_dir);
    let full_html = build_full_html(&content, &html_content, &toc, &filename, &current_settings);

    // Build window title: "First Heading · filename · Marrow 🦴"
    let first_heading = toc.first().map(|(_, text)| truncate_end(text, 20));
    let short_filename = truncate_middle(&filename, 20);
    let title = match first_heading {
        Some(heading) => format!("{} · {} · Marrow 🦴", heading, short_filename),
        None => format!("{} · Marrow 🦴", short_filename),
    };

    // Calculate window size (use settings, add TOC width if visible)
    let width = current_settings.window_width + if current_settings.toc_visible { 200.0 } else { 0.0 };
    let height = current_settings.window_height;

    let builder = WindowBuilder::new()
        .with_title(title)
        .with_inner_size(LogicalSize::new(width, height));

    let window = builder.build(event_loop)?;

    // Apply cascade position after window is created
    if let Some(pos) = calculate_cascade_position(existing_windows) {
        window.set_outer_position(pos);
    }
    let window = Arc::new(window);
    let window_clone = Arc::clone(&window);
    let window_id = window.id();
    let proxy_clone = proxy.clone();
    let settings_clone = Arc::clone(settings);

    // Clone base_dir for navigation handler
    let nav_base_dir = base_dir.map(|p| p.to_path_buf());

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
            } else if msg.starts_with("save_settings:") {
                // Format: "save_settings:{json}"
                let json = &msg[14..];
                if let Ok(new_settings) = serde_json::from_str::<Settings>(json) {
                    let mut settings = settings_clone.lock().unwrap();
                    *settings = new_settings;
                    save_settings(&settings);
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
        .with_navigation_handler(move |url| {
            // Allow internal navigation
            if url.starts_with("about:") || url.starts_with("data:") {
                return true;
            }
            // Open http/https links in default browser
            if url.starts_with("http://") || url.starts_with("https://") {
                let _ = std::process::Command::new("open").arg(&url).spawn();
                return false;
            }
            // Handle file:// URLs
            if let Some(file_path) = url.strip_prefix("file://") {
                let decoded = urlencoding::decode(file_path).unwrap_or_else(|_| file_path.into());
                let path = PathBuf::from(decoded.as_ref());
                if path.exists() {
                    let _ = std::process::Command::new("open").arg(&path).spawn();
                }
                return false;
            }
            // Local file link - resolve relative to markdown file's directory
            if let Some(ref base) = nav_base_dir {
                let decoded = urlencoding::decode(&url).unwrap_or_else(|_| url.clone().into());
                let path = base.join(decoded.as_ref());
                if path.exists() {
                    let _ = std::process::Command::new("open").arg(&path).spawn();
                    return false;
                }
            }
            // Block navigation to unknown URLs
            false
        })
        .build(&window)?;

    let file_path = path.cloned();
    Ok((window_id, AppWindow { window, _webview: webview, file_path }))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let initial_path = std::env::args().nth(1).map(|arg| {
        let path = PathBuf::from(&arg);
        path.canonicalize().unwrap_or(path)
    });

    // Load persistent settings
    let settings = Arc::new(Mutex::new(load_settings()));

    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    let mut windows: HashMap<WindowId, AppWindow> = HashMap::new();

    // Only create initial window if a file was passed via command line
    if let Some(ref path) = initial_path {
        let (id, app_window) = create_window(&event_loop, proxy.clone(), Some(path), &settings, &windows)?;
        windows.insert(id, app_window);
    }

    event_loop.run(move |event, event_loop, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            TaoEvent::Opened { urls } => {
                for url in urls {
                    if let Ok(path) = url.to_file_path() {
                        // Check if file is already open
                        if let Some(existing_id) = find_window_for_path(&windows, &path) {
                            // Focus the existing window
                            if let Some(app_window) = windows.get(&existing_id) {
                                app_window.window.set_focus();
                            }
                        } else {
                            // Create new window
                            if let Ok((id, app_window)) = create_window(event_loop, proxy.clone(), Some(&path), &settings, &windows) {
                                windows.insert(id, app_window);
                            }
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

fn resolve_image_url(url: &str, base_dir: Option<&std::path::Path>) -> String {
    use base64::{Engine as _, engine::general_purpose::STANDARD};

    // Already absolute URL or data URI
    if url.starts_with("http://") || url.starts_with("https://")
        || url.starts_with("file://") || url.starts_with("data:") {
        return url.to_string();
    }

    // Try to resolve relative path and embed as data URI
    if let Some(base) = base_dir {
        let path = base.join(url);
        if path.exists() {
            if let Ok(data) = std::fs::read(&path) {
                let mime = get_mime_type(&path);
                let b64 = STANDARD.encode(&data);
                return format!("data:{};base64,{}", mime, b64);
            }
        }
    }

    // Return as-is if we can't resolve
    url.to_string()
}

fn get_mime_type(path: &std::path::Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()).as_deref() {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("svg") => "image/svg+xml",
        Some("webp") => "image/webp",
        Some("ico") => "image/x-icon",
        Some("bmp") => "image/bmp",
        _ => "application/octet-stream",
    }
}

fn markdown_to_html(markdown: &str, base_dir: Option<&std::path::Path>) -> String {
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
                if lang == Some("math") {
                    // Math block - render for KaTeX processing
                    html_output.push_str(&format!(r#"<div class="math-block" data-lines="{}-__MATH_END__">$$"#, start_line));
                    tag_stack.push("math".to_string());
                } else if let Some(lang) = lang {
                    html_output.push_str(&format!(r#"<pre data-lines="{}-__PRE_END__"><code class="language-{}">"#, start_line, lang));
                    tag_stack.push("pre".to_string());
                } else {
                    html_output.push_str(&format!(r#"<pre data-lines="{}-__PRE_END__"><code>"#, start_line));
                    tag_stack.push("pre".to_string());
                }
            }
            Event::End(TagEnd::CodeBlock) => {
                let tag_type = tag_stack.pop().unwrap_or_default();
                if tag_type == "math" {
                    html_output.push_str("$$</div>\n");
                    if let Some(pos) = html_output.rfind("__MATH_END__") {
                        html_output.replace_range(pos..pos + 12, &(end_line + 1).to_string());
                    }
                } else {
                    html_output.push_str("</code></pre>\n");
                    if let Some(pos) = html_output.rfind("__PRE_END__") {
                        // Add 1 to include the closing ``` fence line
                        html_output.replace_range(pos..pos + 11, &(end_line + 1).to_string());
                    }
                }
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
                let resolved_url = resolve_image_url(&dest_url, base_dir);
                let mut img_html = format!(r#"<img src="{}" alt=""#, resolved_url);
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

const CSS: &str = include_str!("style.css");
const JS: &str = include_str!("script.js");
const HTML_TEMPLATE: &str = include_str!("template.html");
const HLJS_JS: &str = include_str!("../vendor/highlight.min.js");
const HLJS_CSS: &str = include_str!("../vendor/github-dark.min.css");
const KATEX_JS: &str = include_str!("../vendor/katex.min.js");
const KATEX_CSS: &str = include_str!("../vendor/katex-embedded.min.css");
const KATEX_AUTO: &str = include_str!("../vendor/auto-render.min.js");

fn build_full_html(content: &str, rendered_html: &str, toc: &[(usize, String)], _filename: &str, settings: &Settings) -> String {
    let settings_json = serde_json::to_string(settings).unwrap_or_else(|_| "{}".to_string());

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

    let raw_markdown_escaped = html_escape(content);

    // Create JSON array of markdown lines for copy handler
    let markdown_lines_json: String = content
        .lines()
        .map(|line| {
            let escaped = line
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('\t', "\\t");
            format!("\"{}\"", escaped)
        })
        .collect::<Vec<_>>()
        .join(",");

    HTML_TEMPLATE
        .replace("{hljs_css}", HLJS_CSS)
        .replace("{hljs_js}", HLJS_JS)
        .replace("{katex_css}", KATEX_CSS)
        .replace("{katex_js}", KATEX_JS)
        .replace("{katex_auto}", KATEX_AUTO)
        .replace("{css}", CSS)
        .replace("{github_view}", rendered_html)
        .replace("{terminal_view}", &raw_markdown_escaped)
        .replace("{toc}", &toc_html)
        .replace("{markdown_lines}", &markdown_lines_json)
        .replace("{settings}", &settings_json)
        .replace("{js}", JS)
}

fn html_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
