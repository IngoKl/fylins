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
// Constants
// =============================================================================

/// Minimum bytes per line in hex dump display
const HEX_DUMP_MIN_BYTES_PER_LINE: usize = 8;

/// Maximum bytes per line in hex dump display
const HEX_DUMP_MAX_BYTES_PER_LINE: usize = 16;

// =============================================================================
// Theme
// =============================================================================

#[derive(Clone, Copy)]
struct Theme {
    accent: Color,
    accent_alt: Color,
    surface_alt: Color,
    muted: Color,
    text: Color,
    selection: Color,
    warning: Color,
}

const THEME: Theme = Theme {
    accent: Color::Cyan,
    accent_alt: Color::Magenta,
    surface_alt: Color::Rgb(24, 26, 32),
    muted: Color::DarkGray,
    text: Color::White,
    selection: Color::Rgb(40, 44, 52),
    warning: Color::Yellow,
};

fn themed_block(title: impl Into<String>, accent: Color) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(accent))
        .title(Span::styled(
            title.into(),
            Style::default().fg(accent).add_modifier(Modifier::BOLD),
        ))
}

fn badge(text: impl Into<String>, fg: Color, bg: Color) -> Span<'static> {
    Span::styled(
        format!(" {} ", text.into()),
        Style::default().fg(fg).bg(bg).add_modifier(Modifier::BOLD),
    )
}

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
    let bytes_per_line = bytes_per_line.clamp(HEX_DUMP_MIN_BYTES_PER_LINE, HEX_DUMP_MAX_BYTES_PER_LINE);

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
    let theme = THEME;
    let input_str: String = input.iter().collect();
    let (content, accent, label) = match mode {
        Mode::Search => (format!("> {}", input_str), Color::Yellow, "Search"),
        Mode::Rename => {
            let before: String = input.iter().take(cursor).collect();
            let after: String = input.iter().skip(cursor).collect();
            (format!("{}|{}", before, after), Color::Green, "Rename")
        }
        Mode::Path => {
            let before: String = input.iter().take(cursor).collect();
            let after: String = input.iter().skip(cursor).collect();
            (format!("{}|{}", before, after), Color::Magenta, "Path")
        }
        Mode::NewFile => {
            let before: String = input.iter().take(cursor).collect();
            let after: String = input.iter().skip(cursor).collect();
            (format!("{}|{}", before, after), Color::Green, "New File")
        }
        Mode::NewFolder => {
            let before: String = input.iter().take(cursor).collect();
            let after: String = input.iter().skip(cursor).collect();
            (format!("{}|{}", before, after), Color::Blue, "New Folder")
        }
        Mode::ConfirmDelete => (
            path.to_string_lossy().to_string(),
            Color::Red,
            "Confirm Delete",
        ),
        Mode::Normal | Mode::Help => (path.to_string_lossy().to_string(), theme.accent, "Path"),
    };

    let spans = vec![
        badge("fylins", Color::Black, theme.accent),
        Span::raw(" "),
        badge(label, Color::Black, accent),
        Span::raw("  "),
        Span::styled(content, Style::default().fg(theme.text)),
    ];

    Paragraph::new(Line::from(spans))
        .style(Style::default().fg(theme.text).bg(theme.surface_alt))
        .block(themed_block("Path", accent))
}

fn render_preview(preview: &Preview, scroll: u16, width: usize) -> Paragraph<'static> {
    let theme = THEME;
    match preview {
        Preview::None => Paragraph::new("Select something to preview")
            .style(Style::default().fg(theme.muted))
            .block(themed_block("Preview", theme.accent))
            .wrap(Wrap { trim: false }),
        Preview::Directory(items) => {
            let content = if items.is_empty() {
                "[ empty directory ]".to_string()
            } else {
                items.join("\n")
            };
            Paragraph::new(content)
                .style(Style::default().fg(theme.text))
                .block(themed_block("Preview (Directory)", theme.accent_alt))
                .wrap(Wrap { trim: false })
                .scroll((scroll, 0))
        }
        Preview::Text { content, extension } => {
            let title = format_preview_title(extension);
            let lines = highlight_code(content, extension);
            Paragraph::new(lines)
                .style(Style::default().fg(theme.text))
                .block(themed_block(title, theme.accent))
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
                .style(Style::default().fg(theme.accent))
                .block(themed_block("Preview (Image)", theme.accent_alt))
        }
        Preview::Binary(data) => Paragraph::new(format_hex(data, width))
            .style(Style::default().fg(theme.warning))
            .block(themed_block("Preview (Hex)", theme.accent_alt))
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0)),
        Preview::Error(msg) => Paragraph::new(msg.clone())
            .style(Style::default().fg(Color::Red))
            .block(themed_block("Preview", Color::Red)),
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

