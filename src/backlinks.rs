use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use mdbook_preprocessor::book::{Book, BookItem};
use regex::Regex;

struct Ref {
    name: String,
    source_path: PathBuf,
}

pub(crate) fn run_backlinks_pass(book: &mut Book, verbose: bool) {
    // Build stem → source_path index for wikilink resolution.
    let stem_index = build_stem_index(&book.items);

    let md_re = Regex::new(r"(!?)\[(?:[^\]]*)\]\(([^)]+)\)").expect("valid regex");
    let wiki_re = Regex::new(r"(!?)\[\[([^\]|#]+?)(?:[|#][^\]]*)?\]\]").expect("valid regex");

    // Collect outgoing links from every chapter and build the reverse map:
    // target source_path → list of chapters that link to it.
    let mut map: HashMap<PathBuf, Vec<Ref>> = HashMap::new();
    collect_from_items(&book.items, &stem_index, &md_re, &wiki_re, &mut map);

    if map.is_empty() {
        return;
    }

    let mut injected = 0usize;

    book.for_each_mut(|item| {
        if let BookItem::Chapter(ch) = item {
            let source = match &ch.source_path {
                Some(sp) if !is_excalidraw(sp) => sp.clone(),
                _ => return,
            };
            let refs = match map.get(&source) {
                Some(r) if !r.is_empty() => r,
                _ => return,
            };

            let from_dir = source.parent().unwrap_or(Path::new(""));
            let mut sorted: Vec<&Ref> = refs.iter().collect();
            sorted.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

            let mut section = String::from("\n\n---\n\n## Backlinks\n\n");
            for r in sorted {
                let url = relative_url(from_dir, &r.source_path);
                section.push_str(&format!("- [{}]({})\n", r.name, url));
            }
            ch.content.push_str(&section);
            injected += 1;
        }
    });

    if verbose {
        eprintln!(
            " INFO [mdbook-obsidian]: backlinks: injected sections into {injected} chapter(s)"
        );
    }
}

// ---------------------------------------------------------------------------
// Phase 1: build stem index
// ---------------------------------------------------------------------------

fn build_stem_index(items: &[BookItem]) -> HashMap<String, PathBuf> {
    let mut index: HashMap<String, PathBuf> = HashMap::new();
    for item in items {
        if let BookItem::Chapter(ch) = item {
            if let Some(sp) = &ch.source_path {
                if !is_excalidraw(sp) {
                    let stem = sp
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("")
                        .to_lowercase();
                    if !stem.is_empty() {
                        index.entry(stem).or_insert_with(|| sp.clone());
                    }
                }
            }
            for (k, v) in build_stem_index(&ch.sub_items) {
                index.entry(k).or_insert(v);
            }
        }
    }
    index
}

// ---------------------------------------------------------------------------
// Phase 2: collect outgoing links
// ---------------------------------------------------------------------------

fn collect_from_items(
    items: &[BookItem],
    stem_index: &HashMap<String, PathBuf>,
    md_re: &Regex,
    wiki_re: &Regex,
    map: &mut HashMap<PathBuf, Vec<Ref>>,
) {
    for item in items {
        if let BookItem::Chapter(ch) = item {
            if let Some(sp) = &ch.source_path {
                if !is_excalidraw(sp) {
                    let dir = sp.parent().unwrap_or(Path::new(""));
                    for target in extract_targets(&ch.content, dir, stem_index, md_re, wiki_re) {
                        if target != *sp {
                            map.entry(target).or_default().push(Ref {
                                name: ch.name.clone(),
                                source_path: sp.clone(),
                            });
                        }
                    }
                }
            }
            collect_from_items(&ch.sub_items, stem_index, md_re, wiki_re, map);
        }
    }
}

