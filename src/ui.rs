use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};
use std::{path::Path, time::SystemTime};

use crate::app::{App, GitStatus, Mode, Preview};
use crate::highlight::highlight_code;

// =============================================================================
// Formatting Helpers
// =============================================================================

/// Formats a file size in human-readable form (B, K, M, G).
pub fn format_size(size: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if size >= GB {
        format!("{:.1}G", size as f64 / GB as f64)
    } else if size >= MB {
        format!("{:.1}M", size as f64 / MB as f64)
    } else if size >= KB {
        format!("{:.1}K", size as f64 / KB as f64)
    } else {
        format!("{}B", size)
    }
}

/// Formats binary data as a hex dump with ASCII representation.
pub fn format_hex(data: &[u8], width: usize) -> String {
    let bytes_per_line = (width.saturating_sub(12)) / 4;
    let bytes_per_line = bytes_per_line.clamp(8, 16);

    data.chunks(bytes_per_line)
        .enumerate()
        .map(|(i, chunk)| {
            let offset = format!("{:08x}  ", i * bytes_per_line);
            let hex: String = chunk.iter().fold(String::new(), |mut acc, b| {
                use std::fmt::Write;
                let _ = write!(acc, "{:02x} ", b);
                acc
            });
            let ascii: String = chunk
                .iter()
                .map(|&b| {
                    if b.is_ascii_graphic() || b == b' ' {
                        b as char
                    } else {
                        '.'
                    }
                })
                .collect();
            format!(
                "{}{:<width$} {}",
                offset,
                hex,
                ascii,
                width = bytes_per_line * 3
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_time(time: Option<SystemTime>) -> String {
    match time {
        Some(t) => {
            let duration = t.duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default();
            let secs = duration.as_secs();

            // Simple date formatting (YYYY-MM-DD HH:MM)
            let days_since_epoch = secs / 86400;
            let time_of_day = secs % 86400;
            let hours = time_of_day / 3600;
            let minutes = (time_of_day % 3600) / 60;

            // Approximate date calculation
            let mut year = 1970;
            let mut remaining_days = days_since_epoch;

            loop {
                let days_in_year = if is_leap_year(year) { 366 } else { 365 };
                if remaining_days < days_in_year {
                    break;
                }
                remaining_days -= days_in_year;
                year += 1;
            }

            let months = [
                31,
                28 + if is_leap_year(year) { 1 } else { 0 },
                31,
                30,
                31,
                30,
                31,
                31,
                30,
                31,
                30,
                31,
            ];
            let mut month = 1;
            for days_in_month in months {
                if remaining_days < days_in_month {
                    break;
                }
                remaining_days -= days_in_month;
                month += 1;
            }
            let day = remaining_days + 1;

            format!(
                "{:04}-{:02}-{:02} {:02}:{:02}",
                year, month, day, hours, minutes
            )
        }
        None => "----".to_string(),
    }
}

fn is_leap_year(year: u64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

// =============================================================================
// UI Rendering
// =============================================================================

fn render_header(path: &Path, mode: &Mode, input: &[char], cursor: usize) -> Paragraph<'static> {
    let input_str: String = input.iter().collect();
    let (content, style, title) = match mode {
        Mode::Search => (
            format!("🔍 {}", input_str),
            Style::default().fg(Color::Yellow),
            "Search",
        ),
        Mode::Rename => {
            let before: String = input.iter().take(cursor).collect();
            let after: String = input.iter().skip(cursor).collect();
            (
                format!("{}│{}", before, after),
                Style::default().fg(Color::Green),
                "Rename",
            )
        }
        Mode::Path => {
            let before: String = input.iter().take(cursor).collect();
            let after: String = input.iter().skip(cursor).collect();
            (
                format!("{}│{}", before, after),
                Style::default().fg(Color::Magenta),
                "Path",
            )
        }
        Mode::NewFile => {
            let before: String = input.iter().take(cursor).collect();
            let after: String = input.iter().skip(cursor).collect();
            (
                format!("{}│{}", before, after),
                Style::default().fg(Color::Green),
                "New File",
            )
        }
        Mode::NewFolder => {
            let before: String = input.iter().take(cursor).collect();
            let after: String = input.iter().skip(cursor).collect();
            (
                format!("{}│{}", before, after),
                Style::default().fg(Color::Blue),
                "New Folder",
            )
        }
        Mode::Normal | Mode::ConfirmDelete => (
            path.to_string_lossy().to_string(),
            Style::default().fg(Color::Cyan),
            "Path",
        ),
    };

    Paragraph::new(content)
        .style(style)
        .block(Block::default().borders(Borders::ALL).title(title))
}

fn render_preview<'a>(preview: &Preview, scroll: u16, width: usize) -> Paragraph<'a> {
    match preview {
        Preview::None => {
            Paragraph::new("").block(Block::default().borders(Borders::ALL).title("Preview"))
        }
        Preview::Directory(items) => {
            let content = if items.is_empty() {
                "(empty directory)".to_string()
            } else {
                items.join("\n")
            };
            Paragraph::new(content)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("Preview (Directory)"),
                )
                .wrap(Wrap { trim: false })
                .scroll((scroll, 0))
        }
        Preview::Text { content, extension } => {
            let title = format_preview_title(extension);
            let lines = highlight_code(content, extension);
            Paragraph::new(lines)
                .block(Block::default().borders(Borders::ALL).title(title))
                .wrap(Wrap { trim: false })
                .scroll((scroll, 0))
        }
        Preview::Image {
            width,
            height,
            format,
        } => {
            let content = format!(
                "\n  Format: {}\n  Dimensions: {} x {} px\n\n  (Image preview not available)",
                format, width, height
            );
            Paragraph::new(content)
                .style(Style::default().fg(Color::Cyan))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("Preview (Image)"),
                )
        }
        Preview::Binary(data) => Paragraph::new(format_hex(data, width))
            .style(Style::default().fg(Color::Yellow))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Preview (Hex)"),
            )
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0)),
        Preview::Error(msg) => Paragraph::new(msg.clone())
            .style(Style::default().fg(Color::Red))
            .block(Block::default().borders(Borders::ALL).title("Preview")),
    }
}

