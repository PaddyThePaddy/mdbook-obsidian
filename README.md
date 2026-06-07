# mdbook-obsidian

An [mdBook](https://rust-lang.github.io/mdBook/) preprocessor that makes Obsidian-flavored markdown render correctly in mdBook — normalizing links, converting Obsidian syntax, generating a table of contents, and embedding Excalidraw diagrams.

> **Warning:** This repo was drafted by an AI agent. Do not use it in production.

## Table of Contents

- [Features](#features)
  - [Always-on](#always-on)
  - [Optional](#optional)
- [Installation](#installation)
- [Configuration](#configuration)
  - [Quick reference](#quick-reference)
  - [Debugging](#debugging-verbose)
  - [Automatic TOC](#automatic-toc-generate_toc)
  - [Auto heading insertion](#auto-heading-insertion-insert_heading)
  - [Hard line breaks](#hard-line-breaks-hard_line_breaks)
  - [Obsidian-flavored syntax](#obsidian-flavored-syntax-obsidian_syntax)
  - [Backlinks](#backlinks-backlinks)
  - [Image lightbox](#image-lightbox-lightbox)
- [Roadmap](#roadmap)
- [License](#license)

## Features

### Always-on

These run automatically with no configuration required.

**Internal link normalization** — Obsidian encodes spaces as `%20` and preserves original casing; mdBook generates lowercase, hyphenated heading IDs. The preprocessor aligns anchor fragments so in-page links resolve correctly:

| Before | After |
|--------|-------|
| `[Section](#Grade%201%20-%20Color)` | `[Section](#grade-1---color)` |
| `[Link](Note.md#Grade%201)` | `[Link](Note.md#grade-1)` |

Image links, external URLs, links inside fenced code blocks, and inline code spans are left untouched — only `#fragment` portions are normalized.

**Excalidraw viewer pages** — Links and wikilinks pointing to `.excalidraw` files are automatically converted into navigable viewer pages. The viewer HTML is compiled into the binary; no extra files are needed.

| Markdown source | Result |
|---|---|
| `![[My Drawing.excalidraw]]` | Link to viewer page |
| `[[My Drawing.excalidraw\|See diagram]]` | Link with custom text |
| `[diagram](My%20Drawing.excalidraw)` | Link to viewer page |

The viewer loads React and Excalidraw from a CDN, renders the drawing in read-only mode, and inherits mdBook's active light/dark theme. The published book must be reachable from the internet for the browser to fetch the CDN scripts.

---

### Optional

These are disabled by default and enabled per feature flag in `book.toml`.

| Feature | Option | Default |
|---------|--------|---------|
| Automatic TOC generation | `generate_toc` | `false` |
| Auto heading insertion | `insert_heading` | `false` |
| Hard line breaks | `hard_line_breaks` | `false` |
| Obsidian-flavored syntax | `obsidian_syntax` | `false` |
| Backlinks | `backlinks` | `false` |
| Image lightbox | `lightbox` | `false` |

**Automatic TOC generation** — Scans `src/` and adds any markdown file not already listed in `SUMMARY.md` as a navigable chapter. The folder hierarchy is reflected in the sidebar. Subdirectories with an `index.md` or `README.md` become clickable section headers; otherwise the section header is non-clickable.

**Auto heading insertion** — When a chapter's content does not begin with a top-level `# Heading`, one is inserted using the chapter's file name. Excalidraw pages and draft section headers are not affected.

**Hard line breaks** — Converts every single newline between non-empty lines to a hard break (`<br>`). Blank lines, lines already ending with two spaces or `\`, and fenced code block content are untouched.

**Obsidian-flavored syntax** — Converts Obsidian-specific markdown to standard HTML:

**Backlinks** — Appends a `## Backlinks` section to each chapter listing every other chapter that links to it. Both standard markdown links (`[text](page.md)`) and wikilinks (`[[Page Name]]`) are detected. Links inside fenced code blocks are ignored.

**Image lightbox** — Clicking or tapping any image opens it in a full-screen overlay. Supports drag-to-pan, scroll-wheel zoom (desktop), pinch-to-zoom (touch), double-tap to reset, and Escape / backdrop click to close. Only injected on pages that actually contain images. Both standard markdown links (`[text](page.md)`) and wikilinks (`[[Page Name]]`) are detected. Links inside fenced code blocks are ignored.

| Syntax | Result |
|--------|--------|
| `%%hidden text%%` | Removed (inline or multi-line) |
| `==highlighted==` | `<mark>highlighted</mark>` |
| `[[Note Name]]` | `[Note Name](Note%20Name.md)` |
| `[[Note Name\|Display]]` | `[Display](Note%20Name.md)` |
| `[[Note Name#Heading]]` | `[Note Name](Note%20Name.md#heading)` |
| `> [!note] Title` | Styled callout block |

---

## Installation

### From source

```sh
cargo install --path . --locked
```

---

## Configuration

Add the preprocessor to `book.toml` to activate it:

```toml
[preprocessor.obsidian]
```

All options are optional. Below is a full reference followed by per-feature details.

### Quick reference

```toml
[preprocessor.obsidian]
verbose          = false        # print every link transformation to stderr

generate_toc     = false        # auto-discover uncovered .md files
toc_ignore_file  = ".mdignore"  # extra ignore file (gitignore syntax)
toc_sort         = "none"       # "none" | "alpha" | "modified"
toc_dirs_first   = false        # list directories before files at each level

insert_heading   = false        # insert # Heading when chapter has none
hard_line_breaks = false        # treat single newlines as <br>
obsidian_syntax  = false        # convert Obsidian-flavored markdown
backlinks        = false        # append a Backlinks section to each chapter
lightbox         = false        # tap/click images to zoom and pan
```

---

### Debugging (`verbose`)

```toml
[preprocessor.obsidian]
verbose = true
```

Prints one line per transformed link to stderr:

```
[mdbook-obsidian] link: #Grade%201%20-%20Color  =>  #grade-1---color
```

Build and filter:

```sh
mdbook build 2>&1 | grep mdbook-obsidian
```

---

### Automatic TOC (`generate_toc`)

```toml
[preprocessor.obsidian]
generate_toc = true
```

Files matched by `.gitignore` at any level are always excluded. To add book-specific ignore patterns, create an ignore file (gitignore syntax) and reference it:

```toml
toc_ignore_file = ".mdignore"
```

Example `.mdignore`:

```
*-draft.md
private/
```

**Insertion point** — Place `<!-- mdbook-obsidian toc -->` in `SUMMARY.md` to control where auto-generated chapters appear:

```markdown
- [Introduction](intro.md)

<!-- mdbook-obsidian toc -->

- [Changelog](changelog.md)
```

Without the placeholder, discovered chapters are appended at the end.

**Inline TOC list** — The same placeholder inside a chapter file is replaced with a nested markdown list of all auto-discovered chapters, useful for index pages:

```markdown
## Notes

<!-- mdbook-obsidian toc -->
```

**Sorting**

```toml
toc_sort      = "alpha"   # "none" (filesystem order) | "alpha" | "modified"
toc_dirs_first = true     # directories before files at each level
```

`modified` sorts by file modification time (oldest first). For directories, the directory's own mtime is used.

---

### Auto heading insertion (`insert_heading`)

```toml
[preprocessor.obsidian]
insert_heading = true
```

Inserts `# <filename>` at the top of any chapter that does not already start with a level-1 heading.

---

### Hard line breaks (`hard_line_breaks`)

```toml
[preprocessor.obsidian]
hard_line_breaks = true
```

Treats every single newline between non-empty lines as a hard break. Useful when your Obsidian notes rely on single-newline line breaks that CommonMark otherwise ignores.

---

### Obsidian-flavored syntax (`obsidian_syntax`)

```toml
[preprocessor.obsidian]
obsidian_syntax = true
```

**Comments** (`%%...%%`) are removed, including multi-line spans. Content inside fenced code blocks is never touched.

**Highlights** (`==text==`) become `<mark>text</mark>`. Triple-equals (`===`) and inline code spans are left untouched.

**Wikilinks** (`[[...]]`) become regular markdown links. Embeds (`![[...]]`) and Excalidraw wikilinks are left unchanged for other passes to handle.

**Callouts** (`> [!type]`) are converted to styled HTML blocks. Supported types and aliases:

| Type | Aliases |
|------|---------|
| `note` | — |
| `abstract` | `summary`, `tldr` |
| `info`, `todo` | — |
| `tip` | `hint`, `important` |
| `success` | `check`, `done` |
| `question` | `help`, `faq` |
| `warning` | `caution`, `attention` |
| `failure` | `fail`, `missing` |
| `danger` | `error` |
| `bug`, `example`, `quote` | (`quote` alias: `cite`) |

Append `+` for an expanded foldable callout or `-` for collapsed:

```markdown
> [!tip]+ Expanded by default
> Content here

> [!warning]- Click to expand
> This is hidden initially
```

Foldable callouts use the native `<details>`/`<summary>` elements and require no JavaScript. A small `<style>` block is injected into pages that contain callouts; override it with your own `additional-css` in `book.toml`.

---

### Backlinks (`backlinks`)

```toml
[preprocessor.obsidian]
backlinks = true
```

Appends a `## Backlinks` section at the bottom of each chapter that is referenced by at least one other chapter. The section contains a list of links back to those chapters, sorted alphabetically.

Both link formats are detected:

- Standard markdown links: `[text](other-page.md)`
- Wikilinks: `[[Other Page]]` (when `obsidian_syntax` is disabled; otherwise they are already converted to markdown links before this pass runs)

Links inside fenced code blocks are ignored. Excalidraw viewer pages are excluded from both collection and injection. If the same chapter links to a page multiple times, it appears only once in the backlinks list.

---

### Image lightbox (`lightbox`)

```toml
[preprocessor.obsidian]
lightbox = true
```

Adds a full-screen image viewer to any chapter that contains at least one image. No external libraries or extra files are required — the CSS and JavaScript are compiled into the binary and injected only on pages that need them.

**Controls:**

| Action | Behaviour |
|--------|-----------|
| Click / tap image | Open lightbox |
| Drag | Pan while zoomed |
| Scroll wheel | Zoom in / out (desktop) |
| Pinch | Zoom in / out (touch) |
| Double-click / double-tap | Reset zoom and position |
| Click backdrop or `×` button | Close |
| Escape | Close |

---

## Roadmap

- [ ] Tag stripping or conversion
- [ ] Excalidraw: self-hosted CDN fallback for air-gapped deployments
- [ ] Excalidraw: collision handling when two files share the same stem

## License

MIT