fn extract_targets(
    content: &str,
    chapter_dir: &Path,
    stem_index: &HashMap<String, PathBuf>,
    md_re: &Regex,
    wiki_re: &Regex,
) -> Vec<PathBuf> {
    let mut targets: Vec<PathBuf> = Vec::new();
    let mut in_fence = false;
    let mut fence_char = '`';
    let mut fence_len = 0usize;

    for line in content.lines() {
        let trimmed = line.trim_start();
        let (is_fence, fc, fl) = crate::detect_fence(trimmed);

        if in_fence {
            if is_fence && fc == fence_char && fl >= fence_len {
                in_fence = false;
            }
            continue;
        }
        if is_fence {
            in_fence = true;
            fence_char = fc;
            fence_len = fl;
            continue;
        }

        // Markdown links: [text](url)
        for cap in md_re.captures_iter(line) {
            if cap.get(1).map_or("", |m| m.as_str()) == "!" {
                continue; // image embed
            }
            let raw_url = cap.get(2).map_or("", |m| m.as_str());
            if let Some(p) = resolve_md_link(raw_url, chapter_dir) {
                targets.push(p);
            }
        }

        // Wikilinks: [[Note Name]] — present when obsidian_syntax pass is disabled
        // or runs after this pass. When obsidian_syntax is enabled, wikilinks are
        // already converted to markdown links and caught by the md_re branch above.
        for cap in wiki_re.captures_iter(line) {
            if cap.get(1).map_or("", |m| m.as_str()) == "!" {
                continue; // ![[embed]]
            }
            let name = cap.get(2).map_or("", |m| m.as_str()).trim();
            if name.ends_with(".excalidraw") {
                continue;
            }
            let stem = Path::new(name)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(name)
                .to_lowercase();
            if let Some(p) = stem_index.get(&stem) {
                targets.push(p.clone());
            }
        }
    }

    targets.sort();
    targets.dedup();
    targets
}

fn resolve_md_link(raw: &str, chapter_dir: &Path) -> Option<PathBuf> {
    // Strip optional title: `url "title"` → `url`
    let url = raw.split_whitespace().next().unwrap_or(raw);

    if url.starts_with("http://")
        || url.starts_with("https://")
        || url.starts_with("//")
        || url.starts_with('#')
        || url.is_empty()
    {
        return None;
    }

    // Strip anchor and query string; keep only the path portion.
    let path_part = url.split('#').next().unwrap_or("").split('?').next().unwrap_or("");
    if path_part.is_empty() {
        return None;
    }

    // Only follow .md links; skip images, PDFs, excalidraw files, etc.
    let lower = path_part.to_lowercase();
    if !lower.ends_with(".md") || lower.ends_with(".excalidraw.md") {
        return None;
    }

    let decoded = urlencoding::decode(path_part)
        .map(|c| c.into_owned())
        .unwrap_or_else(|_| path_part.to_string());

    Some(normalize_path(&chapter_dir.join(decoded)))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn normalize_path(p: &Path) -> PathBuf {
    let mut out: Vec<Component> = Vec::new();
    for comp in p.components() {
        match comp {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            c => out.push(c),
        }
    }
    out.iter().collect()
}

/// Compute a URL-encoded relative path from `from_dir` to `to`, suitable for
/// use in a markdown link inside a chapter whose source is inside `from_dir`.
fn relative_url(from_dir: &Path, to: &Path) -> String {
    let from: Vec<_> = from_dir.components().collect();
    let to: Vec<_> = to.components().collect();

    let common = from.iter().zip(to.iter()).take_while(|(a, b)| a == b).count();

    let up = from.len() - common;
    let mut parts: Vec<String> = (0..up).map(|_| "..".to_string()).collect();
    for comp in &to[common..] {
        parts.push(urlencoding::encode(&comp.as_os_str().to_string_lossy()).into_owned());
    }

    if parts.is_empty() {
        ".".to_string()
    } else {
        parts.join("/")
    }
}

fn is_excalidraw(p: &Path) -> bool {
    let s = p.to_string_lossy();
    s.ends_with(".excalidraw.md") || s.ends_with(".excalidraw")
}
