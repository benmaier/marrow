// ============================================================================
// IMPORTS & TYPES
// ============================================================================

use pulldown_cmark::{Options, Parser, HeadingLevel, Event, Tag, TagEnd, CodeBlockKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;
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
    RequestOutputLines {
        window_id: WindowId,
        cell_idx: usize,
        output_idx: usize,
        amount: String,
    },
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
            toc_visible: true,
            view_mode: "github".to_string(),
            font_size_level: 0,
            theme: "dark".to_string(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
struct AllSettings {
    #[serde(default)]
    default: Settings,
    #[serde(default)]
    extensions: HashMap<String, Settings>,
}

impl Default for AllSettings {
    fn default() -> Self {
        Self {
            default: Settings::default(),
            extensions: HashMap::new(),
        }
    }
}

impl AllSettings {
    fn get_for_extension(&self, ext: &str) -> &Settings {
        self.extensions.get(ext).unwrap_or(&self.default)
    }

    fn set_for_extension(&mut self, ext: &str, settings: Settings) {
        self.extensions.insert(ext.to_string(), settings);
    }
}

// Jupyter Notebook structures
#[derive(Deserialize)]
struct Notebook {
    cells: Vec<NotebookCell>,
    #[allow(dead_code)]
    metadata: Option<Value>,
}

#[derive(Deserialize)]
struct NotebookCell {
    cell_type: String,
    source: StringOrArray,
    #[serde(default)]
    outputs: Vec<CellOutput>,
    #[allow(dead_code)]
    execution_count: Option<i64>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum StringOrArray {
    String(String),
    Array(Vec<String>),
}

impl StringOrArray {
    fn to_string(&self) -> String {
        match self {
            StringOrArray::String(s) => s.clone(),
            StringOrArray::Array(arr) => arr.join(""),
        }
    }
}

#[derive(Deserialize)]
struct CellOutput {
    output_type: String,
    #[serde(default)]
    text: Option<StringOrArray>,
    #[serde(default)]
    data: Option<HashMap<String, StringOrArray>>,
    #[serde(default)]
    ename: Option<String>,
    #[serde(default)]
    evalue: Option<String>,
    #[serde(default)]
    traceback: Option<Vec<String>>,
}

// Storage for truncated output lines (for "show more" functionality)
#[derive(Clone)]
struct TruncatedOutput {
    full_lines: Vec<String>,  // All lines, pre-escaped HTML
    total_lines: usize,
    shown_lines: usize,       // How many currently shown (100 initially)
}

// ============================================================================
// SETTINGS PERSISTENCE
// ============================================================================

fn get_settings_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("com", "marrow", "app")
        .map(|dirs| dirs.config_dir().join("settings.json"))
}

fn load_settings() -> AllSettings {
    get_settings_path()
        .and_then(|path| std::fs::read_to_string(&path).ok())
        .and_then(|contents| serde_json::from_str(&contents).ok())
        .unwrap_or_default()
}

fn save_settings(settings: &AllSettings) {
    if let Some(path) = get_settings_path() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(settings) {
            let _ = std::fs::write(&path, json);
        }
    }
}

// ============================================================================
// WINDOW MANAGEMENT
// ============================================================================

struct AppWindow {
    window: Arc<Window>,
    webview: WebView,
    file_path: Option<PathBuf>,
    truncated_outputs: HashMap<(usize, usize), TruncatedOutput>,
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
    settings: &Arc<Mutex<AllSettings>>,
    existing_windows: &HashMap<WindowId, AppWindow>,
) -> Result<(WindowId, AppWindow), Box<dyn std::error::Error>> {
    // Extract file extension for per-extension settings
    let extension = path
        .and_then(|p| p.extension())
        .and_then(|e| e.to_str())
        .unwrap_or("md")
        .to_string();

    let all_settings = settings.lock().unwrap();
    let current_settings = all_settings.get_for_extension(&extension).clone();
    drop(all_settings);

    let base_dir = path.and_then(|p| p.parent());
    let is_notebook = extension == "ipynb";

    // Load and render content based on file type
    let (_content, filename, toc, full_html, truncated_outputs) = if is_notebook {
        let filename = path
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("untitled")
            .to_string();

        match path.and_then(|p| std::fs::read_to_string(p).ok()) {
            Some(json_content) => {
                match serde_json::from_str::<Notebook>(&json_content) {
                    Ok(notebook) => {
                        let (notebook_html, toc, truncated) = notebook_to_html(&notebook, base_dir);
                        let html = build_full_html_notebook(&notebook_html, &toc, &current_settings, &extension);
                        (json_content, filename, toc, html, truncated)
                    }
                    Err(e) => {
                        let error_md = format!("# Error\n\nCould not parse notebook: {}", e);
                        let toc = extract_toc(&error_md);
                        let rendered = markdown_to_html(&error_md, base_dir);
                        let html = build_full_html_markdown(&error_md, &rendered, &toc, &current_settings, &extension);
                        (error_md, "Error".to_string(), toc, html, HashMap::new())
                    }
                }
            }
            None => {
                let error_md = "# Error\n\nCould not load file".to_string();
                let toc = extract_toc(&error_md);
                let rendered = markdown_to_html(&error_md, base_dir);
                let html = build_full_html_markdown(&error_md, &rendered, &toc, &current_settings, &extension);
                (error_md, "Error".to_string(), toc, html, HashMap::new())
            }
        }
    } else {
        let (content, filename) = load_file(path);
        let toc = extract_toc(&content);
        let html_content = markdown_to_html(&content, base_dir);
        let full_html = build_full_html_markdown(&content, &html_content, &toc, &current_settings, &extension);
        (content, filename, toc, full_html, HashMap::new())
    };

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
                // Format: "save_settings:ext:{json}" e.g. "save_settings:md:{...}"
                let rest = &msg[14..];
                if let Some(colon_pos) = rest.find(':') {
                    let ext = &rest[..colon_pos];
                    let json = &rest[colon_pos + 1..];
                    if let Ok(new_settings) = serde_json::from_str::<Settings>(json) {
                        let mut all_settings = settings_clone.lock().unwrap();
                        all_settings.set_for_extension(ext, new_settings);
                        save_settings(&all_settings);
                    }
                }
            } else if msg.starts_with("get_output_lines:") {
                // Format: "get_output_lines:cell_idx:output_idx:amount"
                let parts: Vec<&str> = msg[17..].split(':').collect();
                if parts.len() == 3 {
                    let cell_idx: usize = parts[0].parse().unwrap_or(0);
                    let output_idx: usize = parts[1].parse().unwrap_or(0);
                    let amount = parts[2].to_string();
                    let _ = proxy_clone.send_event(UserEvent::RequestOutputLines {
                        window_id,
                        cell_idx,
                        output_idx,
                        amount,
                    });
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
    Ok((window_id, AppWindow { window, webview, file_path, truncated_outputs }))
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
            TaoEvent::UserEvent(UserEvent::RequestOutputLines { window_id, cell_idx, output_idx, amount }) => {
                if let Some(app_window) = windows.get_mut(&window_id) {
                    if let Some(truncated) = app_window.truncated_outputs.get_mut(&(cell_idx, output_idx)) {
                        let (lines_html, hidden_remaining, is_complete) = if amount == "all" {
                            // Send all remaining lines (between shown and tail)
                            let remaining: Vec<_> = truncated.full_lines[truncated.shown_lines..truncated.total_lines - 10].to_vec();
                            let html = remaining.join("\n");
                            (html, 0usize, true)
                        } else {
                            // Send next N lines
                            let n: usize = amount.parse().unwrap_or(50);
                            let end = (truncated.shown_lines + n).min(truncated.total_lines - 10);
                            let lines: Vec<_> = truncated.full_lines[truncated.shown_lines..end].to_vec();
                            let html = lines.join("\n");
                            truncated.shown_lines = end;
                            let hidden = truncated.total_lines - 10 - end;
                            (html, hidden, hidden == 0)
                        };

                        // Call back to JS
                        let js = format!(
                            "receiveOutputLines({}, {}, {}, {}, {})",
                            cell_idx,
                            output_idx,
                            serde_json::to_string(&lines_html).unwrap_or_else(|_| "\"\"".to_string()),
                            hidden_remaining,
                            is_complete
                        );
                        let _ = app_window.webview.evaluate_script(&js);
                    }
                }
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

// ============================================================================
// NOTEBOOK RENDERING
// ============================================================================

fn notebook_to_markdown(notebook: &Notebook) -> String {
    let mut md = String::new();

    for (i, cell) in notebook.cells.iter().enumerate() {
        if i > 0 {
            md.push_str("\n---\n\n");
        }

        match cell.cell_type.as_str() {
            "markdown" => {
                md.push_str(&cell.source.to_string());
                md.push_str("\n\n");
            }
            "code" => {
                // Code cell - wrap source in python fence
                md.push_str("```python\n");
                md.push_str(&cell.source.to_string());
                if !cell.source.to_string().ends_with('\n') {
                    md.push('\n');
                }
                md.push_str("```\n\n");

                // Process outputs
                for output in &cell.outputs {
                    match output.output_type.as_str() {
                        "stream" => {
                            if let Some(text) = &output.text {
                                md.push_str("```\n");
                                md.push_str(&text.to_string());
                                if !text.to_string().ends_with('\n') {
                                    md.push('\n');
                                }
                                md.push_str("```\n\n");
                            }
                        }
                        "execute_result" | "display_data" => {
                            if let Some(data) = &output.data {
                                // Check for image first
                                if let Some(img) = data.get("image/png") {
                                    let b64 = img.to_string().replace('\n', "");
                                    md.push_str(&format!("![output](data:image/png;base64,{})\n\n", b64));
                                } else if let Some(img) = data.get("image/jpeg") {
                                    let b64 = img.to_string().replace('\n', "");
                                    md.push_str(&format!("![output](data:image/jpeg;base64,{})\n\n", b64));
                                } else if let Some(text) = data.get("text/plain") {
                                    md.push_str("```\n");
                                    md.push_str(&text.to_string());
                                    if !text.to_string().ends_with('\n') {
                                        md.push('\n');
                                    }
                                    md.push_str("```\n\n");
                                }
                            }
                        }
                        "error" => {
                            md.push_str("```\n");
                            if let Some(ename) = &output.ename {
                                md.push_str(ename);
                                if let Some(evalue) = &output.evalue {
                                    md.push_str(": ");
                                    md.push_str(evalue);
                                }
                                md.push('\n');
                            }
                            if let Some(tb) = &output.traceback {
                                for line in tb {
                                    // Strip ANSI codes from traceback
                                    let clean = strip_ansi_codes(line);
                                    md.push_str(&clean);
                                    md.push('\n');
                                }
                            }
                            md.push_str("```\n\n");
                        }
                        _ => {}
                    }
                }
            }
            "raw" => {
                md.push_str("```\n");
                md.push_str(&cell.source.to_string());
                if !cell.source.to_string().ends_with('\n') {
                    md.push('\n');
                }
                md.push_str("```\n\n");
            }
            _ => {}
        }
    }

    md
}

fn strip_ansi_codes(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Skip until we hit a letter (end of ANSI sequence)
            while let Some(&next) = chars.peek() {
                chars.next();
                if next.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            result.push(c);
        }
    }
    result
}

// Convert ANSI escape codes to HTML spans with colors
fn ansi_to_html(s: &str) -> String {
    let mut result = String::new();
    let mut in_span = false;
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Parse ANSI sequence
            if chars.peek() == Some(&'[') {
                chars.next(); // consume '['
                let mut code = String::new();
                while let Some(&next) = chars.peek() {
                    if next.is_ascii_digit() || next == ';' {
                        code.push(chars.next().unwrap());
                    } else {
                        chars.next(); // consume the letter (usually 'm')
                        break;
                    }
                }

                // Close current span if open
                if in_span {
                    result.push_str("</span>");
                    in_span = false;
                }

                // Map ANSI code to color
                let color = match code.as_str() {
                    "31" | "0;31" | "1;31" => Some("#e06c75"), // red
                    "32" | "0;32" | "1;32" => Some("#98c379"), // green
                    "33" | "0;33" | "1;33" => Some("#e5c07b"), // yellow
                    "34" | "0;34" | "1;34" => Some("#61afef"), // blue
                    "35" | "0;35" | "1;35" => Some("#c678dd"), // magenta
                    "36" | "0;36" | "1;36" => Some("#56b6c2"), // cyan
                    "37" | "0;37" | "1;37" => Some("#abb2bf"), // white
                    "38;5;160" | "38;5;196" => Some("#e06c75"), // extended red
                    "38;5;28" | "38;5;34" => Some("#98c379"), // extended green
                    _ => None, // reset or unknown
                };

                if let Some(col) = color {
                    result.push_str(&format!("<span style=\"color:{}\">", col));
                    in_span = true;
                }
            }
        } else {
            // Escape HTML characters
            match c {
                '<' => result.push_str("&lt;"),
                '>' => result.push_str("&gt;"),
                '&' => result.push_str("&amp;"),
                _ => result.push(c),
            }
        }
    }

    // Close any open span
    if in_span {
        result.push_str("</span>");
    }
    result
}

// Strip outer <pre style="..."> wrapper from HTML but keep inner content
fn strip_pre_wrapper(html: &str) -> String {
    let trimmed = html.trim();
    // Check if it starts with <pre and ends with </pre>
    if trimmed.starts_with("<pre") && trimmed.ends_with("</pre>") {
        // Find the end of opening tag
        if let Some(end_tag_pos) = trimmed.find('>') {
            let inner = &trimmed[end_tag_pos + 1..trimmed.len() - 6]; // remove "</pre>"
            // Also strip trailing newline in inner content
            return inner.trim_end_matches('\n').to_string();
        }
    }
    html.to_string()
}

/// Convert notebook to native HTML rendering
fn notebook_to_html(notebook: &Notebook, base_dir: Option<&std::path::Path>) -> (String, Vec<(usize, String)>, HashMap<(usize, usize), TruncatedOutput>) {
    let mut html = String::from("<div class=\"notebook\">\n");
    let mut toc: Vec<(usize, String)> = Vec::new();
    let mut truncated_outputs: HashMap<(usize, usize), TruncatedOutput> = HashMap::new();

    for (cell_idx, cell) in notebook.cells.iter().enumerate() {
        match cell.cell_type.as_str() {
            "markdown" => {
                let md_source = cell.source.to_string();
                // Extract headings for TOC
                extract_headings_from_markdown(&md_source, &mut toc);
                // Render markdown using existing function
                let rendered = markdown_to_html(&md_source, base_dir);
                html.push_str(&format!(
                    "<div class=\"nb-cell nb-markdown-cell\" data-cell-idx=\"{}\">\n{}\n</div>\n",
                    cell_idx, rendered
                ));
            }
            "code" => {
                let exec_count = cell.execution_count.map(|n| n.to_string()).unwrap_or_else(|| " ".to_string());
                let source = html_escape(&cell.source.to_string());

                html.push_str(&format!(
                    r#"<div class="nb-cell nb-code-cell" data-cell-idx="{}">
    <div class="nb-cell-header">
        <span class="nb-prompt nb-in">In [{}]:</span>
        <button class="nb-collapse-btn">▼</button>
    </div>
    <div class="nb-input">
        <pre><code class="language-python">{}</code></pre>
    </div>
"#,
                    cell_idx, exec_count, source
                ));

                // Render outputs
                if !cell.outputs.is_empty() {
                    html.push_str("    <div class=\"nb-outputs\">\n");
                    for (output_idx, output) in cell.outputs.iter().enumerate() {
                        if let Some(truncated) = render_output(&mut html, output, &exec_count, cell_idx, output_idx) {
                            truncated_outputs.insert((cell_idx, output_idx), truncated);
                        }
                    }
                    html.push_str("    </div>\n");
                }

                html.push_str("</div>\n");
            }
            "raw" => {
                let source = html_escape(&cell.source.to_string());
                html.push_str(&format!(
                    r#"<div class="nb-cell nb-raw-cell" data-cell-idx="{}">
    <div class="nb-raw-content">{}</div>
</div>
"#,
                    cell_idx, source
                ));
            }
            _ => {}
        }
    }

    html.push_str("</div>\n");

    (html, toc, truncated_outputs)
}

// Helper to render truncated text output with "show more" UI
// Shows first 200 lines + last 10 lines, only if hidden > 80
fn render_truncated_text(
    html: &mut String,
    lines: &[String],
    cell_idx: usize,
    output_idx: usize,
    css_class: &str,
    prompt_html: &str,
) -> TruncatedOutput {
    let total = lines.len();
    let head_lines = &lines[..200];
    let tail_lines = &lines[total - 10..];
    let hidden = total - 210;

    html.push_str(&format!(
        r#"        <div class="{}" data-cell-idx="{}" data-output-idx="{}">
            {}
            <div class="nb-output-content"><div class="nb-output-head">{}</div>
            <div class="nb-output-truncated">
                <span class="nb-truncated-info">{} lines hidden</span>
                <button class="nb-show-more" data-amount="50">Show 50 more</button>
                <button class="nb-show-all">Show all</button>
            </div>
            <div class="nb-output-tail">{}</div></div>
        </div>
"#,
        css_class,
        cell_idx,
        output_idx,
        prompt_html,
        head_lines.join("\n"),
        hidden,
        tail_lines.join("\n")
    ));

    TruncatedOutput {
        full_lines: lines.to_vec(),
        total_lines: total,
        shown_lines: 200,
    }
}

fn render_output(
    html: &mut String,
    output: &CellOutput,
    exec_count: &str,
    cell_idx: usize,
    output_idx: usize,
) -> Option<TruncatedOutput> {
    match output.output_type.as_str() {
        "stream" => {
            if let Some(text) = &output.text {
                let text_str = text.to_string();
                let lines: Vec<String> = text_str.lines().map(|l| html_escape(l)).collect();

                if lines.len() > 290 {
                    return Some(render_truncated_text(
                        html,
                        &lines,
                        cell_idx,
                        output_idx,
                        "nb-output nb-output-stream",
                        "",
                    ));
                } else {
                    let escaped = html_escape(&text_str);
                    html.push_str(&format!(
                        r#"        <div class="nb-output nb-output-stream">
            <div class="nb-output-content">{}</div>
        </div>
"#,
                        escaped
                    ));
                }
            }
        }
        "execute_result" | "display_data" => {
            if let Some(data) = &output.data {
                // Check for images first (prioritize visual output)
                if let Some(img) = data.get("image/png") {
                    let b64 = img.to_string().replace('\n', "");
                    html.push_str(&format!(
                        r#"        <div class="nb-output nb-output-image">
            <img src="data:image/png;base64,{}" class="nb-figure" alt="output">
        </div>
"#,
                        b64
                    ));
                } else if let Some(img) = data.get("image/jpeg") {
                    let b64 = img.to_string().replace('\n', "");
                    html.push_str(&format!(
                        r#"        <div class="nb-output nb-output-image">
            <img src="data:image/jpeg;base64,{}" class="nb-figure" alt="output">
        </div>
"#,
                        b64
                    ));
                } else if let Some(html_content) = data.get("text/html") {
                    // HTML output - no truncation per design decision
                    let html_str = html_content.to_string();
                    let cleaned = strip_pre_wrapper(&html_str);
                    let prompt = if output.output_type == "execute_result" {
                        format!(r#"<div class="nb-output-header"><span class="nb-prompt nb-out">Out[{}]:</span></div>"#, exec_count)
                    } else {
                        String::new()
                    };
                    html.push_str(&format!(
                        r#"        <div class="nb-output nb-output-html">
            {}
            <div class="nb-output-content">{}</div>
        </div>
"#,
                        prompt, cleaned
                    ));
                } else if let Some(text) = data.get("text/plain") {
                    let text_str = text.to_string();
                    let lines: Vec<String> = text_str.lines().map(|l| html_escape(l)).collect();

                    let prompt = if output.output_type == "execute_result" {
                        format!(r#"<div class="nb-output-header"><span class="nb-prompt nb-out">Out[{}]:</span></div>"#, exec_count)
                    } else {
                        String::new()
                    };

                    if lines.len() > 290 {
                        return Some(render_truncated_text(
                            html,
                            &lines,
                            cell_idx,
                            output_idx,
                            "nb-output nb-output-text",
                            &prompt,
                        ));
                    } else {
                        let escaped = html_escape(&text_str);
                        html.push_str(&format!(
                            r#"        <div class="nb-output nb-output-text">
            {}
            <div class="nb-output-content">{}</div>
        </div>
"#,
                            prompt, escaped
                        ));
                    }
                }
            }
        }
        "error" => {
            // Build error lines for potential truncation
            let mut error_lines: Vec<String> = Vec::new();

            if let Some(ename) = &output.ename {
                let mut first_line = format!("<span style=\"color:#e06c75;font-weight:bold\">{}</span>", html_escape(ename));
                if let Some(evalue) = &output.evalue {
                    first_line.push_str(": ");
                    first_line.push_str(&html_escape(evalue));
                }
                error_lines.push(first_line);
            }

            if let Some(tb) = &output.traceback {
                for line in tb {
                    let colored = ansi_to_html(line);
                    error_lines.push(colored);
                }
            }

            if error_lines.len() > 290 {
                return Some(render_truncated_text(
                    html,
                    &error_lines,
                    cell_idx,
                    output_idx,
                    "nb-output nb-output-error",
                    "",
                ));
            } else {
                let error_html = error_lines.join("\n");
                html.push_str(&format!(
                    r#"        <div class="nb-output nb-output-error">
            <div class="nb-output-content">{}</div>
        </div>
"#,
                    error_html
                ));
            }
        }
        _ => {}
    }
    None
}

fn extract_headings_from_markdown(markdown: &str, toc: &mut Vec<(usize, String)>) {
    let options = Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_TASKLISTS;

    let parser = Parser::new_ext(markdown, options);
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
            Event::End(TagEnd::Heading(_)) if in_heading => {
                in_heading = false;
                toc.push((current_level, current_text.clone()));
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
}

// ============================================================================
// MARKDOWN RENDERING
// ============================================================================

fn load_file(path: Option<&PathBuf>) -> (String, String) {
    if let Some(path) = path {
        let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("untitled").to_string();

        // Check if it's a Jupyter notebook
        if path.extension().map(|e| e == "ipynb").unwrap_or(false) {
            match std::fs::read_to_string(path) {
                Ok(contents) => {
                    match serde_json::from_str::<Notebook>(&contents) {
                        Ok(notebook) => (notebook_to_markdown(&notebook), filename),
                        Err(e) => (format!("# Error\n\nCould not parse notebook: {}", e), "Error".to_string()),
                    }
                }
                Err(e) => (format!("# Error\n\nCould not load file: {}", e), "Error".to_string()),
            }
        } else {
            match std::fs::read_to_string(path) {
                Ok(c) => (c, filename),
                Err(e) => (format!("# Error\n\nCould not load file: {}", e), "Error".to_string()),
            }
        }
    } else {
        ("# Welcome to Marrow\n\nOpen a markdown file to get started.\n\nDrag and drop a `.md` or `.ipynb` file or open one with Marrow.".to_string(), "Marrow".to_string())
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

// ============================================================================
// HTML TEMPLATE BUILDING
// ============================================================================

const CSS: &str = include_str!("style.css");
const JS: &str = include_str!("script.js");
const HTML_TEMPLATE: &str = include_str!("template.html");
const HLJS_JS: &str = include_str!("../vendor/highlight.min.js");
const HLJS_CSS: &str = include_str!("../vendor/github-dark.min.css");
const KATEX_JS: &str = include_str!("../vendor/katex.min.js");
const KATEX_CSS: &str = include_str!("../vendor/katex-embedded.min.css");
const KATEX_AUTO: &str = include_str!("../vendor/auto-render.min.js");

fn build_settings_json(settings: &Settings, extension: &str) -> String {
    let mut settings_with_ext = serde_json::to_value(settings).unwrap_or(serde_json::json!({}));
    if let Some(obj) = settings_with_ext.as_object_mut() {
        obj.insert("extension".to_string(), serde_json::json!(extension));
    }
    serde_json::to_string(&settings_with_ext).unwrap_or_else(|_| "{}".to_string())
}

fn build_toc_html(toc: &[(usize, String)]) -> String {
    toc.iter()
        .map(|(level, text)| {
            let slug = slugify(text);
            format!(
                r##"<a href="#" onclick="scrollToHeading('{}'); return false;" class="toc-item toc-level-{}">{}</a>"##,
                slug, level, html_escape(text)
            )
        })
        .collect()
}

fn build_full_html_markdown(content: &str, rendered_html: &str, toc: &[(usize, String)], settings: &Settings, extension: &str) -> String {
    let settings_json = build_settings_json(settings, extension);
    let toc_html = build_toc_html(toc);
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
        .replace("{notebook_view}", "")
        .replace("{md_display}", "block")
        .replace("{nb_display}", "none")
        .replace("{toc}", &toc_html)
        .replace("{markdown_lines}", &markdown_lines_json)
        .replace("{settings}", &settings_json)
        .replace("{js}", JS)
}

fn build_full_html_notebook(notebook_html: &str, toc: &[(usize, String)], settings: &Settings, extension: &str) -> String {
    let settings_json = build_settings_json(settings, extension);
    let toc_html = build_toc_html(toc);

    HTML_TEMPLATE
        .replace("{hljs_css}", HLJS_CSS)
        .replace("{hljs_js}", HLJS_JS)
        .replace("{katex_css}", KATEX_CSS)
        .replace("{katex_js}", KATEX_JS)
        .replace("{katex_auto}", KATEX_AUTO)
        .replace("{css}", CSS)
        .replace("{github_view}", "")
        .replace("{terminal_view}", "")
        .replace("{notebook_view}", notebook_html)
        .replace("{md_display}", "none")
        .replace("{nb_display}", "block")
        .replace("{toc}", &toc_html)
        .replace("{markdown_lines}", "")
        .replace("{settings}", &settings_json)
        .replace("{js}", JS)
}

fn html_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
