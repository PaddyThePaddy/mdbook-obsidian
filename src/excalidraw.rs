use std::borrow::Cow;
use std::fmt::Write as FmtWrite;
use std::path::{Path, PathBuf};

use mdbook_preprocessor::book::Chapter;
use regex::Regex;

const EXCALIDRAW_TEMPLATE: &str = include_str!("assets/excalidraw-page.html");

pub(crate) struct ExcalidrawRef {
    /// Absolute path to the .excalidraw file on disk.
    pub(crate) file_path: PathBuf,
    /// URL-safe slug used for `_excalidraw/{slug}.html`.
    pub(crate) slug: String,
    /// Human-readable display name (file stem, original casing).
    pub(crate) name: String,
}

pub(crate) fn excalidraw_slug(decoded_stem: &str) -> String {
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
pub(crate) fn excalidraw_href(slug: &str, depth: usize) -> String {
    format!("{}_excalidraw/{}.html", "../".repeat(depth), slug)
}

/// Build a synthetic chapter that renders the Excalidraw scene.
pub(crate) fn make_excalidraw_chapter(r: &ExcalidrawRef, json: &str) -> Chapter {
    // Split template once so we can stream-write without an intermediate copy.
    let (tmpl_before, tmpl_after) = EXCALIDRAW_TEMPLATE
        .split_once("{{SCENE_JSON}}")
        .expect("template missing {{SCENE_JSON}}");

    let mut html =
        String::with_capacity(tmpl_before.len() + json.len() + tmpl_after.len() + 10);
    html.push_str(tmpl_before);
    // Write JSON with `</` → `<\/` escaping inline — avoids a full intermediate copy.
    let mut rest = json;
    while let Some(pos) = rest.find("</") {
        html.push_str(&rest[..pos]);
        html.push_str("<\\/");
        rest = &rest[pos + 2..];
    }
    html.push_str(rest);
    html.push_str(tmpl_after);

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

/// Build a synthetic chapter that shows a human-readable error when the scene
/// data can't be extracted.
pub(crate) fn make_excalidraw_error_chapter(r: &ExcalidrawRef, error: &str) -> Chapter {
    let html = format!(
        "<div style=\"padding:2rem;border:2px solid #c00;border-radius:6px;\
         background:#fff5f5;color:#900;font-family:sans-serif\">\
         <h2>Cannot display: {name}</h2><p>{error}</p></div>",
        name = &r.name,
        error = error,
    );
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

/// Parse the `## Embedded Files` section of an `.excalidraw.md` file.
/// Returns pairs of `(fileId, vault-relative-path)`.
fn parse_embedded_files(content: &str) -> Vec<(String, String)> {
    let mut result = Vec::new();
    let section_start = match content.find("## Embedded Files") {
        Some(pos) => pos + "## Embedded Files".len(),
        None => return result,
    };
    // Section ends at the next `##` heading, `%%`, or EOF.
    let section_end = content[section_start..]
        .find("\n## ")
        .or_else(|| content[section_start..].find("\n%%"))
        .map(|p| section_start + p)
        .unwrap_or(content.len());

    for line in content[section_start..section_end].lines() {
        // Each line looks like: `<fileId>: [[path/to/file.png]]`
        if let Some((id, rest)) = line.split_once(": [[") {
            let id = id.trim().to_string();
            if !id.is_empty() {
                if let Some(path) = rest.strip_suffix("]]") {
                    result.push((id, path.to_string()));
                }
            }
        }
    }
    result
}

/// Inject embedded image files into the Excalidraw JSON's `files` object using
/// string manipulation — avoids deserialising the full JSON into a Value tree
/// (which can use 5–10× the raw byte size for complex drawings).
///
/// Images are referenced by relative URL rather than base64-encoded data URIs.
/// Excalidraw passes `files[id].dataURL` directly to `img.src`, so any valid
/// URL (including relative paths) is accepted by the browser at render time.
fn inject_embedded_files(
    json: &str,
    file_path: &Path,
    src_dir: &Path,
    embedded: &[(String, String)],
) -> String {
    if embedded.is_empty() {
        return json.to_string();
    }
    let dir = file_path.parent().unwrap_or(Path::new("."));

    // Build the new entries as a JSON fragment: `"id":{...},"id2":{...}`
    let mut new_entries = String::new();
    for (id, rel_path) in embedded {
        // Check whether the ID is already a *key* in the files object.
        // Using `"id":` avoids false-positives from `"fileId":"id"` values
        // that appear in every image element inside the elements array.
        if json.contains(&format!("\"{}\":", id)) {
            continue;
        }
        // Try path relative to the excalidraw file first, then vault-root
        // relative (how Obsidian stores embedded file paths in the section).
        let img_abs = {
            let from_dir = dir.join(rel_path);
            if from_dir.exists() { from_dir } else { src_dir.join(rel_path) }
        };
        if !img_abs.exists() {
            continue;
        }
        // Excalidraw viewer pages live one level deep (_excalidraw/<slug>.html).
        // Prefix the src-relative image path with "../" to reach the output root.
        // mdBook copies all non-markdown source files to the same relative path
        // in the output, so this URL resolves correctly at serve time.
        let img_src_rel = img_abs
            .strip_prefix(src_dir)
            .unwrap_or(Path::new(rel_path));
        let url = format!("../{}", img_src_rel.to_string_lossy().replace('\\', "/"));
        let mime = match img_abs.extension().and_then(|e| e.to_str()) {
            Some("jpg") | Some("jpeg") => "image/jpeg",
            Some("gif") => "image/gif",
            Some("svg") => "image/svg+xml",
            Some("webp") => "image/webp",
            _ => "image/png",
        };
        if !new_entries.is_empty() {
            new_entries.push(',');
        }
        // IDs and MIME types are safe without escaping; URLs use only
        // ASCII path characters from the source filesystem.
        write!(
            new_entries,
            r#""{id}":{{"mimeType":"{mime}","id":"{id}","dataURL":"{url}","created":0}}"#
        )
        .unwrap();
    }

    if new_entries.is_empty() {
        return json.to_string();
    }

    inject_into_files_object(json, &new_entries).unwrap_or_else(|| json.to_string())
}

/// Insert `new_entries` (JSON key:value pairs without surrounding braces) into
/// the `"files"` object already present in `json`, without a full JSON parse.
fn inject_into_files_object(json: &str, new_entries: &str) -> Option<String> {
    // Locate the "files" key.
    let key_pos = json.find("\"files\"")?;
    let after_key = &json[key_pos + 7..]; // skip past `"files"`

    // Find the `:` separator.
    let colon_offset = after_key.find(':')?;
    let after_colon = &after_key[colon_offset + 1..];

    // Skip optional whitespace and verify the value is an object.
    let ws = after_colon.len()
        - after_colon
            .trim_start_matches(|c: char| c.is_ascii_whitespace())
            .len();
    let open_abs = key_pos + 7 + colon_offset + 1 + ws;
    if json.as_bytes().get(open_abs) != Some(&b'{') {
        return None; // value is not an object — bail out
    }

    // Find the matching closing `}`.
    let close_rel = find_matching_close_brace(&json[open_abs..])?;
    let close_abs = open_abs + close_rel;

    let inner = json[open_abs + 1..close_abs].trim();

    let mut result = String::with_capacity(json.len() + new_entries.len() + 2);
    result.push_str(&json[..close_abs]);
    if !inner.is_empty() {
        result.push(',');
    }
    result.push_str(new_entries);
    result.push_str(&json[close_abs..]);
    Some(result)
}

/// Return the byte index of the `}` that closes the `{` at position 0 of `s`.
/// Correctly skips over string literals (including escaped characters).
fn find_matching_close_brace(s: &str) -> Option<usize> {
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    for (i, b) in s.bytes().enumerate() {
        if escaped {
            escaped = false;
            continue;
        }
        if in_string {
            match b {
                b'\\' => escaped = true,
                b'"' => in_string = false,
                _ => {}
            }
        } else {
            match b {
                b'"' => in_string = true,
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i);
                    }
                }
                _ => {}
            }
        }
    }
    None
}