fn help_row(key: &str, desc: &str, theme: &Theme) -> Line<'static> {
    Line::from(vec![
        badge(key, Color::Black, theme.accent),
        Span::raw(" "),
        Span::styled(desc.to_string(), Style::default().fg(theme.text)),
    ])
}

fn push_help_section(
    lines: &mut Vec<Line<'static>>,
    title: &str,
    rows: &[(&str, &str)],
    theme: &Theme,
) {
    lines.push(Line::from(vec![Span::styled(
        title.to_string(),
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD),
    )]));
    for (key, desc) in rows {
        lines.push(help_row(key, desc, theme));
    }
    lines.push(Line::from(""));
}

fn render_help(mode: &Mode) -> Paragraph<'static> {
    let theme = THEME;
    let hints: Vec<(&str, &str)> = match mode {
        Mode::Normal => vec![
            ("hjkl", "move"),
            ("c/x/v", "copy/cut/paste"),
            ("d", "delete"),
            ("n/N", "new"),
            ("q", "quit"),
            ("?", "help"),
        ],
        Mode::Path => vec![("Enter", "go"), ("Esc", "cancel")],
        Mode::Search => vec![("Enter", "confirm"), ("Esc", "cancel")],
        Mode::Rename => vec![("Enter", "confirm"), ("Esc", "cancel")],
        Mode::ConfirmDelete => vec![("y", "delete"), ("n/Esc", "cancel")],
        Mode::NewFile | Mode::NewFolder => vec![("Enter", "create"), ("Esc", "cancel")],
        Mode::Help => vec![("?", "close"), ("Esc", "close")],
    };

    let mut spans: Vec<Span> = Vec::new();
    for (i, (key, desc)) in hints.iter().enumerate() {
        spans.push(badge(*key, Color::Black, theme.accent));
        spans.push(Span::styled(
            format!(" {}", desc),
            Style::default().fg(theme.text),
        ));
        if i != hints.len() - 1 {
            spans.push(Span::raw("  "));
        }
    }

    Paragraph::new(Line::from(spans))
        .style(Style::default().fg(theme.muted))
        .block(themed_block("Help", theme.accent))
}

