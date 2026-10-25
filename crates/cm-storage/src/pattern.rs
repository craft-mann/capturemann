//! Folder patterns such as `CaptureMann/{yyyy}/{MM}/{title}`.

/// Date parts for a pattern (local time of the capture).
#[derive(Debug, Clone, Copy)]
pub struct DateParts {
    pub year: i32,
    pub month: u8,
    pub day: u8,
}

/// Expands `{yyyy}`, `{MM}`, `{dd}` and `{title}`. Each segment is made safe: path separators
/// and control characters in the title become `_`, and `.`/`..` segments are dropped, so the
/// result always stays below the destination root. Unknown tokens are kept as written.
pub fn expand_pattern(pattern: &str, date: DateParts, title: &str) -> String {
    let title = sanitize_segment(title);
    let expanded = pattern
        .replace("{yyyy}", &format!("{:04}", date.year))
        .replace("{MM}", &format!("{:02}", date.month))
        .replace("{dd}", &format!("{:02}", date.day))
        .replace("{title}", &title);
    expanded
        .split(['/', '\\'])
        .map(str::trim)
        .filter(|s| !s.is_empty() && *s != "." && *s != "..")
        .collect::<Vec<_>>()
        .join("/")
}

/// `name`, then `name (2)`, `name (3)`, ... with the number before the extension
/// (`Title.cmrec`, `Title (2).cmrec`). The name is sanitised first.
pub fn numbered_names(name: &str) -> impl Iterator<Item = String> {
    let name = sanitize_segment(name);
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (name[..i].to_owned(), name[i..].to_owned()),
        _ => (name.clone(), String::new()),
    };
    (1u32..10_000).map(move |n| {
        if n == 1 {
            name.clone()
        } else {
            format!("{stem} ({n}){ext}")
        }
    })
}

pub fn sanitize_segment(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '_'
            } else {
                c
            }
        })
        .collect();
    let cleaned = cleaned.trim().trim_matches('.').to_string();
    if cleaned.is_empty() {
        "untitled".into()
    } else {
        cleaned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const D: DateParts = DateParts {
        year: 2026,
        month: 10,
        day: 3,
    };

    #[test]
    fn default_pattern() {
        assert_eq!(
            expand_pattern("CaptureMann/{yyyy}/{MM}/{title}", D, "Lesson 1"),
            "CaptureMann/2026/10/Lesson 1"
        );
    }

    #[test]
    fn title_cannot_escape_the_root() {
        assert_eq!(expand_pattern("{title}", D, "../../etc"), "_.._etc");
    }

    #[test]
    fn dotdot_segments_dropped() {
        assert_eq!(expand_pattern("a/../b/./c", D, "x"), "a/b/c");
    }

    #[test]
    fn numbered_names_put_the_number_before_the_extension() {
        let v: Vec<String> = numbered_names("Lesson.cmrec").take(3).collect();
        assert_eq!(v, ["Lesson.cmrec", "Lesson (2).cmrec", "Lesson (3).cmrec"]);
        let v: Vec<String> = numbered_names("a/b").take(2).collect();
        assert_eq!(v, ["a_b", "a_b (2)"]);
        assert_eq!(numbered_names("x").count(), 9_999);
    }

    #[test]
    fn empty_title_gets_a_name() {
        assert_eq!(expand_pattern("{title}", D, "  "), "untitled");
    }
}
