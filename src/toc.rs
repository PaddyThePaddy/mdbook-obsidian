use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use mdbook_preprocessor::book::{Book, BookItem, Chapter, SectionNumber};
use mdbook_preprocessor::PreprocessorContext;
use regex::Regex;

use crate::excalidraw::excalidraw_slug;

const TOC_PLACEHOLDER: &str = "<!-- mdbook-obsidian toc -->";

pub(crate) fn run_toc_pass(ctx: &PreprocessorContext, book: &mut Book, verbose: bool) {
    let generate_toc = ctx
        .config
        .get::<bool>("preprocessor.obsidian.generate_toc")
        .unwrap_or(None)
        .unwrap_or(false);
    if !generate_toc {
        return;
    }

    let custom_ignore = ctx
        .config
        .get::<String>("preprocessor.obsidian.toc_ignore_file")
        .unwrap_or(None);

    let src_dir = ctx.root.join(&ctx.config.book.src);
    let summary_content =
        std::fs::read_to_string(src_dir.join("SUMMARY.md")).unwrap_or_default();

    let covered = covered_paths(&book.items);
    let files = scan_files(&src_dir, &covered, custom_ignore.as_deref(), verbose);

    if files.is_empty() {
        if verbose {
            eprintln!(" INFO [mdbook-obsidian]: toc: no uncovered files found");
        }
        return;
    }

    // Continue section numbering after existing top-level numbered chapters.
    let start_num = book
        .items
        .iter()
        .filter_map(|item| {
            if let BookItem::Chapter(ch) = item {
                ch.number.as_ref().and_then(|n| n.first().copied())
            } else {
                None
            }
        })
        .max()
        .unwrap_or(0)
        + 1;

    let new_items = build_items(&files, &src_dir, Path::new(""), &[], &[], start_num);
    if new_items.is_empty() {
        return;
    }

    let toc_markdown = render_markdown(&new_items, 0);

    let insert_at =
        placeholder_index(&summary_content, &book.items).unwrap_or(book.items.len());

    if verbose {
        eprintln!(
            " INFO [mdbook-obsidian]: toc: inserting {} top-level item(s) at position {insert_at}",
            new_items.len()
        );
    }

    for (i, item) in new_items.into_iter().enumerate() {
        book.items.insert(insert_at + i, item);
    }

    // Replace the placeholder in all chapter content (existing + newly inserted).
    book.for_each_mut(|item| {
        if let BookItem::Chapter(ch) = item {
            if ch.content.contains(TOC_PLACEHOLDER) {
                ch.content = ch.content.replace(TOC_PLACEHOLDER, &toc_markdown);
            }
        }
    });
}

/// Collect source_paths of all chapters already in the book (recursive).
fn covered_paths(items: &[BookItem]) -> HashSet<PathBuf> {
    let mut covered = HashSet::new();
    for item in items {
        if let BookItem::Chapter(ch) = item {
            if let Some(sp) = &ch.source_path {
                covered.insert(sp.clone());
            }
            covered.extend(covered_paths(&ch.sub_items));
        }
    }
    covered
}

/// Walk `src_dir` respecting `.gitignore` (and optional extra ignore file).
/// Returns uncovered `.md` files sorted by relative path.
fn scan_files(
    src_dir: &Path,
    covered: &HashSet<PathBuf>,
    custom_ignore: Option<&str>,
    verbose: bool,
) -> Vec<PathBuf> {
    let mut builder = ignore::WalkBuilder::new(src_dir);
    builder.standard_filters(true);
    if let Some(f) = custom_ignore {
        builder.add_custom_ignore_filename(f);
    }

    let mut files = Vec::new();
    for result in builder.build() {
        let entry = match result {
            Ok(e) => e,
            Err(err) => {
                if verbose {
                    eprintln!(" WARN [mdbook-obsidian]: toc walk error: {err}");
                }
                continue;
            }
        };
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let rel = match path.strip_prefix(src_dir) {
            Ok(r) => r.to_path_buf(),
            Err(_) => continue,
        };
        let name = rel.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name == "SUMMARY.md" {
            continue;
        }
        if !covered.contains(&rel) {
            files.push(rel);
        }
    }
    files.sort();
    files
}