fn render_help_screen<'a>() -> Paragraph<'a> {
    let theme = THEME;
    let mut lines: Vec<Line<'static>> = Vec::new();

    lines.push(Line::from(vec![
        Span::styled(
            "FYLINS",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("  Terminal file browser", Style::default().fg(theme.muted)),
    ]));
    lines.push(Line::from(""));

    push_help_section(
        &mut lines,
        "Navigation",
        &[
            ("j/k or up/down", "Move selection"),
            ("l or Enter", "Open or enter"),
            ("h or Backspace", "Parent directory"),
            ("`", "Go to start directory"),
            ("PgUp/PgDn", "Scroll preview"),
        ],
        &theme,
    );

    push_help_section(
        &mut lines,
        "File actions",
        &[
            ("c / x / v", "Copy / Cut / Paste"),
            ("n / N", "New file / folder"),
            ("r", "Rename"),
            ("d", "Delete"),
            ("o", "Open with default app"),
            ("y", "Copy path to clipboard"),
        ],
        &theme,
    );

    push_help_section(
        &mut lines,
        "View & filter",
        &[
            ("/", "Search or filter"),
            ("H", "Toggle hidden files"),
            ("p", "Jump to path"),
        ],
        &theme,
    );

    push_help_section(
        &mut lines,
        "Git status",
        &[
            ("M", "Modified"),
            ("S", "Staged"),
            ("?", "Untracked"),
            ("!", "Conflict"),
            ("I", "Ignored"),
        ],
        &theme,
    );

    push_help_section(
        &mut lines,
        "Other",
        &[("?", "Toggle help"), ("q or Esc", "Quit")],
        &theme,
    );

    lines.push(Line::from(vec![Span::styled(
        "Press ? or Esc to close this help",
        Style::default().fg(theme.muted),
    )]));

    Paragraph::new(lines)
        .style(Style::default().fg(theme.text))
        .block(themed_block("Help", theme.accent))
        .wrap(Wrap { trim: false })
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
    let help = render_help(&app.mode);

    // If in help mode, show help screen instead of file list and preview
    if app.mode == Mode::Help {
        let help_screen = render_help_screen();
        f.render_widget(header, main_chunks[0]);
        f.render_widget(help_screen, main_chunks[1]);
        f.render_widget(help, main_chunks[3]);
    } else {
        let file_list = render_file_list_owned(&entry_data, app.show_hidden);
        let preview = render_preview(&app.preview, app.scroll, preview_width);
        let status = render_status_bar_data(&app.message, &app.mode, status_info.as_ref());

        f.render_widget(header, main_chunks[0]);
        f.render_stateful_widget(file_list, content_chunks[0], &mut app.state);
        f.render_widget(preview, content_chunks[1]);
        f.render_widget(status, main_chunks[2]);
        f.render_widget(help, main_chunks[3]);
    }
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
    let theme = THEME;
    let items: Vec<ListItem> = entries
        .iter()
        .map(|entry| {
            let icon = if entry.is_dir {
                Span::styled("> ", Style::default().fg(theme.accent))
            } else {
                Span::styled("- ", Style::default().fg(theme.muted))
            };

            let base_style = if entry.is_dir {
                Style::default().fg(theme.text).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text)
            };

            let name_style = if entry.is_hidden {
                base_style.add_modifier(Modifier::DIM)
            } else {
                base_style
            };

            let git_indicator = match entry.git_status {
                Some(GitStatus::Modified) => Some(badge("M", Color::Black, Color::Yellow)),
                Some(GitStatus::Staged) => Some(badge("S", Color::Black, Color::Green)),
                Some(GitStatus::Untracked) => Some(badge("?", Color::Black, Color::Red)),
                Some(GitStatus::Conflict) => Some(badge("!", Color::Black, Color::Magenta)),
                Some(GitStatus::Ignored) => Some(badge("I", Color::Black, Color::DarkGray)),
                None => None,
            };

            let mut spans = vec![icon, Span::styled(entry.name.clone(), name_style)];
            if let Some(badge) = git_indicator {
                spans.push(Span::raw(" "));
                spans.push(badge);
            }
            if !entry.is_dir && entry.name != ".." {
                spans.push(Span::raw("  "));
                spans.push(Span::styled(
                    format!("{:>7}", format_size(entry.size)),
                    Style::default().fg(theme.muted),
                ));
            }

            ListItem::new(Line::from(spans))
        })
        .collect();

    let title = if show_hidden {
        "Files (showing hidden)"
    } else {
        "Files"
    };

    List::new(items)
        .block(themed_block(title, theme.accent))
        .highlight_style(
            Style::default()
                .bg(theme.selection)
                .fg(theme.text)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ")
}

fn render_status_bar_data(
    message: &Option<String>,
    mode: &Mode,
    entry: Option<&StatusInfo>,
) -> Paragraph<'static> {
    let theme = THEME;

    if let Some(msg) = message {
        let is_delete = *mode == Mode::ConfirmDelete;
        let status_badge = if is_delete {
            badge(msg.clone(), Color::White, Color::Red)
        } else {
            badge(msg.clone(), Color::Black, theme.warning)
        };
        let accent = if is_delete { Color::Red } else { theme.accent };
        return Paragraph::new(Line::from(vec![status_badge]))
            .block(themed_block("Status", accent));
    }

    let mut spans: Vec<Span> = Vec::new();

    if let Some(e) = entry {
        if e.name == ".." {
            spans.push(Span::styled(
                "Parent directory",
                Style::default().fg(theme.muted),
            ));
        } else {
            let entry_badge = if e.is_dir {
                badge("DIR", Color::Black, theme.accent)
            } else {
                badge("FILE", Color::Black, theme.accent_alt)
            };
            spans.push(entry_badge);

            if !e.is_dir {
                spans.push(Span::raw(" "));
                spans.push(Span::styled(
                    format_size(e.size),
                    Style::default().fg(theme.text),
                ));
            }

            spans.push(Span::raw("  "));
            spans.push(Span::styled(
                format_time(e.modified),
                Style::default().fg(theme.muted),
            ));

            let perm = if e.readonly { "RO" } else { "RW" };
            spans.push(Span::raw("  "));
            spans.push(badge(perm, Color::Black, theme.accent));

            if e.is_hidden {
                spans.push(Span::raw(" "));
                spans.push(badge("hidden", Color::White, theme.muted));
            }
        }
    } else {
        spans.push(Span::styled(
            "No file selected",
            Style::default().fg(theme.muted),
        ));
    }

    Paragraph::new(Line::from(spans))
        .style(Style::default().fg(theme.text))
        .block(themed_block("Info", theme.accent))
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
