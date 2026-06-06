# mdbook-obsidian

An [mdBook](https://rust-lang.github.io/mdBook/) preprocessor that transforms [Obsidian](https://obsidian.md/)-style markdown syntax so it renders correctly in mdBook.

**Warning: This repo is completed drafted by AI agent. Do not use it in production.**

## Features

### Internal link normalization

Obsidian encodes spaces in link paths as `%20` and preserves the original file casing. mdBook generates heading IDs that are lowercase and hyphenated. This preprocessor aligns anchor fragments so in-page links resolve correctly:

| Before | After |
|--------|-------|
| `[Section](#Grade%201%20-%20Color)` | `[Section](#grade-1---color)` |
| `[Link](Note.md#Grade%201)` | `[Link](Note.md#grade-1)` |

Image links and file paths are left untouched — only `#fragment` portions are normalized. External URLs, links inside fenced code blocks, and links inside inline code spans are also unchanged.

### Excalidraw viewer pages

Links and wikilinks that point to `.excalidraw` files are automatically converted into navigable viewer pages. No extra files need to be added to your book — the viewer HTML is compiled into the preprocessor binary.

**Supported link formats:**

| Markdown source | Result |
|---|---|
| `![[My Drawing.excalidraw]]` | Link to viewer page |
| `[[My Drawing.excalidraw\|See diagram]]` | Link with custom text |
| `[diagram](My%20Drawing.excalidraw)` | Link to viewer page |

The viewer page loads [React](https://react.dev/) and [@excalidraw/excalidraw](https://www.npmjs.com/package/@excalidraw/excalidraw) from a CDN and renders the drawing in read-only mode inside mdBook's normal page chrome (sidebar and navigation are preserved). The theme (light/dark) is read from mdBook's active theme automatically.

**Requirements:** the published book must be accessible from the internet so the browser can reach the CDN scripts.

## Installation

### From source
`
```sh
cargo install --path .
```

### From crates.io (once published)

```sh
cargo install mdbook-obsidian
```

## Configuration

Add the preprocessor to your `book.toml`:

```toml
[preprocessor.obsidian]
```

That is all that is required. Run `mdbook build` as usual.

### Verbose / debug logging

Set `verbose = true` to print every link transformation to stderr during the build:

```toml
[preprocessor.obsidian]
verbose = true
```

Then build and filter the output:

```sh
mdbook build 2>&1 | grep mdbook-obsidian
```

Each transformed link prints one line showing the before and after:

```
[mdbook-obsidian] link: #Grade%201%20-%20Color  =>  #grade-1---color
```

Links that are unchanged (external URLs, image links, already-normalized anchors) produce no output.

## Workflow

1. Write notes in Obsidian.
2. Copy or symlink the note files into your mdBook `src/` directory.
3. Build with `mdbook build` — the preprocessor normalizes every internal link before rendering.

### Automatic TOC generation

When enabled, the preprocessor scans the `src/` directory and automatically adds any markdown files that are **not listed in `SUMMARY.md`** as navigable book chapters. The file tree is reflected in the sidebar as a nested hierarchy.

Enable it in `book.toml`:

```toml
[preprocessor.obsidian]
generate_toc = true
```

**Controlling which files are included:**

Files matched by `.gitignore` (at any level) are always excluded. To add extra ignore patterns specific to the book, create an ignore file and reference it:

```toml
[preprocessor.obsidian]
generate_toc = true
toc_ignore_file = ".mdignore"   # name of your extra ignore file
```

The ignore file uses the same syntax as `.gitignore`. For example, to exclude draft files:

```
*-draft.md
private/
```

**Directory structure:**

- Files at the root of `src/` become top-level chapters.
- Subdirectories become nested sections in the sidebar, preserving the folder hierarchy. If a directory contains `index.md` or `README.md`, that file is used as the clickable section header; otherwise the section header is non-clickable (a draft chapter).
- `.excalidraw.md` files found during the scan are included as Excalidraw viewer pages at the correct position in the hierarchy — they are not listed twice even if also linked from other chapters.

**Sorting the generated TOC:**

By default, entries appear in the order the filesystem returns them. Use `toc_sort` to change this:

```toml
[preprocessor.obsidian]
generate_toc = true
toc_sort = "alpha"      # "none" (filesystem order, default), "alpha" (alphabetical), "modified" (oldest mtime first)
toc_dirs_first = true   # list subdirectory sections before files at each level (default: false)
```

`toc_sort = "modified"` sorts files by their modification timestamp. For directories, the directory's own mtime is used (which changes when files are directly added or removed from it).

**Controlling insertion order:**

Place the placeholder comment `<!-- mdbook-obsidian toc -->` inside `SUMMARY.md` to control where the auto-generated chapters appear relative to your manually listed chapters:

```markdown
- [Introduction](intro.md)
- [Setup](setup.md)

<!-- mdbook-obsidian toc -->

- [Changelog](changelog.md)
```

Auto-discovered chapters are inserted at the placeholder's position. If the placeholder is absent, discovered chapters are appended at the end.

**Inline TOC list:**

The same placeholder can also be placed inside any regular chapter file. There it is replaced with a nested markdown list of all auto-discovered chapters, useful for a landing page or index:

```markdown
## Auto-discovered notes

<!-- mdbook-obsidian toc -->
```

### Auto heading insertion

When a chapter's content does not begin with a top-level heading (`# …`), the preprocessor inserts one derived from the chapter's file name. Enable it in `book.toml`:

```toml
[preprocessor.obsidian]
insert_heading = true
```

Excalidraw viewer pages and draft section headers (directories without an index file) are not affected.

### Hard line breaks

Obsidian users often write with single newlines expecting visible line breaks. Standard CommonMark only creates a hard break when a line ends with two or more spaces. Enable this conversion with:

```toml
[preprocessor.obsidian]
hard_line_breaks = true
```

With this setting, every single newline between two non-empty lines is treated as a hard break (`<br>`). Blank lines (paragraph separators), lines already ending with `  ` or `\`, and content inside fenced code blocks are left untouched.

## Roadmap

- [ ] Wikilink conversion for regular notes: `[[Note Name]]` → `[Note Name](note-name.md)`
- [ ] Obsidian callout/admonition blocks
- [ ] Tag stripping or conversion
- [ ] Excalidraw: self-hosted CDN fallback for air-gapped deployments
- [ ] Excalidraw: collision handling when two files share the same stem

## License

MIT