fn format_preview_title(ext: &str) -> String {
    let lang = match ext {
        "rs" => "Rust",
        "py" => "Python",
        "js" => "JavaScript",
        "ts" => "TypeScript",
        "jsx" | "tsx" => "React",
        "go" => "Go",
        "c" | "h" => "C",
        "cpp" | "hpp" | "cc" => "C++",
        "java" => "Java",
        "rb" => "Ruby",
        "php" => "PHP",
        "html" | "htm" => "HTML",
        "css" => "CSS",
        "json" => "JSON",
        "yaml" | "yml" => "YAML",
        "toml" => "TOML",
        "md" => "Markdown",
        "sh" | "bash" => "Shell",
        "sql" => "SQL",
        "xml" => "XML",
        _ => "Text",
    };
    format!("Preview ({})", lang)
}

fn render_help(mode: &Mode) -> Paragraph<'static> {
    let help_text = match mode {
        Mode::Normal => {
            "hjkl:Nav  /:Filter  H:Hidden  c/x/v:Copy/Cut/Paste  n/N:New  r:Rename  d:Del  q:Quit"
        }
        Mode::Path => "Enter:Go  Esc:Cancel",
        Mode::Search => "Enter:Confirm  Esc:Cancel",
        Mode::Rename => "Enter:Confirm  Esc:Cancel",
        Mode::ConfirmDelete => "y:Delete  n:Cancel",
        Mode::NewFile | Mode::NewFolder => "Enter:Create  Esc:Cancel",
    };

    Paragraph::new(help_text)
        .style(Style::default().fg(Color::DarkGray))
        .block(Block::default().borders(Borders::ALL).title("Help"))
}

/// Renders the complete UI to the terminal frame.
pub fn draw_ui(f: &mut Frame, app: &mut App) {
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header/input
            Constraint::Min(0),    // Content
            Constraint::Length(3), // Status bar
            Constraint::Length(3), // Help
        ])
        .split(f.area());

    let content_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(main_chunks[1]);

    let preview_width = content_chunks[1].width.saturating_sub(2) as usize;

    // Collect entry data to avoid borrow conflicts
    let entry_data: Vec<EntryDisplay> = app
        .entries()
        .map(|e| EntryDisplay {
            name: e.name.clone(),
            is_dir: e.is_dir,
            size: e.size,
            is_hidden: e.is_hidden,
            git_status: e.git_status,
        })
        .collect();

    // Get status info before building widgets
    let status_info = app.selected_entry().map(|e| StatusInfo {
        name: e.name.clone(),
        is_dir: e.is_dir,
        size: e.size,
        modified: e.modified,
        is_hidden: e.is_hidden,
        readonly: e.readonly,
    });

    // Build widgets
    let header = render_header(&app.current_dir, &app.mode, &app.input[..], app.cursor);
    let file_list = render_file_list_owned(&entry_data, app.show_hidden);
    let preview = render_preview(&app.preview, app.scroll, preview_width);
    let status = render_status_bar_data(&app.message, &app.mode, status_info.as_ref());
    let help = render_help(&app.mode);

    f.render_widget(header, main_chunks[0]);
    f.render_stateful_widget(file_list, content_chunks[0], &mut app.state);
    f.render_widget(preview, content_chunks[1]);
    f.render_widget(status, main_chunks[2]);
    f.render_widget(help, main_chunks[3]);
}

// Helper structs for owned data
struct EntryDisplay {
    name: String,
    is_dir: bool,
    size: u64,
    is_hidden: bool,
    git_status: Option<GitStatus>,
}