/// Recursively build `BookItem`s from a sorted list of relative file paths.
/// Directories with an `index.md` / `README.md` become clickable section headers;
/// otherwise they become draft (non-clickable) headers.
///
/// `number_prefix` is the section-number ancestry (e.g. `[3]` means we're inside
/// the third top-level group). `start_idx` is the 1-based index for the first
/// item at the current depth; siblings increment it sequentially.
fn build_items(
    files: &[PathBuf],
    src_dir: &Path,
    prefix: &Path,
    parent_names: &[String],
    number_prefix: &[u32],
    start_idx: u32,
) -> Vec<BookItem> {
    let mut direct: Vec<&PathBuf> = Vec::new();
    let mut subdirs: BTreeMap<String, Vec<&PathBuf>> = BTreeMap::new();

    for file in files {
        let Ok(rel) = file.strip_prefix(prefix) else {
            continue;
        };
        let mut comps = rel.components();
        match (comps.next(), comps.next()) {
            (Some(_), None) => direct.push(file),
            (Some(first), Some(_)) => {
                let dir = first.as_os_str().to_string_lossy().into_owned();
                subdirs.entry(dir).or_default().push(file);
            }
            _ => {}
        }
    }

    let mut items: Vec<BookItem> = Vec::new();
    let mut counter = start_idx;

    // Direct files (already sorted from scan).
    for rel_path in direct {
        let content = std::fs::read_to_string(src_dir.join(rel_path)).unwrap_or_default();
        let raw_stem = rel_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Untitled");
        // Strip the inner .excalidraw extension for files like "Drawing.excalidraw.md".
        let name = raw_stem.strip_suffix(".excalidraw").unwrap_or(raw_stem).to_string();
        let mut num = number_prefix.to_vec();
        num.push(counter);
        counter += 1;
        items.push(BookItem::Chapter(Chapter {
            name,
            content,
            number: Some(SectionNumber::new(num)),
            sub_items: vec![],
            path: Some(rel_path.clone()),
            source_path: Some(rel_path.clone()),
            parent_names: parent_names.to_vec(),
        }));
    }

    // Subdirectories (BTreeMap keeps them sorted).
    for (dir_name, dir_files) in subdirs {
        let subdir = prefix.join(&dir_name);
        let display = dir_name.replace(['-', '_'], " ");

        let mut child_parents = parent_names.to_vec();
        child_parents.push(display.clone());

        // index.md or README.md becomes the section's own page.
        let index_rel = subdir.join("index.md");
        let readme_rel = subdir.join("README.md");
        let (section_content, section_path, section_source) =
            if src_dir.join(&index_rel).exists() {
                let c = std::fs::read_to_string(src_dir.join(&index_rel)).unwrap_or_default();
                (c, Some(index_rel.clone()), Some(index_rel.clone()))
            } else if src_dir.join(&readme_rel).exists() {
                let c = std::fs::read_to_string(src_dir.join(&readme_rel)).unwrap_or_default();
                (c, Some(readme_rel.clone()), Some(readme_rel.clone()))
            } else {
                (String::new(), None, None)
            };

        let children: Vec<PathBuf> = dir_files
            .iter()
            .filter(|f| **f != &index_rel && **f != &readme_rel)
            .map(|f| (*f).clone())
            .collect();

        let mut my_num = number_prefix.to_vec();
        my_num.push(counter);
        counter += 1;

        let sub_items = build_items(&children, src_dir, &subdir, &child_parents, &my_num, 1);

        if sub_items.is_empty() && section_path.is_none() {
            continue; // empty draft directory — skip
        }

        items.push(BookItem::Chapter(Chapter {
            name: display,
            content: section_content,
            number: Some(SectionNumber::new(my_num)),
            sub_items,
            path: section_path,
            source_path: section_source,
            parent_names: parent_names.to_vec(),
        }));
    }

    items
}

/// Find where to insert new chapters in `book.items` by locating the placeholder
/// in `SUMMARY.md` and counting which existing chapters precede it.
fn placeholder_index(summary_content: &str, items: &[BookItem]) -> Option<usize> {
    let placeholder_pos = summary_content.find(TOC_PLACEHOLDER)?;
    let before = &summary_content[..placeholder_pos];

    let link_re = Regex::new(r"\]\(([^)]+\.md)\)").expect("valid regex");
    let paths_before: HashSet<PathBuf> = link_re
        .captures_iter(before)
        .map(|c| PathBuf::from(urlencoding::decode(c[1].trim()).unwrap_or_default().as_ref()))
        .collect();

    // Last top-level item whose source_path was listed before the placeholder.
    let last_idx = items.iter().rposition(|item| {
        if let BookItem::Chapter(ch) = item {
            if let Some(sp) = &ch.source_path {
                return paths_before.contains(sp);
            }
        }
        false
    });

    Some(last_idx.map(|i| i + 1).unwrap_or(0))
}

/// Render generated chapters as a nested markdown list (for in-chapter placeholder replacement).
fn render_markdown(items: &[BookItem], depth: usize) -> String {
    let indent = "  ".repeat(depth);
    let mut out = String::new();
    for item in items {
        if let BookItem::Chapter(ch) = item {
            if let Some(path) = &ch.path {
                let path_str = path.to_string_lossy();
                // Excalidraw sources are redirected to _excalidraw/{slug}.html by Pass 2.
                // Compute that URL now rather than using the raw .excalidraw.md path,
                // which would be wrong and may contain spaces that break markdown links.
                let href = if path_str.ends_with(".excalidraw.md")
                    || path_str.ends_with(".excalidraw")
                {
                    format!("_excalidraw/{}.html", excalidraw_slug(&ch.name))
                } else {
                    // Percent-encode each path component so spaces and other
                    // special characters don't break markdown link syntax.
                    path.with_extension("html")
                        .components()
                        .map(|c| {
                            urlencoding::encode(&c.as_os_str().to_string_lossy()).into_owned()
                        })
                        .collect::<Vec<_>>()
                        .join("/")
                };
                out.push_str(&format!("{}- [{}]({})\n", indent, ch.name, href));
            } else {
                out.push_str(&format!("{}- {}\n", indent, ch.name));
            }
            if !ch.sub_items.is_empty() {
                out.push_str(&render_markdown(&ch.sub_items, depth + 1));
            }
        }
    }
    out
}
