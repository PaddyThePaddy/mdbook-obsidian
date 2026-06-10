mod anchors;
mod backlinks;
mod breaks;
mod embed;
mod excalidraw;
mod lightbox;
mod obsidian_syntax;
mod toc;

use std::collections::HashSet;
use std::io;
use std::path::PathBuf;
use std::process;

use mdbook_preprocessor::book::{Book, BookItem};
use mdbook_preprocessor::errors::Result;
use mdbook_preprocessor::{parse_input, Preprocessor, PreprocessorContext};
use regex::Regex;

use excalidraw::{
    excalidraw_slug, make_excalidraw_chapter, make_excalidraw_error_chapter, process_excalidraw,
    read_excalidraw_json, ExcalidrawRef,
};

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

        if verbose {
            eprintln!(" INFO [mdbook-obsidian]: running");
        }

        // --- Pass 0: TOC generation --------------------------------------
        toc::run_toc_pass(ctx, &mut book, verbose);

        // --- Pass 1: anchor normalization --------------------------------
        let link_re = Regex::new(r"(!?)\[([^\]]*)\]\(([^)]+)\)").expect("valid regex");
        book.for_each_mut(|item| {
            if let BookItem::Chapter(chapter) = item {
                chapter.content = anchors::process_content(&link_re, &chapter.content, verbose);
            }
        });

        // --- Pass 2: Excalidraw link detection and rewrite ---------------
        let excalidraw_enabled = ctx
            .config
            .get::<bool>("preprocessor.obsidian.excalidraw")
            .unwrap_or(None)
            .unwrap_or(false);

        let src_dir = ctx.root.join(&ctx.config.book.src);

        if excalidraw_enabled {
            let excalidraw_link_re =
                Regex::new(r"(!?)\[([^\]]*)\]\(([^)]*\.excalidraw(?:\.md)?)\)").expect("valid regex");
            let excalidraw_wiki_re =
                Regex::new(r"(!?)\[\[([^\]]+\.excalidraw)(?:\|([^\]]*))?\]\]").expect("valid regex");

            let mut all_refs: Vec<ExcalidrawRef> = Vec::new();
            // Slugs of chapters that ARE excalidraw files (listed directly in SUMMARY.md or
            // discovered by the TOC pass). We convert them in-place; the injection loop below
            // skips their slugs to avoid creating a duplicate chapter.
            let mut summary_slugs: HashSet<String> = HashSet::new();

            book.for_each_mut(|item| {
                if let BookItem::Chapter(chapter) = item {
                    let is_excalidraw_source = chapter
                        .source_path
                        .as_ref()
                        .and_then(|p| p.to_str())
                        .map(|s| s.ends_with(".excalidraw.md") || s.ends_with(".excalidraw"))
                        .unwrap_or(false);

                    if is_excalidraw_source {
                        // Convert the chapter in-place to the viewer page.
                        let source = chapter.source_path.as_ref().unwrap();
                        let file_path = src_dir.join(source);

                        let stem_raw = source
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("drawing");
                        let name =
                            stem_raw.strip_suffix(".excalidraw").unwrap_or(stem_raw).to_string();
                        let slug = excalidraw_slug(&name);

                        let r = ExcalidrawRef { file_path: file_path.clone(), slug: slug.clone(), name };

                        chapter.content = match read_excalidraw_json(&file_path) {
                            Ok(json) => make_excalidraw_chapter(&r, &json).content,
                            Err(msg) => {
                                if verbose {
                                    eprintln!(" WARN [mdbook-obsidian]: {}: {msg}", file_path.display());
                                }
                                make_excalidraw_error_chapter(&r, &msg).content
                            }
                        };
                        chapter.path = Some(PathBuf::from(format!("_excalidraw/{}.md", slug)));
                        summary_slugs.insert(slug);
                    } else {
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
                }
            });

            // Inject one synthetic chapter per unique excalidraw slug not already handled above.
            let mut seen = summary_slugs;
            for r in &all_refs {
                if !seen.insert(r.slug.clone()) {
                    continue;
                }
                let chapter = match read_excalidraw_json(&r.file_path) {
                    Ok(json) => make_excalidraw_chapter(r, &json),
                    Err(msg) => {
                        if verbose {
                            eprintln!(" WARN [mdbook-obsidian]: {}: {msg}", r.file_path.display());
                        }
                        make_excalidraw_error_chapter(r, &msg)
                    }
                };
                book.items.push(BookItem::Chapter(chapter));
            }
        }

        // --- Pass 3: hard line breaks ------------------------------------
        let hard_line_breaks = ctx
            .config
            .get::<bool>("preprocessor.obsidian.hard_line_breaks")
            .unwrap_or(None)
            .unwrap_or(false);

        if hard_line_breaks {
            book.for_each_mut(|item| {
                if let BookItem::Chapter(chapter) = item {
                    let path_str = chapter
                        .path
                        .as_ref()
                        .and_then(|p| p.to_str())
                        .unwrap_or("");
                    if path_str.is_empty() || path_str.starts_with("_excalidraw/") {
                        return;
                    }
                    chapter.content = breaks::patch_hard_line_breaks(&chapter.content);
                }
            });
        }

        // --- Pass 4: heading insertion -----------------------------------
        let insert_heading = ctx
            .config
            .get::<bool>("preprocessor.obsidian.insert_heading")
            .unwrap_or(None)
            .unwrap_or(false);

        if insert_heading {
            book.for_each_mut(|item| {
                if let BookItem::Chapter(chapter) = item {
                    let path_str = chapter
                        .path
                        .as_ref()
                        .and_then(|p| p.to_str())
                        .unwrap_or("");
                    if path_str.is_empty() || path_str.starts_with("_excalidraw/") {
                        return;
                    }
                    let first = chapter.content.lines().find(|l| !l.trim().is_empty());
                    if first.map(|l| l.starts_with("# ")).unwrap_or(false) {
                        return;
                    }
                    let heading = format!("# {}\n\n", chapter.name);
                    chapter.content.insert_str(0, &heading);
                }
            });
        }

        // --- Pass 5: Obsidian-flavored syntax ---------------------------------
        let obsidian_syntax = ctx
            .config
            .get::<bool>("preprocessor.obsidian.obsidian_syntax")
            .unwrap_or(None)
            .unwrap_or(false);

        if obsidian_syntax {
            book.for_each_mut(|item| {
                if let BookItem::Chapter(chapter) = item {
                    let path_str = chapter
                        .path
                        .as_ref()
                        .and_then(|p| p.to_str())
                        .unwrap_or("");
                    if path_str.is_empty() || path_str.starts_with("_excalidraw/") {
                        return;
                    }
                    chapter.content =
                        obsidian_syntax::process(&chapter.content, verbose);
                }
            });
        }

        // --- Pass 6: Backlinks -----------------------------------------------
        let backlinks_enabled = ctx
            .config
            .get::<bool>("preprocessor.obsidian.backlinks")
            .unwrap_or(None)
            .unwrap_or(false);

        if backlinks_enabled {
            backlinks::run_backlinks_pass(&mut book, verbose);
        }

        // --- Pass 7: Image lightbox ------------------------------------------
        let lightbox = ctx
            .config
            .get::<bool>("preprocessor.obsidian.lightbox")
            .unwrap_or(None)
            .unwrap_or(false);

        if lightbox {
            book.for_each_mut(|item| {
                if let BookItem::Chapter(chapter) = item {
                    let path_str = chapter
                        .path
                        .as_ref()
                        .and_then(|p| p.to_str())
                        .unwrap_or("");
                    if path_str.is_empty() || path_str.starts_with("_excalidraw/") {
                        return;
                    }
                    chapter.content = lightbox::process(&chapter.content);
                }
            });
        }

        // --- Pass 8: Embed (YouTube, etc.) ----------------------------------
        let embed_enabled = ctx
            .config
            .get::<bool>("preprocessor.obsidian.embed")
            .unwrap_or(None)
            .unwrap_or(false);

        if embed_enabled {
            book.for_each_mut(|item| {
                if let BookItem::Chapter(chapter) = item {
                    let path_str = chapter
                        .path
                        .as_ref()
                        .and_then(|p| p.to_str())
                        .unwrap_or("");
                    if path_str.is_empty() || path_str.starts_with("_excalidraw/") {
                        return;
                    }
                    chapter.content = embed::process(&chapter.content);
                }
            });
        }

        Ok(book)
    }

    fn supports_renderer(&self, _renderer: &str) -> Result<bool> {
        Ok(true)
    }
}

// ---------------------------------------------------------------------------
// Shared utility: fenced-code-block detection
// Used by anchors, excalidraw, and breaks modules.
// ---------------------------------------------------------------------------

pub(crate) fn detect_fence(trimmed: &str) -> (bool, char, usize) {
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

// ---------------------------------------------------------------------------
// Binary entry point
// ---------------------------------------------------------------------------

fn main() {
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
