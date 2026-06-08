use regex::Regex;

const CALLOUT_CSS: &str = r"<style>
.callout{border-left:4px solid #888;border-radius:0 4px 4px 0;margin:1em 0;overflow:hidden}
.callout-title{font-weight:600;padding:.4em .8em;background:rgba(0,0,0,.07)}
.callout-content{padding:.4em .8em}.callout-content>:last-child{margin-bottom:0}
details.callout>summary{cursor:pointer;list-style:none;font-weight:600;padding:.4em .8em;background:rgba(0,0,0,.07)}
details.callout>summary::-webkit-details-marker{display:none}
.callout-note,.callout-info,.callout-todo{border-color:#4a9eff;background:rgba(74,158,255,.06)}
.callout-note .callout-title,.callout-info .callout-title,.callout-todo .callout-title,
details.callout-note>summary{background:rgba(74,158,255,.18)}
.callout-abstract{border-color:#00bcd4;background:rgba(0,188,212,.06)}
.callout-abstract .callout-title,details.callout-abstract>summary{background:rgba(0,188,212,.18)}
.callout-tip{border-color:#00c853;background:rgba(0,200,83,.06)}
.callout-tip .callout-title,details.callout-tip>summary{background:rgba(0,200,83,.18)}
.callout-success{border-color:#00c853;background:rgba(0,200,83,.06)}
.callout-success .callout-title,details.callout-success>summary{background:rgba(0,200,83,.18)}
.callout-question{border-color:#ffb300;background:rgba(255,179,0,.06)}
.callout-question .callout-title,details.callout-question>summary{background:rgba(255,179,0,.18)}
.callout-warning{border-color:#ff6d00;background:rgba(255,109,0,.06)}
.callout-warning .callout-title,details.callout-warning>summary{background:rgba(255,109,0,.18)}
.callout-failure{border-color:#f44336;background:rgba(244,67,54,.06)}
.callout-failure .callout-title,details.callout-failure>summary{background:rgba(244,67,54,.18)}
.callout-danger{border-color:#f44336;background:rgba(244,67,54,.06)}
.callout-danger .callout-title,details.callout-danger>summary{background:rgba(244,67,54,.18)}
.callout-bug{border-color:#d50000;background:rgba(213,0,0,.06)}
.callout-bug .callout-title,details.callout-bug>summary{background:rgba(213,0,0,.18)}
.callout-example{border-color:#7c4dff;background:rgba(124,77,255,.06)}
.callout-example .callout-title,details.callout-example>summary{background:rgba(124,77,255,.18)}
.callout-quote{border-color:#9e9e9e;background:rgba(158,158,158,.06)}
.callout-quote .callout-title,details.callout-quote>summary{background:rgba(158,158,158,.18)}
</style>";

/// Apply all Obsidian-flavored syntax conversions:
/// - `%%...%%` comments are removed
/// - `^block-id` markers become `<span id="block-id">` anchors
/// - `==text==` becomes `<mark>text</mark>`
/// - `> [!type]` callout blocks become styled HTML
/// - `[[wikilinks]]` become regular markdown links
pub(crate) fn process(content: &str, _verbose: bool) -> String {
    let s = remove_comments(content);
    let s = convert_block_ids(&s);
    let (s, had_callouts) = convert_callouts(&s);
    let s = convert_highlights(&s);
    let s = convert_wikilinks(&s);
    if had_callouts {
        format!("{CALLOUT_CSS}\n\n{s}")
    } else {
        s
    }
}

// ---------------------------------------------------------------------------
// Comments: %%...%% → removed (may span multiple lines)
// ---------------------------------------------------------------------------

fn remove_comments(content: &str) -> String {
    let lines: Vec<&str> = content.split('\n').collect();
    let n = lines.len();
    let mut result = String::with_capacity(content.len());
    let mut in_code_block = false;
    let mut fence: Option<(char, usize)> = None;
    let mut in_comment = false;

    for (li, &line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        let (is_fence, fc, flen) = crate::detect_fence(trimmed);

        if is_fence && !in_comment {
            if !in_code_block {
                in_code_block = true;
                fence = Some((fc, flen));
            } else if let Some((f, c)) = fence {
                if fc == f && flen >= c {
                    in_code_block = false;
                    fence = None;
                }
            }
        }

        if in_code_block {
            result.push_str(line);
        } else {
            result.push_str(&strip_line_comments(line, &mut in_comment));
        }
        if li < n - 1 {
            result.push('\n');
        }
    }
    result
}

fn strip_line_comments(line: &str, in_comment: &mut bool) -> String {
    let mut out = String::new();
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if i + 1 < chars.len() && chars[i] == '%' && chars[i + 1] == '%' {
            *in_comment = !*in_comment;
            i += 2;
        } else if !*in_comment {
            out.push(chars[i]);
            i += 1;
        } else {
            i += 1;
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Block IDs: `^id` at line end → `<span id="id"></span>` anchor
// ---------------------------------------------------------------------------

fn convert_block_ids(content: &str) -> String {
    let lines: Vec<&str> = content.split('\n').collect();
    let n = lines.len();
    let mut result = String::with_capacity(content.len());
    let mut in_code_block = false;
    let mut fence: Option<(char, usize)> = None;

    for (li, &line) in lines.iter().enumerate() {
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
        } else if in_code_block {
            result.push_str(line);
        } else {
            result.push_str(&convert_block_id_line(line));
        }
        if li < n - 1 {
            result.push('\n');
        }
    }
    result
}

fn convert_block_id_line(line: &str) -> String {
    let trimmed = line.trim();

    // Standalone block ID: line contains nothing but `^id` (possibly with whitespace).
    if let Some(rest) = trimmed.strip_prefix('^') {
        if !rest.is_empty()
            && rest.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        {
            return format!("<span id=\"{rest}\"></span>");
        }
    }

    // Inline block ID: ` ^id` at the very end of the line.
    if let Some(pos) = line.rfind(" ^") {
        let after = &line[pos + 2..];
        let id_len = after
            .find(|c: char| !c.is_ascii_alphanumeric() && c != '-')
            .unwrap_or(after.len());
        let id = &after[..id_len];
        let rest_trimmed = after[id_len..].trim();
        if !id.is_empty() && rest_trimmed.is_empty() {
            let before = &line[..pos];
            // Plain paragraph text: wrap the content so the ID sits on the element itself.
            // Headings/lists/blockquotes: append an empty span to avoid breaking block syntax.
            if is_plain_paragraph(before) {
                return format!("<span id=\"{id}\">{before}</span>");
            } else {
                return format!("{before} <span id=\"{id}\"></span>");
            }
        }
    }

    line.to_string()
}

/// Returns true when `s` is plain paragraph content (no markdown block-level prefix).
/// Headings (`# `), unordered lists (`- `/`* `/`+ `), ordered lists (`1. `),
/// and blockquotes (`> `) are non-plain — wrapping them in a `<span>` would
/// prevent pulldown-cmark from recognising the block-level syntax.
fn is_plain_paragraph(s: &str) -> bool {
    let t = s.trim_start();
    if t.starts_with('#') || t.starts_with('>') || t.starts_with(':') {
        return false;
    }
    if t.starts_with("- ") || t.starts_with("* ") || t.starts_with("+ ") {
        return false;
    }
    // Ordered list: one or more digits followed by `. ` or `) `
    let digits_end = t.find(|c: char| !c.is_ascii_digit()).unwrap_or(0);
    if digits_end > 0 {
        let after_digits = &t[digits_end..];
        if after_digits.starts_with(". ") || after_digits.starts_with(") ") {
            return false;
        }
    }
    true
}

// ---------------------------------------------------------------------------
// Highlights: ==text== → <mark>text</mark>
// ---------------------------------------------------------------------------

fn convert_highlights(content: &str) -> String {
    let lines: Vec<&str> = content.split('\n').collect();
    let n = lines.len();
    let mut result = String::with_capacity(content.len());
    let mut in_code_block = false;
    let mut fence: Option<(char, usize)> = None;

    for (li, &line) in lines.iter().enumerate() {
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
        } else if in_code_block {
            result.push_str(line);
        } else {
            result.push_str(&highlight_line(line));
        }
        if li < n - 1 {
            result.push('\n');
        }
    }
    result
}

/// Skip inline code spans, then apply the character-level highlight scanner to the rest.
fn highlight_line(line: &str) -> String {
    let mut out = String::new();
    let mut remaining = line;
    while !remaining.is_empty() {
        match remaining.find('`') {
            None => {
                out.push_str(&highlight_segment(remaining));
                break;
            }
            Some(pos) => {
                out.push_str(&highlight_segment(&remaining[..pos]));
                remaining = &remaining[pos..];
                let tick_count = remaining.chars().take_while(|&c| c == '`').count();
                let after_open = &remaining[tick_count..];
                let closing = "`".repeat(tick_count);
                match after_open.find(closing.as_str()) {
                    Some(end) => {
                        let span_end = tick_count + end + tick_count;
                        out.push_str(&remaining[..span_end]);
                        remaining = &remaining[span_end..];
                    }
                    None => {
                        out.push_str(&highlight_segment(remaining));
                        break;
                    }
                }
            }
        }
    }
    out
}

/// Character-level `==highlight==` scanner.
/// Requires that `==` is not immediately preceded or followed by another `=`.
fn highlight_segment(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    let mut out = String::new();
    let mut i = 0;

    while i < n {
        // Detect opening `==` that is not part of `===`.
        if chars[i] == '='
            && i + 1 < n
            && chars[i + 1] == '='
            && (i == 0 || chars[i - 1] != '=')
            && i + 2 < n
            && chars[i + 2] != '='
            && chars[i + 2] != '\n'
        {
            let content_start = i + 2;
            let mut found = false;
            let mut j = content_start;
            while j < n {
                if chars[j] == '\n' {
                    break;
                }
                // Detect closing `==` not part of `===`.
                if chars[j] == '='
                    && j + 1 < n
                    && chars[j + 1] == '='
                    && j > content_start
                    && chars[j - 1] != '='
                    && (j + 2 >= n || chars[j + 2] != '=')
                {
                    let content: String = chars[content_start..j].iter().collect();
                    out.push_str("<mark>");
                    out.push_str(&content);
                    out.push_str("</mark>");
                    i = j + 2;
                    found = true;
                    break;
                }
                j += 1;
            }
            if !found {
                out.push(chars[i]);
                i += 1;
            }
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Callouts: > [!type] Title → HTML block
// ---------------------------------------------------------------------------

fn convert_callouts(content: &str) -> (String, bool) {
    let lines: Vec<&str> = content.split('\n').collect();
    let n = lines.len();
    let mut result = String::new();
    let mut had_callouts = false;
    let mut in_code_block = false;
    let mut fence: Option<(char, usize)> = None;
    let mut i = 0;

    while i < n {
        let line = lines[i];
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
            if i < n - 1 {
                result.push('\n');
            }
            i += 1;
            continue;
        }

        if in_code_block {
            result.push_str(line);
            if i < n - 1 {
                result.push('\n');
            }
            i += 1;
            continue;
        }

        if let Some((callout_type, title, foldable)) = parse_callout_header(line) {
            had_callouts = true;
            i += 1;

            // Collect body lines, stripping the leading "> " prefix.
            let mut body: Vec<&str> = Vec::new();
            while i < n {
                let next = lines[i];
                if let Some(rest) = next.strip_prefix("> ") {
                    body.push(rest);
                    i += 1;
                } else if next == ">" {
                    body.push("");
                    i += 1;
                } else {
                    break;
                }
            }

            result.push_str(&render_callout(&callout_type, &title, foldable, &body));
        } else {
            result.push_str(line);
            if i < n - 1 {
                result.push('\n');
            }
            i += 1;
        }
    }

    (result, had_callouts)
}

fn normalize_callout_type(raw: &str) -> &str {
    match raw {
        "summary" | "tldr" => "abstract",
        "hint" | "important" => "tip",
        "check" | "done" => "success",
        "help" | "faq" => "question",
        "caution" | "attention" => "warning",
        "fail" | "missing" => "failure",
        "error" => "danger",
        "cite" => "quote",
        other => other,
    }
}

/// Parse `> [!type]`, `> [!type]+ Title`, `> [!type]- Title` etc.
/// Returns `(canonical_type, display_title, foldable)` on success.
fn parse_callout_header(line: &str) -> Option<(String, String, Option<bool>)> {
    let rest = line.trim_start().strip_prefix("> [!")?;
    let bracket_end = rest.find(']')?;
    let raw_type = rest[..bracket_end].trim().to_lowercase();
    let after = &rest[bracket_end + 1..];

    let (foldable, title_str) = if after.starts_with('+') {
        (Some(true), after[1..].trim())
    } else if after.starts_with('-') {
        (Some(false), after[1..].trim())
    } else {
        (None, after.trim())
    };

    let canonical = normalize_callout_type(&raw_type).to_string();
    let title = if title_str.is_empty() {
        let mut t = canonical.clone();
        if let Some(c) = t.get_mut(..1) {
            c.make_ascii_uppercase();
        }
        t
    } else {
        title_str.to_string()
    };

    Some((canonical, title, foldable))
}

fn render_callout(
    callout_type: &str,
    title: &str,
    foldable: Option<bool>,
    body: &[&str],
) -> String {
    let type_class = format!("callout-{callout_type}");
    let body_md = body.join("\n");

    // Use <details>/<summary> for foldable callouts — no JS required.
    // The blank lines around body_md cause pulldown-cmark to treat it as
    // markdown (not raw HTML), so bold/italic/etc. inside callouts still render.
    if let Some(expanded) = foldable {
        let open_attr = if expanded { " open" } else { "" };
        format!(
            "<details class=\"callout {type_class}\" data-callout=\"{callout_type}\"{open_attr}>\n\
            <summary class=\"callout-title\">{title}</summary>\n\
            <div class=\"callout-content\">\n\
            \n{body_md}\n\
            \n</div>\n\
            </details>\n\n"
        )
    } else {
        format!(
            "<div class=\"callout {type_class}\" data-callout=\"{callout_type}\">\n\
            <div class=\"callout-title\">{title}</div>\n\
            <div class=\"callout-content\">\n\
            \n{body_md}\n\
            \n</div>\n\
            </div>\n\n"
        )
    }
}

// ---------------------------------------------------------------------------
// Wikilinks: [[Note Name]] → [Note Name](Note%20Name.md)
//            [[Note Name|Display]] → [Display](Note%20Name.md)
//            ![[...]] embeds and [[*.excalidraw]] are left unchanged.
// ---------------------------------------------------------------------------

fn convert_wikilinks(content: &str) -> String {
    let re = Regex::new(r"(!?)\[\[([^\]]+?)\]\]").expect("valid regex");

    let lines: Vec<&str> = content.split('\n').collect();
    let n = lines.len();
    let mut result = String::with_capacity(content.len());
    let mut in_code_block = false;
    let mut fence: Option<(char, usize)> = None;

    for (li, &line) in lines.iter().enumerate() {
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
        } else if in_code_block {
            result.push_str(line);
        } else {
            result.push_str(&wikilink_line(&re, line));
        }
        if li < n - 1 {
            result.push('\n');
        }
    }
    result
}

fn wikilink_line(re: &Regex, line: &str) -> String {
    re.replace_all(line, |caps: &regex::Captures| {
        let bang = &caps[1];
        let inner = &caps[2];

        // Keep embeds (![[...]]) unchanged.
        if bang == "!" {
            return caps[0].to_string();
        }

        // Split on first `|` for display text.
        let (path_part, display_override) = if let Some(pipe) = inner.find('|') {
            (&inner[..pipe], Some(inner[pipe + 1..].trim()))
        } else {
            (inner.as_ref(), None)
        };

        // Skip excalidraw links — handled by the excalidraw pass.
        if path_part.ends_with(".excalidraw") || path_part.ends_with(".excalidraw.md") {
            return caps[0].to_string();
        }

        // Split on `#` for heading anchor.
        let (file_part, frag_raw) = if let Some(hash) = path_part.find('#') {
            (&path_part[..hash], Some(&path_part[hash + 1..]))
        } else {
            (path_part, None)
        };

        // Add .md if no extension present; empty file_part = same-page link.
        let file_with_ext = if file_part.is_empty() {
            String::new()
        } else if std::path::Path::new(file_part).extension().is_some() {
            file_part.to_string()
        } else {
            format!("{file_part}.md")
        };

        // Encode each path component separately (preserve `/`).
        let encoded: String = file_with_ext
            .split('/')
            .map(|c| urlencoding::encode(c).into_owned())
            .collect::<Vec<_>>()
            .join("/");

        // Block IDs (`^id`) are kept verbatim; headings are slugified.
        let fragment = frag_raw.map(|f| {
            if let Some(block_id) = f.strip_prefix('^') {
                block_id.to_string()
            } else {
                f.to_lowercase().replace(' ', "-")
            }
        });

        let url = if let Some(f) = &fragment {
            format!("{}#{}", encoded, f)
        } else {
            encoded
        };

        // Display text: explicit override, filename stem, or fragment for same-page links.
        let display: String = if let Some(d) = display_override {
            d.to_string()
        } else {
            let stem = std::path::Path::new(file_part)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(file_part)
                .to_string();
            if stem.is_empty() {
                // Same-page link: use the fragment as display text.
                frag_raw
                    .unwrap_or("")
                    .trim_start_matches('^')
                    .to_string()
            } else {
                stem
            }
        };

        format!("[{display}]({url})")
    })
    .into_owned()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- comments ---

    #[test]
    fn removes_inline_comment() {
        assert_eq!(
            process("before %%hidden%% after", false),
            "before  after"
        );
    }

    #[test]
    fn removes_multiline_comment() {
        let input = "start\n%%\nhidden\n%%\nend";
        assert_eq!(process(input, false), "start\n\n\n\nend");
    }

    #[test]
    fn preserves_comment_in_code_block() {
        let input = "```\n%%not removed%%\n```";
        assert_eq!(process(input, false), input);
    }

    // --- highlights ---

    #[test]
    fn converts_highlight() {
        assert_eq!(process("==hello==", false), "<mark>hello</mark>");
    }

    #[test]
    fn ignores_triple_equals() {
        let input = "===foo===";
        assert_eq!(process(input, false), input);
    }

    #[test]
    fn preserves_highlight_in_code_span() {
        let input = "`==not highlighted==`";
        assert_eq!(process(input, false), input);
    }

    // --- callouts ---

    #[test]
    fn converts_basic_callout() {
        let input = "> [!note]\n> Body text";
        let out = process(input, false);
        assert!(out.contains("callout-note"));
        assert!(out.contains("Note"));
        assert!(out.contains("Body text"));
    }

    #[test]
    fn callout_custom_title() {
        let input = "> [!tip] My Tip\n> Content";
        let out = process(input, false);
        assert!(out.contains("My Tip"));
        assert!(out.contains("callout-tip"));
    }

    #[test]
    fn foldable_callout_uses_details() {
        let input = "> [!faq]- Collapsed\n> Content";
        let out = process(input, false);
        assert!(out.contains("<details"));
        assert!(!out.contains("open"));
    }

    #[test]
    fn foldable_callout_expanded() {
        let input = "> [!faq]+ Expanded\n> Content";
        let out = process(input, false);
        assert!(out.contains("open"));
    }

    #[test]
    fn callout_injects_css() {
        let input = "> [!note]\n> Body";
        let out = process(input, false);
        assert!(out.contains("<style>"));
    }

    #[test]
    fn non_callout_blockquote_unchanged() {
        let input = "> This is a regular blockquote";
        let out = process(input, false);
        assert_eq!(out, input);
    }

    // --- block IDs ---

    #[test]
    fn block_id_inline_plain_paragraph() {
        assert_eq!(
            process("Some paragraph text ^my-block", false),
            "<span id=\"my-block\">Some paragraph text</span>"
        );
    }

    #[test]
    fn block_id_standalone_line() {
        assert_eq!(
            process("^standalone-id", false),
            "<span id=\"standalone-id\"></span>"
        );
    }

    #[test]
    fn block_id_in_fenced_code_unchanged() {
        let input = "```\ncode line ^not-an-id\n```";
        assert_eq!(process(input, false), input);
    }

    #[test]
    fn block_id_not_converted_mid_line() {
        // `^id` with more text after it → not a block ID
        let input = "text ^id more text";
        assert_eq!(process(input, false), input);
    }

    #[test]
    fn block_id_heading_appends_empty_span() {
        // Headings: keep the `##` outside so markdown syntax is preserved.
        assert_eq!(
            process("## Section ^sec-id", false),
            "## Section <span id=\"sec-id\"></span>"
        );
    }

    #[test]
    fn block_id_list_item_appends_empty_span() {
        assert_eq!(
            process("- Item text ^item-id", false),
            "- Item text <span id=\"item-id\"></span>"
        );
    }

    // --- wikilinks with block IDs ---

    #[test]
    fn wikilink_to_block_id() {
        let out = process("[[My Note#^block-id]]", false);
        assert_eq!(out, "[My Note](My%20Note.md#block-id)");
    }

    #[test]
    fn wikilink_same_page_block_id() {
        let out = process("[[#^block-id]]", false);
        assert_eq!(out, "[block-id](#block-id)");
    }

    #[test]
    fn wikilink_same_page_block_id_with_display() {
        let out = process("[[#^block-id|See this]]", false);
        assert_eq!(out, "[See this](#block-id)");
    }

    #[test]
    fn wikilink_cross_note_block_id_with_display() {
        let out = process("[[My Note#^block-id|See here]]", false);
        assert_eq!(out, "[See here](My%20Note.md#block-id)");
    }

    // --- wikilinks ---

    #[test]
    fn converts_plain_wikilink() {
        let out = process("[[My Note]]", false);
        assert_eq!(out, "[My Note](My%20Note.md)");
    }

    #[test]
    fn converts_wikilink_with_display() {
        let out = process("[[My Note|See here]]", false);
        assert_eq!(out, "[See here](My%20Note.md)");
    }

    #[test]
    fn converts_wikilink_with_heading() {
        let out = process("[[My Note#Section Title]]", false);
        assert_eq!(out, "[My Note](My%20Note.md#section-title)");
    }

    #[test]
    fn preserves_embed_wikilink() {
        let input = "![[image.png]]";
        assert_eq!(process(input, false), input);
    }

    #[test]
    fn preserves_excalidraw_wikilink() {
        let input = "[[Drawing.excalidraw]]";
        assert_eq!(process(input, false), input);
    }

    #[test]
    fn preserves_wikilink_in_code_block() {
        let input = "```\n[[Not converted]]\n```";
        assert_eq!(process(input, false), input);
    }
}
