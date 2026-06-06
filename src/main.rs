use std::collections::HashSet;
use std::io;
use std::path::{Path, PathBuf};
use std::process;

use mdbook_preprocessor::book::{Book, BookItem, Chapter};
use mdbook_preprocessor::errors::Result;
use mdbook_preprocessor::{parse_input, Preprocessor, PreprocessorContext};
use regex::Regex;

const EXCALIDRAW_TEMPLATE: &str = include_str!("assets/excalidraw-page.html");

// ---------------------------------------------------------------------------
// Preprocessor entry point
// ---------------------------------------------------------------------------

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

        // --- Pass 1: anchor normalization --------------------------------
        // Capture optional leading `!` so we can skip image links.
        let link_re = Regex::new(r"(!?)\[([^\]]*)\]\(([^)]+)\)").expect("valid regex");
        book.for_each_mut(|item| {
            if let BookItem::Chapter(chapter) = item {
                chapter.content = process_content(&link_re, &chapter.content, verbose);
            }
        });

        // --- Pass 2: Excalidraw link detection and rewrite ---------------
        // Matches [text](path.excalidraw) — image variant ![ ] handled the same way.
        let excalidraw_link_re =
            Regex::new(r"(!?)\[([^\]]*)\]\(([^)]*\.excalidraw)\)").expect("valid regex");
        // Matches [[path.excalidraw]] and ![[path.excalidraw]] and [[path.excalidraw|alias]]
        let excalidraw_wiki_re =
            Regex::new(r"(!?)\[\[([^\]]+\.excalidraw)(?:\|([^\]]*))?\]\]").expect("valid regex");

        let src_dir = ctx.root.join(&ctx.config.book.src);
        let mut all_refs: Vec<ExcalidrawRef> = Vec::new();

        book.for_each_mut(|item| {
            if let BookItem::Chapter(chapter) = item {
                let chapter_dir = chapter
                    .source_path
                    .as_ref()
                    .and_then(|p| p.parent())
                    .map(|p| src_dir.join(p))
                    .unwrap_or_else(|| src_dir.clone());
                let depth = chapter
                    .path
                    .as_ref()
                    .map(|p| p.components().count().saturating_sub(1))
                    .unwrap_or(0);

                chapter.content = process_excalidraw(
                    &excalidraw_link_re,
                    &excalidraw_wiki_re,
                    &chapter.content,
                    &chapter_dir,
                    depth,
                    verbose,
                    &mut all_refs,
                );
            }
        });

        // Inject one synthetic chapter per unique excalidraw slug.
        let mut seen: HashSet<String> = HashSet::new();
        for r in &all_refs {
            if !seen.insert(r.slug.clone()) {
                continue;
            }
            match std::fs::read_to_string(&r.file_path) {
                Ok(json) => book.items.push(BookItem::Chapter(make_excalidraw_chapter(r, &json))),
                Err(e) => {
                    if verbose {
                        eprintln!(
                            "[mdbook-obsidian] cannot read {}: {e}",
                            r.file_path.display()
                        );
                    }
                }
            }
        }

        Ok(book)
    }

    fn supports_renderer(&self, _renderer: &str) -> Result<bool> {
        Ok(true)
    }
}

// ---------------------------------------------------------------------------
// Excalidraw reference
// ---------------------------------------------------------------------------

struct ExcalidrawRef {
    /// Absolute path to the .excalidraw file on disk.
    file_path: PathBuf,
    /// URL-safe slug used for `_excalidraw/{slug}.html`.
    slug: String,
    /// Human-readable display name (file stem, original casing).
    name: String,
}

fn excalidraw_slug(decoded_stem: &str) -> String {
    let slug: String = decoded_stem
        .to_lowercase()
        .replace(' ', "-")
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-')
        .collect();
    slug.trim_matches('-').to_string()
}

/// Relative URL from a chapter at `depth` directory levels to the synthetic
/// excalidraw page for `slug`.
fn excalidraw_href(slug: &str, depth: usize) -> String {
    format!("{}_excalidraw/{}.html", "../".repeat(depth), slug)
}

/// Build a synthetic chapter that renders the Excalidraw scene.
fn make_excalidraw_chapter(r: &ExcalidrawRef, json: &str) -> Chapter {
    // Prevent </script> inside JSON from closing the surrounding script tag.
    let safe_json = json.replace("</", "<\\/");
    let html = EXCALIDRAW_TEMPLATE
        .replace("{{DRAWING_NAME}}", &r.name)
        .replace("{{SCENE_JSON}}", &safe_json);

    Chapter {
        name: r.name.clone(),
        content: html,
        number: None,
        sub_items: vec![],
        path: Some(PathBuf::from(format!("_excalidraw/{}.md", r.slug))),
        source_path: None,
        parent_names: vec![],
    }
}

// ---------------------------------------------------------------------------
// Excalidraw pass (pass 2)
// ---------------------------------------------------------------------------

fn process_excalidraw(
    link_re: &Regex,
    wiki_re: &Regex,
    content: &str,
    chapter_dir: &Path,
    depth: usize,
    verbose: bool,
    refs: &mut Vec<ExcalidrawRef>,
) -> String {
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
            // Wikilinks first, then regular markdown links.
            let line = replace_excalidraw_wikilinks(wiki_re, line, chapter_dir, depth, verbose, refs);
            let line = replace_excalidraw_links(link_re, &line, chapter_dir, depth, verbose, refs);
            result.push_str(&line);
        }
    }

    if trailing_newline && !result.ends_with('\n') {
        result.push('\n');
    }
    result
}

