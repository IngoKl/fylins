use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

/// Highlights code content based on file extension.
///
/// Returns a vector of styled lines suitable for rendering in ratatui.
pub fn highlight_code(content: &str, ext: &str) -> Vec<Line<'static>> {
    let keywords = get_keywords(ext);
    let types = get_types(ext);

    content
        .lines()
        .map(|line| highlight_line(line, &keywords, &types, ext))
        .collect()
}

fn get_keywords(ext: &str) -> Vec<&'static str> {
    match ext {
        "rs" => vec![
            "fn", "let", "mut", "const", "pub", "use", "mod", "struct", "enum", "impl", "trait",
            "where", "for", "if", "else", "match", "loop", "while", "return", "break", "continue",
            "async", "await", "move", "ref", "self", "Self", "super", "crate", "dyn", "static",
            "type", "unsafe", "extern",
        ],
        "py" => vec![
            "def", "class", "if", "elif", "else", "for", "while", "return", "import", "from", "as",
            "try", "except", "finally", "with", "yield", "lambda", "pass", "break", "continue",
            "raise", "assert", "global", "nonlocal", "async", "await",
        ],
        "js" | "ts" | "jsx" | "tsx" => vec![
            "function",
            "const",
            "let",
            "var",
            "if",
            "else",
            "for",
            "while",
            "return",
            "class",
            "extends",
            "import",
            "export",
            "from",
            "default",
            "async",
            "await",
            "try",
            "catch",
            "finally",
            "throw",
            "new",
            "this",
            "super",
            "typeof",
            "instanceof",
        ],
        "go" => vec![
            "func",
            "var",
            "const",
            "type",
            "struct",
            "interface",
            "if",
            "else",
            "for",
            "range",
            "return",
            "break",
            "continue",
            "switch",
            "case",
            "default",
            "go",
            "chan",
            "select",
            "defer",
            "package",
            "import",
            "map",
        ],
        "c" | "h" | "cpp" | "hpp" | "cc" => vec![
            "if",
            "else",
            "for",
            "while",
            "do",
            "switch",
            "case",
            "default",
            "return",
            "break",
            "continue",
            "struct",
            "union",
            "enum",
            "typedef",
            "sizeof",
            "static",
            "const",
            "extern",
            "void",
            "class",
            "public",
            "private",
            "protected",
            "virtual",
            "template",
            "namespace",
            "using",
            "new",
            "delete",
        ],
        "java" => vec![
            "class",
            "interface",
            "extends",
            "implements",
            "if",
            "else",
            "for",
            "while",
            "do",
            "switch",
            "case",
            "default",
            "return",
            "break",
            "continue",
            "new",
            "this",
            "super",
            "public",
            "private",
            "protected",
            "static",
            "final",
            "abstract",
            "void",
            "import",
            "package",
            "try",
            "catch",
            "finally",
            "throw",
            "throws",
        ],
        "sh" | "bash" => vec![
            "if", "then", "else", "elif", "fi", "for", "while", "do", "done", "case", "esac",
            "function", "return", "exit", "export", "local", "readonly",
        ],
        _ => vec![],
    }
}

fn get_types(ext: &str) -> Vec<&'static str> {
    match ext {
        "rs" => vec![
            "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize",
            "f32", "f64", "bool", "char", "str", "String", "Vec", "Option", "Result", "Box", "Rc",
            "Arc", "HashMap", "HashSet", "PathBuf",
        ],
        "py" => vec![
            "int", "float", "str", "bool", "list", "dict", "tuple", "set", "None", "True", "False",
        ],
        "js" | "ts" | "jsx" | "tsx" => vec![
            "string",
            "number",
            "boolean",
            "null",
            "undefined",
            "true",
            "false",
            "Array",
            "Object",
            "Promise",
            "void",
            "any",
            "never",
        ],
        "go" => vec![
            "int", "int8", "int16", "int32", "int64", "uint", "uint8", "uint16", "uint32",
            "uint64", "float32", "float64", "bool", "string", "byte", "rune", "error", "true",
            "false", "nil",
        ],
        "c" | "h" | "cpp" | "hpp" | "cc" => vec![
            "int", "char", "float", "double", "long", "short", "unsigned", "signed", "bool",
            "true", "false", "NULL", "nullptr", "auto",
        ],
        "java" => vec![
            "int", "long", "short", "byte", "float", "double", "boolean", "char", "String", "true",
            "false", "null", "void",
        ],
        _ => vec![],
    }
}

