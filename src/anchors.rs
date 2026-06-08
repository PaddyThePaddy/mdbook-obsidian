use regex::Regex;

pub(crate) fn process_content(re: &Regex, content: &str, verbose: bool) -> String {
    let trailing_newline = content.ends_with('\n');
    let mut result = String::with_capacity(content.len());
    let mut in_code_block = false;
    let mut fence: Option<(char, usize)> = None;

    for (i, line) in content.split('\n').enumerate() {
        if i > 0 {
            result.push('\n');
        }
        let trimmed = line.trim_start();
        let (is_fence, fc, flen) = crate::detect_fence(trimmed);

        if is_fence {
            if !in_code_block {
                in_code_block = true;
                fence = Some((fc, flen));
                result.push_str(line);
            } else if let Some((f, c)) = fence {
                if fc == f && flen >= c {
                    in_code_block = false;
                    fence = None;
                }
                result.push_str(line);
            } else {
                result.push_str(line);
            }
        } else if in_code_block {
            result.push_str(line);
        } else {
            result.push_str(&transform_line(re, line, verbose));
        }
    }

    if trailing_newline && !result.ends_with('\n') {
        result.push('\n');
    }
    result
}

fn transform_line(re: &Regex, line: &str, verbose: bool) -> String {
    let mut result = String::new();
    let mut remaining = line;

    while !remaining.is_empty() {
        match remaining.find('`') {
            None => {
                result.push_str(&replace_links(re, remaining, verbose));
                break;
            }
            Some(pos) => {
                result.push_str(&replace_links(re, &remaining[..pos], verbose));
                remaining = &remaining[pos..];

                let tick_count = remaining.chars().take_while(|&c| c == '`').count();
                let after_open = &remaining[tick_count..];
                let closing = "`".repeat(tick_count);

                match after_open.find(closing.as_str()) {
                    Some(end) => {
                        let span_end = tick_count + end + tick_count;
                        result.push_str(&remaining[..span_end]);
                        remaining = &remaining[span_end..];
                    }
                    None => {
                        result.push_str(&replace_links(re, remaining, verbose));
                        break;
                    }
                }
            }
        }
    }

    result
}

fn replace_links(re: &Regex, text: &str, verbose: bool) -> String {
    re.replace_all(text, |caps: &regex::Captures| {
        let bang = &caps[1];
        let link_text = &caps[2];
        let url = &caps[3];

        if bang == "!" {
            return format!("![{}]({})", link_text, url);
        }

        let new_url = normalize_link(url);
        if verbose && new_url != url[..] {
            eprintln!(" INFO [mdbook-obsidian]: link: {url}  =>  {new_url}");
        }
        format!("[{}]({})", link_text, new_url)
    })
    .into_owned()
}

fn normalize_link(url: &str) -> String {
    if url.starts_with("http://")
        || url.starts_with("https://")
        || url.starts_with("//")
        || url.starts_with("mailto:")
    {
        return url.to_owned();
    }

    if url.starts_with('#') {
        return format!("#{}", normalize_fragment(&url[1..]));
    }

    match url.find('#') {
        None => url.to_owned(),
        Some(i) => format!("{}#{}", &url[..i], normalize_fragment(&url[i + 1..])),
    }
}

fn normalize_fragment(fragment: &str) -> String {
    let decoded = urlencoding::decode(fragment).unwrap_or(std::borrow::Cow::Borrowed(fragment));
    // Strip leading `^` — Obsidian block ID prefix; anchors are generated without it.
    let s = decoded.trim_start_matches('^');
    s.to_lowercase().replace(' ', "-")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link_re() -> Regex {
        Regex::new(r"(!?)\[([^\]]*)\]\(([^)]+)\)").unwrap()
    }

    // --- image links: never transformed ---

    #[test]
    fn skips_image_links_with_encoded_path() {
        let input = "![alt](Pasted%20Image%2020250424.png)";
        assert_eq!(replace_links(&link_re(), input, false), input);
    }

    #[test]
    fn skips_image_links_with_subdir() {
        let input = "![screenshot](attachments/Pasted%20Image.png)";
        assert_eq!(replace_links(&link_re(), input, false), input);
    }

    // --- same-page anchor normalization ---

    #[test]
    fn normalizes_same_page_anchor_with_encoding() {
        let input = "[Grade 1](#Grade%201%20-%20Color)";
        assert_eq!(
            replace_links(&link_re(), input, false),
            "[Grade 1](#grade-1---color)"
        );
    }

    #[test]
    fn already_normalized_same_page_anchor_unchanged() {
        assert_eq!(
            replace_links(&link_re(), "[top](#top)", false),
            "[top](#top)"
        );
    }

    #[test]
    fn strips_block_id_caret_from_fragment() {
        assert_eq!(
            replace_links(&link_re(), "[ref](Note.md#^my-block)", false),
            "[ref](Note.md#my-block)"
        );
    }

    #[test]
    fn strips_block_id_caret_from_same_page_fragment() {
        assert_eq!(
            replace_links(&link_re(), "[ref](#^my-block)", false),
            "[ref](#my-block)"
        );
    }

    #[test]
    fn file_path_left_unchanged() {
        let input = "[Link](A%20file-name%20with%20space.md)";
        assert_eq!(replace_links(&link_re(), input, false), input);
    }

    #[test]
    fn normalizes_anchor_in_cross_page_link() {
        let input = "[Link](My%20Note.md#Grade%201%20-%20Color)";
        assert_eq!(
            replace_links(&link_re(), input, false),
            "[Link](My%20Note.md#grade-1---color)"
        );
    }

    #[test]
    fn skips_external_urls() {
        let input = "[Rust](https://www.rust-lang.org)";
        assert_eq!(replace_links(&link_re(), input, false), input);
    }

    // --- code block / inline code skipping ---

    #[test]
    fn skips_fenced_code_blocks() {
        let input = "```\n[Link](#Grade%201)\n```";
        assert_eq!(process_content(&link_re(), input, false), input);
    }

    #[test]
    fn skips_inline_code() {
        let input = "text `[Link](#Grade%201)` end";
        assert_eq!(transform_line(&link_re(), input, false), input);
    }

    #[test]
    fn transforms_outside_inline_code() {
        let input = "[A](#Grade%201) and `code` and [B](#Math%20Class)";
        assert_eq!(
            transform_line(&link_re(), input, false),
            "[A](#grade-1) and `code` and [B](#math-class)"
        );
    }
}