/// Read the Excalidraw JSON from disk, returning `Err` with a human-readable
/// message when the file is missing, compressed, or in an unrecognised format.
///
/// Handles two on-disk formats:
///   - `name.excalidraw`    — the whole file is a raw JSON object
///   - `name.excalidraw.md` — JSON is inside a ` ```json … ``` ` block
/// Falls back to the `.md` variant automatically when the plain path isn't found.
pub(crate) fn read_excalidraw_json(file_path: &Path, src_dir: &Path) -> Result<String, String> {
    let md_path;
    let paths: &[&Path] = if file_path.to_str().map_or(false, |s| s.ends_with(".md")) {
        &[file_path]
    } else {
        md_path = PathBuf::from(format!("{}.md", file_path.display()));
        &[file_path, md_path.as_path()]
    };

    for &path in paths {
        if let Ok(content) = std::fs::read_to_string(path) {
            if path.to_str().map_or(false, |s| s.ends_with(".excalidraw.md")) {
                let embedded = parse_embedded_files(&content);
                return extract_json_from_excalidraw_md(&content)
                    .map(|json| inject_embedded_files(&json, path, src_dir, &embedded))
                    .ok_or_else(|| {
                        if content.contains("```compressed-json") {
                            "Excalidraw file uses compressed format. \
                             Open it in Obsidian and run \
                             \"Decompress current Excalidraw file\" from the command palette, \
                             then rebuild the book."
                                .to_string()
                        } else {
                            format!(
                                "No ```json block found in {}. \
                                 The file may use an unsupported Excalidraw format.",
                                path.display()
                            )
                        }
                    });
            } else {
                return Ok(content);
            }
        }
    }
    Err(format!("Excalidraw file not found: {}", file_path.display()))
}