struct StatusInfo {
    name: String,
    is_dir: bool,
    size: u64,
    modified: Option<SystemTime>,
    is_hidden: bool,
    readonly: bool,
}

fn render_file_list_owned(entries: &[EntryDisplay], show_hidden: bool) -> List<'static> {
    let items: Vec<ListItem> = entries
        .iter()
        .map(|entry| {
            let (icon, style) = if entry.is_dir {
                (
                    "📁 ",
                    Style::default()
                        .fg(Color::Blue)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                ("📄 ", Style::default().fg(Color::White))
            };

            let size_str = if entry.is_dir || entry.name == ".." {
                String::new()
            } else {
                format!(" {}", format_size(entry.size))
            };

            let name_style = if entry.is_hidden {
                style.add_modifier(Modifier::DIM)
            } else {
                style
            };

            // Git status indicator
            let (git_indicator, git_style) = match entry.git_status {
                Some(GitStatus::Modified) => (" M", Style::default().fg(Color::Yellow)),
                Some(GitStatus::Staged) => (" S", Style::default().fg(Color::Green)),
                Some(GitStatus::Untracked) => (" ?", Style::default().fg(Color::Red)),
                Some(GitStatus::Conflict) => (" !", Style::default().fg(Color::Magenta)),
                Some(GitStatus::Ignored) => (" I", Style::default().fg(Color::DarkGray)),
                None => ("", Style::default()),
            };

            let content = Line::from(vec![
                Span::raw(icon),
                Span::styled(entry.name.clone(), name_style),
                Span::styled(git_indicator, git_style),
                Span::styled(size_str, Style::default().fg(Color::DarkGray)),
            ]);
            ListItem::new(content)
        })
        .collect();

    let title = if show_hidden {
        "Files (showing hidden)"
    } else {
        "Files"
    };

    List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ")
}

fn render_status_bar_data(
    message: &Option<String>,
    mode: &Mode,
    entry: Option<&StatusInfo>,
) -> Paragraph<'static> {
    if let Some(msg) = message {
        let style = if *mode == Mode::ConfirmDelete {
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Yellow)
        };
        return Paragraph::new(msg.clone())
            .style(style)
            .block(Block::default().borders(Borders::ALL).title("Status"));
    }

    let content = if let Some(e) = entry {
        if e.name == ".." {
            "Parent directory".to_string()
        } else {
            let type_str = if e.is_dir { "DIR" } else { "FILE" };
            let size_str = if e.is_dir {
                String::new()
            } else {
                format!(" │ {}", format_size(e.size))
            };
            let time_str = format_time(e.modified);
            let perm_str = if e.readonly { " │ RO" } else { " │ RW" };
            let hidden_str = if e.is_hidden { " │ hidden" } else { "" };

            format!(
                "{}{} │ {}{}{}",
                type_str, size_str, time_str, perm_str, hidden_str
            )
        }
    } else {
        "No file selected".to_string()
    };

    Paragraph::new(content)
        .style(Style::default().fg(Color::Gray))
        .block(Block::default().borders(Borders::ALL).title("Info"))
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_size_bytes() {
        assert_eq!(format_size(0), "0B");
        assert_eq!(format_size(512), "512B");
        assert_eq!(format_size(1023), "1023B");
    }

    #[test]
    fn test_format_size_kilobytes() {
        assert_eq!(format_size(1024), "1.0K");
        assert_eq!(format_size(1536), "1.5K");
        assert_eq!(format_size(10240), "10.0K");
    }

    #[test]
    fn test_format_size_megabytes() {
        assert_eq!(format_size(1024 * 1024), "1.0M");
        assert_eq!(format_size(5 * 1024 * 1024), "5.0M");
    }

    #[test]
    fn test_format_size_gigabytes() {
        assert_eq!(format_size(1024 * 1024 * 1024), "1.0G");
        assert_eq!(format_size(2 * 1024 * 1024 * 1024), "2.0G");
    }

    #[test]
    fn test_is_leap_year() {
        assert!(is_leap_year(2000)); // divisible by 400
        assert!(!is_leap_year(1900)); // divisible by 100 but not 400
        assert!(is_leap_year(2024)); // divisible by 4 but not 100
        assert!(!is_leap_year(2023)); // not divisible by 4
    }

    #[test]
    fn test_format_preview_title() {
        assert_eq!(format_preview_title("rs"), "Preview (Rust)");
        assert_eq!(format_preview_title("py"), "Preview (Python)");
        assert_eq!(format_preview_title("js"), "Preview (JavaScript)");
        assert_eq!(format_preview_title("unknown"), "Preview (Text)");
    }

    #[test]
    fn test_format_hex() {
        let data = vec![0x48, 0x65, 0x6C, 0x6C, 0x6F]; // "Hello"
        let hex = format_hex(&data, 50);
        assert!(hex.contains("48 65 6c 6c 6f"));
        assert!(hex.contains("Hello"));
    }
}
