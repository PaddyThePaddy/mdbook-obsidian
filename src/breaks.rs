/// Convert Obsidian-style single line breaks to CommonMark hard line breaks.
///
/// A single newline between two non-empty lines gets two trailing spaces appended
/// to the first line, which CommonMark treats as a hard break (`<br>`).
/// Lines already ending with `  ` or `\` are left untouched.
/// Content inside fenced code blocks is skipped.
pub(crate) fn patch_hard_line_breaks(content: &str) -> String {
    let mut result = String::with_capacity(content.len() + 64);
    let mut in_code_block = false;
    let mut fence: Option<(char, usize)> = None;
    let lines: Vec<&str> = content.split('\n').collect();

    for (i, &line) in lines.iter().enumerate() {
        if i > 0 {
            result.push('\n');
        }

        let trimmed = line.trim_start();
        let (is_fence, fc, flen) = crate::detect_fence(trimmed);

        if is_fence {
            if !in_code_block {
                in_code_block = true;
                fence = Some((fc, flen));
            } else if let Some((f, c)) = fence {
                if fc == f && flen >= c {
                    in_code_block = false;
                    fence = None;
                }
            }
            result.push_str(line);
            continue;
        }

        if in_code_block {
            result.push_str(line);
            continue;
        }

        let needs_break = !line.trim().is_empty()
            // next line is adjacent and non-empty (not a paragraph break)
            && lines.get(i + 1).map(|l| !l.trim().is_empty()).unwrap_or(false)
            && !line.ends_with("  ")
            && !line.ends_with('\\');

        result.push_str(line);
        if needs_break {
            result.push_str("  ");
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hard_break_added_between_adjacent_lines() {
        let input = "line one\nline two\n";
        assert_eq!(patch_hard_line_breaks(input), "line one  \nline two\n");
    }

    #[test]
    fn hard_break_not_added_across_blank_line() {
        let input = "paragraph one\n\nparagraph two\n";
        assert_eq!(patch_hard_line_breaks(input), input);
    }

    #[test]
    fn hard_break_skips_existing_trailing_spaces() {
        let input = "already hard  \nnext\n";
        assert_eq!(patch_hard_line_breaks(input), input);
    }

    #[test]
    fn hard_break_skips_code_block_contents() {
        let input = "text\n```\nline a\nline b\n```\nafter\n";
        assert_eq!(
            patch_hard_line_breaks(input),
            "text  \n```\nline a\nline b\n```\nafter\n"
        );
    }
}
