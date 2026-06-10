use regex::Regex;

fn url_re() -> &'static Regex {
    use std::sync::OnceLock;
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"https?://[^\s\[\]<>"']+"#).unwrap())
}

pub(crate) fn process_content(re: &Regex, content: &str, verbose: bool) -> String {
    let url_re = url_re();
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
            result.push_str(&transform_line(re, &url_re, line, verbose));
        }
    }

    if trailing_newline && !result.ends_with('\n') {
        result.push('\n');
    }
    result
}

fn transform_line(link_re: &Regex, url_re: &Regex, line: &str, verbose: bool) -> String {
    let mut result = String::new();
    let mut remaining = line;

    while !remaining.is_empty() {
        match remaining.find('`') {
            None => {
                result.push_str(&process_text(link_re, url_re, remaining, verbose));
                break;
            }
            Some(pos) => {
                result.push_str(&process_text(link_re, url_re, &remaining[..pos], verbose));
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
                        result.push_str(&process_text(link_re, url_re, remaining, verbose));
                        break;
                    }
                }
            }
        }
    }

    result
}

/// Normalize existing markdown links and auto-link bare URLs in a plain-text
/// segment (one that contains no inline code spans).
fn process_text(link_re: &Regex, url_re: &Regex, text: &str, verbose: bool) -> String {
    let mut result = String::new();
    let mut last_end = 0;

    for caps in link_re.captures_iter(text) {
        let mat = caps.get(0).unwrap();

        // Auto-link bare URLs in the gap before this markdown link.
        result.push_str(&autolink_gap(url_re, &text[last_end..mat.start()]));

        // Normalize the markdown link itself.
        let bang = caps.get(1).map_or("", |m| m.as_str());
        let link_text = caps.get(2).map_or("", |m| m.as_str());
        let url = caps.get(3).map_or("", |m| m.as_str());

        if bang == "!" {
            result.push_str(mat.as_str());
        } else {
            let new_url = normalize_link(url);
            if verbose && new_url != url {
                eprintln!(" INFO [mdbook-obsidian]: link: {url}  =>  {new_url}");
            }
            result.push_str(&format!("[{link_text}]({new_url})"));
        }

        last_end = mat.end();
    }

    // Auto-link bare URLs in the trailing gap after the last markdown link.
    result.push_str(&autolink_gap(url_re, &text[last_end..]));
    result
}

fn autolink_gap(url_re: &Regex, text: &str) -> String {
    url_re
        .replace_all(text, |caps: &regex::Captures| {
            let raw = caps.get(0).unwrap().as_str();
            let url = strip_url_trailing_punct(raw);
            let trailing = &raw[url.len()..];
            format!("[{url}]({url}){trailing}")
        })
        .into_owned()
}

fn strip_url_trailing_punct(url: &str) -> &str {
    url.trim_end_matches(|c: char| ".,;:!?\"'".contains(c))
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

    fn url_re() -> Regex {
        Regex::new(r#"https?://[^\s\[\]<>"']+"#).unwrap()
    }

    // Thin wrapper so existing tests keep their original call shape.
    // Auto-linking has no effect on inputs that contain no bare HTTP URLs.
    fn replace_links(re: &Regex, text: &str, verbose: bool) -> String {
        process_text(re, &url_re(), text, verbose)
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
        assert_eq!(transform_line(&link_re(), &url_re(), input, false), input);
    }

    #[test]
    fn transforms_outside_inline_code() {
        let input = "[A](#Grade%201) and `code` and [B](#Math%20Class)";
        assert_eq!(
            transform_line(&link_re(), &url_re(), input, false),
            "[A](#grade-1) and `code` and [B](#math-class)"
        );
    }

    // --- auto-link bare URLs ---

    #[test]
    fn autolinks_bare_https_url() {
        let input = "Watch https://youtu.be/abc here.";
        assert_eq!(
            process_text(&link_re(), &url_re(), input, false),
            "Watch [https://youtu.be/abc](https://youtu.be/abc) here."
        );
    }

    #[test]
    fn autolinks_strips_trailing_period() {
        let input = "See https://example.com.";
        assert_eq!(
            process_text(&link_re(), &url_re(), input, false),
            "See [https://example.com](https://example.com)."
        );
    }

    #[test]
    fn autolink_does_not_double_link_existing_link() {
        let input = "[text](https://youtu.be/abc)";
        assert_eq!(
            process_text(&link_re(), &url_re(), input, false),
            "[text](https://youtu.be/abc)"
        );
    }

    #[test]
    fn autolink_skips_url_in_link_text() {
        // URL appears in link text portion — should not be separately auto-linked
        // because the whole `[...](...)` is matched as a single link.
        let input = "[https://youtu.be/abc](https://youtu.be/abc)";
        assert_eq!(
            process_text(&link_re(), &url_re(), input, false),
            "[https://youtu.be/abc](https://youtu.be/abc)"
        );
    }

    #[test]
    fn autolinks_bare_url_in_code_block_skipped() {
        let input = "```\nhttps://example.com\n```";
        assert_eq!(process_content(&link_re(), input, false), input);
    }
}