/// Replace `![[file.excalidraw]]` and `[[file.excalidraw|alias]]` wikilinks.
fn replace_excalidraw_wikilinks(
    wiki_re: &Regex,
    text: &str,
    chapter_dir: &Path,
    depth: usize,
    verbose: bool,
    refs: &mut Vec<ExcalidrawRef>,
) -> String {
    wiki_re
        .replace_all(text, |caps: &regex::Captures| {
            let raw_path = &caps[2]; // e.g. "My Drawing.excalidraw"
            let alias = caps.get(3).map(|m| m.as_str()); // optional |alias text

            let decoded = urlencoding::decode(raw_path)
                .unwrap_or(std::borrow::Cow::Borrowed(raw_path));
            let path = Path::new(decoded.as_ref());
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("drawing");
            let slug = excalidraw_slug(stem);
            let display = alias.unwrap_or(stem).to_string();
            let href = excalidraw_href(&slug, depth);

            let file_path = chapter_dir.join(decoded.as_ref());
            if verbose {
                eprintln!("[mdbook-obsidian] excalidraw wikilink: {raw_path}  =>  {href}");
            }
            refs.push(ExcalidrawRef { file_path, slug, name: stem.to_string() });
            format!("[{}]({})", display, href)
        })
        .into_owned()
}

/// Replace `[text](file.excalidraw)` markdown links.
fn replace_excalidraw_links(
    link_re: &Regex,
    text: &str,
    chapter_dir: &Path,
    depth: usize,
    verbose: bool,
    refs: &mut Vec<ExcalidrawRef>,
) -> String {
    link_re
        .replace_all(text, |caps: &regex::Captures| {
            let bang = &caps[1];
            let link_text = &caps[2];
            let raw_path = &caps[3];

            let decoded = urlencoding::decode(raw_path)
                .unwrap_or(std::borrow::Cow::Borrowed(raw_path));
            let path = Path::new(decoded.as_ref());
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("drawing");
            let slug = excalidraw_slug(stem);
            let href = excalidraw_href(&slug, depth);

            let file_path = chapter_dir.join(decoded.as_ref());
            if verbose {
                eprintln!("[mdbook-obsidian] excalidraw link: {raw_path}  =>  {href}");
            }
            refs.push(ExcalidrawRef { file_path, slug, name: stem.to_string() });

            // Preserve the ! prefix so it renders as a link, not a broken image.
            let _ = bang; // image-style embed treated identically — becomes a link
            format!("[{}]({})", link_text, href)
        })
        .into_owned()
}

// ---------------------------------------------------------------------------
// Anchor-normalisation pass (pass 1) — unchanged from before
// ---------------------------------------------------------------------------

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
        if verbose && new_url != url.as_ref() {
            eprintln!("[mdbook-obsidian] link: {url}  =>  {new_url}");
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
    decoded.to_lowercase().replace(' ', "-")
}

// ---------------------------------------------------------------------------
// Binary entry point
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

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

    // --- excalidraw slug ---

    #[test]
    fn slug_lowercases_and_hyphenates() {
        assert_eq!(excalidraw_slug("My Drawing"), "my-drawing");
    }

    #[test]
    fn slug_strips_non_alphanumeric() {
        assert_eq!(excalidraw_slug("plan (v2)"), "plan-v2");
    }

    // --- excalidraw wikilink replacement ---

    fn wiki_re() -> Regex {
        Regex::new(r"(!?)\[\[([^\]]+\.excalidraw)(?:\|([^\]]*))?\]\]").unwrap()
    }

    fn excalidraw_link_re() -> Regex {
        Regex::new(r"(!?)\[([^\]]*)\]\(([^)]*\.excalidraw)\)").unwrap()
    }

    #[test]
    fn replaces_excalidraw_wikilink() {
        let mut refs = vec![];
        let got = replace_excalidraw_wikilinks(
            &wiki_re(),
            "![[My Drawing.excalidraw]]",
            Path::new("/src"),
            0,
            false,
            &mut refs,
        );
        assert_eq!(got, "[My Drawing](_excalidraw/my-drawing.html)");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].slug, "my-drawing");
    }

    #[test]
    fn respects_wikilink_alias() {
        let mut refs = vec![];
        let got = replace_excalidraw_wikilinks(
            &wiki_re(),
            "[[plan.excalidraw|See the plan]]",
            Path::new("/src"),
            0,
            false,
            &mut refs,
        );
        assert_eq!(got, "[See the plan](_excalidraw/plan.html)");
    }

    #[test]
    fn adjusts_href_depth() {
        let mut refs = vec![];
        let got = replace_excalidraw_wikilinks(
            &wiki_re(),
            "![[draw.excalidraw]]",
            Path::new("/src/a/b"),
            2,
            false,
            &mut refs,
        );
        assert_eq!(got, "[draw](../../_excalidraw/draw.html)");
    }

    #[test]
    fn replaces_excalidraw_markdown_link() {
        let mut refs = vec![];
        let got = replace_excalidraw_links(
            &excalidraw_link_re(),
            "[diagram](my-diagram.excalidraw)",
            Path::new("/src"),
            0,
            false,
            &mut refs,
        );
        assert_eq!(got, "[diagram](_excalidraw/my-diagram.html)");
        assert_eq!(refs[0].slug, "my-diagram");
    }

    #[test]
    fn json_escaping_prevents_script_injection() {
        let r = ExcalidrawRef {
            file_path: PathBuf::from("/fake.excalidraw"),
            slug: "test".into(),
            name: "test".into(),
        };
        let json = r#"{"text":"</script><script>alert(1)</script>"}"#;
        let chapter = make_excalidraw_chapter(&r, json);
        assert!(!chapter.content.contains("</script><script>"));
    }
}
