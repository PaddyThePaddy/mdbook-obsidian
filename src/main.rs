use std::io;
use std::process;

use mdbook_preprocessor::book::{Book, BookItem};
use mdbook_preprocessor::errors::Result;
use mdbook_preprocessor::{parse_input, Preprocessor, PreprocessorContext};
use regex::Regex;

struct ObsidianPreprocessor;

impl Preprocessor for ObsidianPreprocessor {
    fn name(&self) -> &str {
        "obsidian"
    }

    fn run(&self, _ctx: &PreprocessorContext, mut book: Book) -> Result<Book> {
        let re = Regex::new(r"\[([^\]]*)\]\(([^)]+)\)").expect("valid regex");
        book.for_each_mut(|item| {
            if let BookItem::Chapter(chapter) = item {
                chapter.content = process_content(&re, &chapter.content);
            }
        });
        Ok(book)
    }

    fn supports_renderer(&self, _renderer: &str) -> Result<bool> {
        Ok(true)
    }
}

/// Process chapter content, transforming internal markdown links while leaving
/// fenced code blocks and inline code spans untouched.
fn process_content(re: &Regex, content: &str) -> String {
    let trailing_newline = content.ends_with('\n');
    let mut result = String::with_capacity(content.len());
    let mut in_code_block = false;
    let mut fence: Option<(char, usize)> = None;

    for (i, line) in content.split('\n').enumerate() {
        if i > 0 {
            result.push('\n');
        }

        let trimmed = line.trim_start();
        let (is_fence, fc, flen) = detect_fence(trimmed);

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
            result.push_str(&transform_line(re, line));
        }
    }

    if trailing_newline && !result.ends_with('\n') {
        result.push('\n');
    }
    result
}

fn detect_fence(trimmed: &str) -> (bool, char, usize) {
    for &fc in &['`', '~'] {
        if trimmed.starts_with(fc) {
            let n = trimmed.chars().take_while(|&c| c == fc).count();
            if n >= 3 {
                return (true, fc, n);
            }
        }
    }
    (false, '`', 0)
}

/// Transform links in a single line, skipping inline code spans.
fn transform_line(re: &Regex, line: &str) -> String {
    let mut result = String::new();
    let mut remaining = line;

    while !remaining.is_empty() {
        match remaining.find('`') {
            None => {
                result.push_str(&replace_links(re, remaining));
                break;
            }
            Some(pos) => {
                result.push_str(&replace_links(re, &remaining[..pos]));
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
                        // Unclosed backtick run — treat rest as regular text
                        result.push_str(&replace_links(re, remaining));
                        break;
                    }
                }
            }
        }
    }

    result
}

fn replace_links(re: &Regex, text: &str) -> String {
    re.replace_all(text, |caps: &regex::Captures| {
        let link_text = &caps[1];
        let url = &caps[2];
        format!("[{}]({})", link_text, normalize_link(url))
    })
    .into_owned()
}

/// Normalize an Obsidian internal link URL to the mdBook convention:
/// URL-decode → lowercase → spaces to hyphens.
/// External URLs and same-page anchors are returned unchanged.
fn normalize_link(url: &str) -> String {
    if url.starts_with("http://")
        || url.starts_with("https://")
        || url.starts_with("//")
        || url.starts_with("mailto:")
        || url.starts_with('#')
    {
        return url.to_owned();
    }

    let (path, anchor) = match url.find('#') {
        Some(i) => (&url[..i], &url[i..]),
        None => (url, ""),
    };

    let decoded = urlencoding::decode(path).unwrap_or(std::borrow::Cow::Borrowed(path));
    let normalized = decoded.to_lowercase().replace(' ', "-");
    format!("{}{}", normalized, anchor)
}

fn main() {
    eprintln!("[mdbook-obsidian] invoked with args: {:?}", std::env::args().collect::<Vec<_>>());
    let preprocessor = ObsidianPreprocessor;
    let args: Vec<String> = std::env::args().collect();

    if args.len() > 1 && args[1] == "supports" {
        let renderer = args.get(2).map(String::as_str).unwrap_or("");
        match preprocessor.supports_renderer(renderer) {
            Ok(true) => process::exit(0),
            _ => process::exit(1),
        }
    }

    let (ctx, book) =
        parse_input(io::stdin()).expect("failed to parse mdbook preprocessor input");
    let result = preprocessor.run(&ctx, book).expect("preprocessor failed");
    serde_json::to_writer(io::stdout(), &result).expect("failed to write output");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn re() -> Regex {
        Regex::new(r"\[([^\]]*)\]\(([^)]+)\)").unwrap()
    }

    #[test]
    fn normalizes_percent_encoded_spaces() {
        let input = "[My Note](A%20file-name%20with%20space.md)";
        let got = replace_links(&re(), input);
        assert_eq!(got, "[My Note](a-file-name-with-space.md)");
    }

    #[test]
    fn lowercases_plain_path() {
        let input = "[Link](MyNote.md)";
        let got = replace_links(&re(), input);
        assert_eq!(got, "[Link](mynote.md)");
    }

    #[test]
    fn preserves_anchor() {
        let input = "[Link](My%20Note.md#section-title)";
        let got = replace_links(&re(), input);
        assert_eq!(got, "[Link](my-note.md#section-title)");
    }

    #[test]
    fn skips_external_urls() {
        let input = "[Rust](https://www.rust-lang.org)";
        let got = replace_links(&re(), input);
        assert_eq!(got, "[Rust](https://www.rust-lang.org)");
    }

    #[test]
    fn skips_same_page_anchors() {
        let input = "[top](#top)";
        let got = replace_links(&re(), input);
        assert_eq!(got, "[top](#top)");
    }

    #[test]
    fn skips_fenced_code_blocks() {
        let input = "```\n[Link](Not%20Transformed.md)\n```";
        let got = process_content(&re(), input);
        assert_eq!(got, input);
    }

    #[test]
    fn skips_inline_code() {
        let input = "text `[Link](Not%20Transformed.md)` end";
        let got = transform_line(&re(), input);
        assert_eq!(got, "text `[Link](Not%20Transformed.md)` end");
    }

    #[test]
    fn transforms_outside_inline_code() {
        let input = "[A](B%20C.md) and `code` and [D](E%20F.md)";
        let got = transform_line(&re(), input);
        assert_eq!(got, "[A](b-c.md) and `code` and [D](e-f.md)");
    }
}
