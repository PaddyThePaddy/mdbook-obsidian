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

/// Inject embedded image files into the Excalidraw JSON's `files` object.
/// `file_path` is the path of the `.excalidraw.md` file; image paths in
/// `embedded` are resolved relative to its parent directory.
fn inject_embedded_files(
    json: &str,
    file_path: &Path,
    embedded: &[(String, String)],
) -> String {
    if embedded.is_empty() {
        return json.to_string();
    }
    let mut scene: serde_json::Value = match serde_json::from_str(json) {
        Ok(v) => v,
        Err(_) => return json.to_string(),
    };
    let dir = file_path.parent().unwrap_or(Path::new("."));
    let files_map = match scene.get_mut("files").and_then(|f| f.as_object_mut()) {
        Some(m) => m,
        None => return json.to_string(),
    };
    for (id, rel_path) in embedded {
        if files_map.contains_key(id) {
            continue; // already has inline data
        }
        let img_path = dir.join(rel_path);
        if let Ok(data) = std::fs::read(&img_path) {
            use base64::Engine as _;
            let b64 = base64::engine::general_purpose::STANDARD.encode(&data);
            let mime = match img_path.extension().and_then(|e| e.to_str()) {
                Some("jpg") | Some("jpeg") => "image/jpeg",
                Some("gif") => "image/gif",
                Some("svg") => "image/svg+xml",
                Some("webp") => "image/webp",
                _ => "image/png",
            };
            files_map.insert(
                id.clone(),
                serde_json::json!({
                    "mimeType": mime,
                    "id": id,
                    "dataURL": format!("data:{mime};base64,{b64}"),
                    "created": 0
                }),
            );
        }
    }
    serde_json::to_string(&scene).unwrap_or_else(|_| json.to_string())
}

/// Read the Excalidraw JSON from disk, returning `Err` with a human-readable
/// message when the file is missing, compressed, or in an unrecognised format.
///
/// Handles two on-disk formats:
///   - `name.excalidraw`    — the whole file is a raw JSON object
///   - `name.excalidraw.md` — JSON is inside a ` ```json … ``` ` block
/// Falls back to the `.md` variant automatically when the plain path isn't found.
pub(crate) fn read_excalidraw_json(file_path: &Path) -> Result<String, String> {
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
                    .map(|json| inject_embedded_files(&json, path, &embedded))
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
///
/// Supports two formats produced by the Obsidian Excalidraw plugin:
///   - Uncompressed: scene JSON inside a ` ```json ` fenced block
///   - Compressed:   LZ-string (compressToBase64) data inside a ` ```compressed-json ` block
fn extract_json_from_excalidraw_md(content: &str) -> Option<String> {
    if let Some(json) = extract_fenced_block(content, "```json\n") {
        return Some(json);
    }
    if let Some(compressed) = extract_fenced_block(content, "```compressed-json\n") {
        let u16s = lz_str::decompress_from_base64(&compressed)?;
        return String::from_utf16(&u16s).ok();
    }
    None
}

fn extract_fenced_block(content: &str, marker: &str) -> Option<String> {
    let start = content.find(marker)? + marker.len();
    let rest = &content[start..];
    let end = rest.find("\n```")?;
    Some(rest[..end].to_string())
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
}