fn highlight_line(line: &str, keywords: &[&str], types: &[&str], ext: &str) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut current_word = String::new();
    let mut current_other = String::new();
    let mut in_string = false;
    let mut string_char = '"';

    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];

        // Check for comment start
        if !in_string {
            if c == '/' && i + 1 < chars.len() && chars[i + 1] == '/' {
                // Flush current content
                if !current_word.is_empty() {
                    spans.push(colorize_word(&current_word, keywords, types));
                    current_word.clear();
                }
                if !current_other.is_empty() {
                    spans.push(Span::raw(current_other.clone()));
                    current_other.clear();
                }
                // Rest of line is comment
                let comment: String = chars[i..].iter().collect();
                spans.push(Span::styled(comment, Style::default().fg(Color::DarkGray)));
                break;
            }
            if c == '#' && matches!(ext, "py" | "sh" | "bash" | "yaml" | "yml" | "toml") {
                if !current_word.is_empty() {
                    spans.push(colorize_word(&current_word, keywords, types));
                    current_word.clear();
                }
                if !current_other.is_empty() {
                    spans.push(Span::raw(current_other.clone()));
                    current_other.clear();
                }
                let comment: String = chars[i..].iter().collect();
                spans.push(Span::styled(comment, Style::default().fg(Color::DarkGray)));
                break;
            }
        }

        // Handle strings
        if c == '"' || c == '\'' {
            if !in_string {
                if !current_word.is_empty() {
                    spans.push(colorize_word(&current_word, keywords, types));
                    current_word.clear();
                }
                if !current_other.is_empty() {
                    spans.push(Span::raw(current_other.clone()));
                    current_other.clear();
                }
                in_string = true;
                string_char = c;
                current_other.push(c);
            } else if c == string_char {
                current_other.push(c);
                spans.push(Span::styled(
                    current_other.clone(),
                    Style::default().fg(Color::Green),
                ));
                current_other.clear();
                in_string = false;
            } else {
                current_other.push(c);
            }
            i += 1;
            continue;
        }

        if in_string {
            current_other.push(c);
            i += 1;
            continue;
        }

        // Handle words vs other characters
        if c.is_alphanumeric() || c == '_' {
            if !current_other.is_empty() {
                spans.push(Span::raw(current_other.clone()));
                current_other.clear();
            }
            current_word.push(c);
        } else {
            if !current_word.is_empty() {
                spans.push(colorize_word(&current_word, keywords, types));
                current_word.clear();
            }
            current_other.push(c);
        }

        i += 1;
    }

    // Flush remaining content
    if !current_word.is_empty() {
        spans.push(colorize_word(&current_word, keywords, types));
    }
    if !current_other.is_empty() {
        if in_string {
            spans.push(Span::styled(
                current_other,
                Style::default().fg(Color::Green),
            ));
        } else {
            spans.push(Span::raw(current_other));
        }
    }

    Line::from(spans)
}

fn colorize_word(word: &str, keywords: &[&str], types: &[&str]) -> Span<'static> {
    if keywords.contains(&word) {
        Span::styled(
            word.to_string(),
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        )
    } else if types.contains(&word) {
        Span::styled(word.to_string(), Style::default().fg(Color::Cyan))
    } else if word.chars().all(|c| c.is_ascii_digit()) {
        Span::styled(word.to_string(), Style::default().fg(Color::Yellow))
    } else {
        Span::raw(word.to_string())
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_keywords_rust() {
        let keywords = get_keywords("rs");
        assert!(keywords.contains(&"fn"));
        assert!(keywords.contains(&"let"));
        assert!(keywords.contains(&"struct"));
    }

    #[test]
    fn test_get_keywords_python() {
        let keywords = get_keywords("py");
        assert!(keywords.contains(&"def"));
        assert!(keywords.contains(&"class"));
        assert!(keywords.contains(&"import"));
    }

    #[test]
    fn test_get_types_rust() {
        let types = get_types("rs");
        assert!(types.contains(&"String"));
        assert!(types.contains(&"Vec"));
        assert!(types.contains(&"Option"));
    }

    #[test]
    fn test_colorize_word_keyword() {
        let keywords = vec!["fn", "let"];
        let types = vec!["String"];
        let span = colorize_word("fn", &keywords, &types);
        // Verify it returns a styled span (not raw)
        assert!(format!("{:?}", span).contains("Magenta"));
    }

    #[test]
    fn test_colorize_word_type() {
        let keywords = vec!["fn"];
        let types = vec!["String"];
        let span = colorize_word("String", &keywords, &types);
        assert!(format!("{:?}", span).contains("Cyan"));
    }

    #[test]
    fn test_colorize_word_number() {
        let keywords: Vec<&str> = vec![];
        let types: Vec<&str> = vec![];
        let span = colorize_word("42", &keywords, &types);
        assert!(format!("{:?}", span).contains("Yellow"));
    }
}
