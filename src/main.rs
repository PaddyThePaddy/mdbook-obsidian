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

    fn run(&self, ctx: &PreprocessorContext, mut book: Book) -> Result<Book> {
        let verbose = ctx
            .config
            .get::<bool>("preprocessor.obsidian.verbose")
            .unwrap_or(None)
            .unwrap_or(false);

        // Capture optional leading `!` so we can skip image links.
        let re = Regex::new(r"(!?)\[([^\]]*)\]\(([^)]+)\)").expect("valid regex");
        book.for_each_mut(|item| {
            if let BookItem::Chapter(chapter) = item {
                chapter.content = process_content(&re, &chapter.content, verbose);
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
fn process_content(re: &Regex, content: &str, verbose: bool) -> String {
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
            result.push_str(&transform_line(re, line, verbose));
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
                        // Unclosed backtick run — treat rest as regular text
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
        let bang = &caps[1]; // "!" for images, "" for links
        let link_text = &caps[2];
        let url = &caps[3];

        // Never touch image links — the file on disk keeps its original name.
        if bang == "!" {
            return format!("![{}]({})", link_text, url);
        }

        let new_url = normalize_link(url);
        if verbose && new_url != url.as_ref() {
            eprintln!("[mdbook-obsidian] link: {url}  =>  {new_url}");
        }
        format!("[{}]({})", link_text, new_url)
    })
    .into_owned()
}

/// Normalize an Obsidian internal link URL for mdBook:
/// - External URLs: unchanged.
/// - Same-page anchors (`#Heading Name`): normalize only the fragment.
/// - Cross-page links (`page.md` or `page.md#Heading`): normalize only the
///   `#fragment` part; leave the file path as-is so existing file names are
///   not broken.
fn normalize_link(url: &str) -> String {
    if url.starts_with("http://")
        || url.starts_with("https://")
        || url.starts_with("//")
        || url.starts_with("mailto:")
    {
        return url.to_owned();
    }

    // Same-page anchor only — normalize the fragment.
    if url.starts_with('#') {
        return format!("#{}", normalize_fragment(&url[1..]));
    }

    // Cross-page link: keep the file path, only normalize the anchor.
    match url.find('#') {
        None => url.to_owned(),
        Some(i) => format!("{}#{}", &url[..i], normalize_fragment(&url[i + 1..])),
    }
}

fn normalize_fragment(fragment: &str) -> String {
    let decoded = urlencoding::decode(fragment).unwrap_or(std::borrow::Cow::Borrowed(fragment));
    decoded.to_lowercase().replace(' ', "-")
}

fn main() {
    eprintln!(
        "[mdbook-obsidian] invoked with args: {:?}",
        std::env::args().collect::<Vec<_>>()
    );
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
        // Must match the regex in `run`.
        Regex::new(r"(!?)\[([^\]]*)\]\(([^)]+)\)").unwrap()
    }

    // --- image links: never transformed ---

    #[test]
    fn skips_image_links_with_encoded_path() {
        let input = "![alt](Pasted%20Image%2020250424.png)";
        let got = replace_links(&re(), input, false);
        assert_eq!(got, input);
    }

    #[test]
    fn skips_image_links_with_subdir() {
        let input = "![screenshot](attachments/Pasted%20Image.png)";
        let got = replace_links(&re(), input, false);
        assert_eq!(got, input);
    }

    // --- same-page anchors ---

    #[test]
    fn normalizes_same_page_anchor_with_encoding() {
        let input = "[Grade 1](#Grade%201%20-%20Color)";
        let got = replace_links(&re(), input, false);
        assert_eq!(got, "[Grade 1](#grade-1---color)");
    }

    #[test]
    fn already_normalized_same_page_anchor_unchanged() {
        let input = "[top](#top)";
        let got = replace_links(&re(), input, false);
        assert_eq!(got, "[top](#top)");
    }

    // --- cross-page links: file path untouched, anchor normalized ---

    #[test]
    fn file_path_left_unchanged() {
        // The file on disk keeps its original name; only the anchor matters.
        let input = "[Link](A%20file-name%20with%20space.md)";
        let got = replace_links(&re(), input, false);
        assert_eq!(got, input);
    }

    #[test]
    fn normalizes_anchor_in_cross_page_link() {
        let input = "[Link](My%20Note.md#Grade%201%20-%20Color)";
        let got = replace_links(&re(), input, false);
        assert_eq!(got, "[Link](My%20Note.md#grade-1---color)");
    }

    #[test]
    fn skips_external_urls() {
        let input = "[Rust](https://www.rust-lang.org)";
        let got = replace_links(&re(), input, false);
        assert_eq!(got, "[Rust](https://www.rust-lang.org)");
    }

    // --- code block / inline code skipping ---

    #[test]
    fn skips_fenced_code_blocks() {
        let input = "```\n[Link](#Grade%201)\n```";
        let got = process_content(&re(), input, false);
        assert_eq!(got, input);
    }

    #[test]
    fn skips_inline_code() {
        let input = "text `[Link](#Grade%201)` end";
        let got = transform_line(&re(), input, false);
        assert_eq!(got, "text `[Link](#Grade%201)` end");
    }

    #[test]
    fn transforms_outside_inline_code() {
        let input = "[A](#Grade%201) and `code` and [B](#Math%20Class)";
        let got = transform_line(&re(), input, false);
        assert_eq!(got, "[A](#grade-1) and `code` and [B](#math-class)");
    }
}
