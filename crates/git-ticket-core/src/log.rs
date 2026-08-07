pub fn parse_lines(content: &str) -> Vec<String> {
    content
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect()
}

fn join_lines(lines: Vec<String>) -> String {
    if lines.is_empty() {
        String::new()
    } else {
        lines.join("\n") + "\n"
    }
}

/// The ONLY sanctioned way to add a new event to a note's content.
/// Never replace or truncate existing content directly.
pub fn append_line(content: &str, line: &str) -> String {
    let mut lines = parse_lines(content);
    lines.push(line.to_string());
    join_lines(lines)
}

/// Equivalent to git's `cat_sort_uniq` notes-merge strategy, reimplemented
/// natively so sync doesn't depend on shelling out to the `git` binary.
pub fn merge_cat_sort_uniq(local: &str, remote: &str) -> String {
    let mut lines = parse_lines(local);
    lines.extend(parse_lines(remote));
    lines.sort();
    lines.dedup();
    join_lines(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_lines_splits_and_skips_blank_lines() {
        let content = "line1\nline2\n\nline3\n";
        assert_eq!(parse_lines(content), vec!["line1", "line2", "line3"]);
    }

    #[test]
    fn parse_lines_on_empty_content_is_empty() {
        assert!(parse_lines("").is_empty());
    }

    #[test]
    fn append_line_adds_to_empty_content() {
        assert_eq!(append_line("", "first"), "first\n");
    }

    #[test]
    fn append_line_adds_after_existing_lines() {
        assert_eq!(append_line("first\n", "second"), "first\nsecond\n");
    }

    #[test]
    fn append_line_never_drops_existing_lines() {
        let mut content = String::new();
        for i in 0..5 {
            content = append_line(&content, &format!("event-{i}"));
        }
        let lines = parse_lines(&content);
        assert_eq!(lines, vec!["event-0", "event-1", "event-2", "event-3", "event-4"]);
    }

    #[test]
    fn merge_cat_sort_uniq_unions_and_dedupes() {
        let local = "b\na\n";
        let remote = "c\na\n";
        let merged = merge_cat_sort_uniq(local, remote);
        assert_eq!(parse_lines(&merged), vec!["a", "b", "c"]);
    }

    #[test]
    fn merge_cat_sort_uniq_with_empty_remote_keeps_local() {
        let merged = merge_cat_sort_uniq("only\n", "");
        assert_eq!(parse_lines(&merged), vec!["only"]);
    }

    #[test]
    fn merge_cat_sort_uniq_is_commutative() {
        let a = "x\ny\n";
        let b = "y\nz\n";
        assert_eq!(merge_cat_sort_uniq(a, b), merge_cat_sort_uniq(b, a));
    }
}