/// Extract the raw JSON object from an `.excalidraw.md` file.
/// Returns a borrowed slice when the JSON is uncompressed (no allocation),
/// or an owned String after decompression.
fn extract_json_from_excalidraw_md(content: &str) -> Option<Cow<'_, str>> {
    if let Some(json) = extract_fenced_block(content, "```json\n") {
        return Some(Cow::Borrowed(json));
    }
    if let Some(compressed) = extract_fenced_block(content, "```compressed-json\n") {
        let u16s = lz_str::decompress_from_base64(compressed)?;
        return String::from_utf16(&u16s).ok().map(Cow::Owned);
    }
    None
}

/// Return a slice of `content` containing only what is between `marker` and
/// the next closing ` ``` ` — no allocation for the common uncompressed case.
fn extract_fenced_block<'a>(content: &'a str, marker: &str) -> Option<&'a str> {
    let start = content.find(marker)? + marker.len();
    let rest = &content[start..];
    let end = rest.find("\n```")?;
    Some(&rest[..end])
}

/// Scan `content` for excalidraw links and rewrite them to viewer URLs.
/// Collects each referenced file into `refs` for later synthetic-chapter injection.
pub(crate) fn process_excalidraw(
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
            let line = replace_wikilinks(wiki_re, line, chapter_dir, depth, verbose, refs);
            let line = replace_links(link_re, &line, chapter_dir, depth, verbose, refs);
            result.push_str(&line);
        }
    }

    if trailing_newline && !result.ends_with('\n') {
        result.push('\n');
    }
    result
}

/// Replace `![[file.excalidraw]]` and `[[file.excalidraw|alias]]` wikilinks.
fn replace_wikilinks(
    wiki_re: &Regex,
    text: &str,
    chapter_dir: &Path,
    depth: usize,
    verbose: bool,
    refs: &mut Vec<ExcalidrawRef>,
) -> String {
    wiki_re
        .replace_all(text, |caps: &regex::Captures| {
            let raw_path = &caps[2];
            let alias = caps.get(3).map(|m| m.as_str());

            let decoded = urlencoding::decode(raw_path)
                .unwrap_or(std::borrow::Cow::Borrowed(raw_path));
            let path = Path::new(decoded.as_ref());
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("drawing");
            let slug = excalidraw_slug(stem);
            let display = alias.unwrap_or(stem).to_string();
            let href = excalidraw_href(&slug, depth);

            let base = chapter_dir.join(decoded.as_ref());
            let file_path = if base.exists() {
                base.clone()
            } else {
                PathBuf::from(format!("{}.md", base.display()))
            };

            if verbose {
                eprintln!(" INFO [mdbook-obsidian]: excalidraw wikilink: {raw_path}  =>  {href}");
            }
            refs.push(ExcalidrawRef { file_path, slug, name: stem.to_string() });
            format!("[{}]({})", display, href)
        })
        .into_owned()
}

/// Replace `[text](file.excalidraw)` and `[text](file.excalidraw.md)` markdown links.
fn replace_links(
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

            let stem_raw = path.file_stem().and_then(|s| s.to_str()).unwrap_or("drawing");
            let stem = stem_raw.strip_suffix(".excalidraw").unwrap_or(stem_raw);

            let slug = excalidraw_slug(stem);
            let href = excalidraw_href(&slug, depth);
            let file_path = chapter_dir.join(decoded.as_ref());

            if verbose {
                eprintln!(" INFO [mdbook-obsidian]: excalidraw link: {raw_path}  =>  {href}");
            }
            refs.push(ExcalidrawRef { file_path, slug, name: stem.to_string() });

            let _ = bang; // image-style embed treated identically — becomes a link
            format!("[{}]({})", link_text, href)
        })
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wiki_re() -> Regex {
        Regex::new(r"(!?)\[\[([^\]]+\.excalidraw)(?:\|([^\]]*))?\]\]").unwrap()
    }

    fn link_re() -> Regex {
        Regex::new(r"(!?)\[([^\]]*)\]\(([^)]*\.excalidraw(?:\.md)?)\)").unwrap()
    }

    #[test]
    fn slug_lowercases_and_hyphenates() {
        assert_eq!(excalidraw_slug("My Drawing"), "my-drawing");
    }

    #[test]
    fn slug_strips_non_alphanumeric() {
        assert_eq!(excalidraw_slug("plan (v2)"), "plan-v2");
    }

    #[test]
    fn replaces_excalidraw_wikilink() {
        let mut refs = vec![];
        let got = replace_wikilinks(
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
        let got = replace_wikilinks(
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
        let got = replace_wikilinks(
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
        let got = replace_links(
            &link_re(),
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
    fn replaces_excalidraw_md_link() {
        let mut refs = vec![];
        let got = replace_links(
            &link_re(),
            "[Castle word puzzle.excalidraw](../Castle%20word%20puzzle.excalidraw.md)",
            Path::new("/src/a/b"),
            2,
            false,
            &mut refs,
        );
        assert_eq!(
            got,
            "[Castle word puzzle.excalidraw](../../_excalidraw/castle-word-puzzle.html)"
        );
        assert_eq!(refs[0].slug, "castle-word-puzzle");
        assert_eq!(refs[0].name, "Castle word puzzle");
    }

    #[test]
    fn extracts_json_from_excalidraw_md() {
        let content = "---\nexcalidraw-plugin: parsed\n---\n\n%%\n# Drawing\n```json\n{\"type\":\"excalidraw\"}\n```\n%%\n";
        let json = extract_json_from_excalidraw_md(content).unwrap();
        assert_eq!(json, r#"{"type":"excalidraw"}"#);
    }

    #[test]
    fn decompresses_compressed_json_from_excalidraw_md() {
        let json = r#"{"type":"excalidraw","version":2,"elements":[]}"#;
        let utf16: Vec<u16> = json.encode_utf16().collect();
        let compressed_str = lz_str::compress_to_base64(&utf16);
        let content = format!(
            "---\nexcalidraw-plugin: parsed\n---\n\n# Drawing\n```compressed-json\n{compressed_str}\n```\n"
        );
        let result = extract_json_from_excalidraw_md(&content).unwrap();
        assert_eq!(result, json);
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

    #[test]
    fn inject_into_empty_files_object() {
        let json = r#"{"type":"excalidraw","files":{}}"#;
        let entries = r#""abc":{"mimeType":"image/png","id":"abc","dataURL":"data:image/png;base64,AA==","created":0}"#;
        let result = inject_into_files_object(json, entries).unwrap();
        assert!(result.contains(r#""files":{"abc":"#));
    }

    #[test]
    fn inject_into_non_empty_files_object() {
        let json = r#"{"type":"excalidraw","files":{"existing":{"id":"existing"}}}"#;
        let entries = r#""new":{"id":"new"}"#;
        let result = inject_into_files_object(json, entries).unwrap();
        assert!(result.contains(r#""existing":{"id":"existing"},"new":{"id":"new"}"#));
    }

    #[test]
    fn find_brace_handles_nested_objects() {
        let s = r#"{"a":{"b":1},"c":2}"#;
        let close = find_matching_close_brace(s).unwrap();
        assert_eq!(&s[close..], "}");
        assert_eq!(close, s.len() - 1);
    }

    #[test]
    fn find_brace_handles_braces_in_strings() {
        let s = r#"{"key":"val}ue"}"#;
        let close = find_matching_close_brace(s).unwrap();
        assert_eq!(close, s.len() - 1);
    }
}
