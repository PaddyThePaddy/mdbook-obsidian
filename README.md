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

## Roadmap

- [ ] Wikilink conversion for regular notes: `[[Note Name]]` → `[Note Name](note-name.md)`
- [ ] Obsidian callout/admonition blocks
- [ ] Tag stripping or conversion
- [ ] Excalidraw: self-hosted CDN fallback for air-gapped deployments
- [ ] Excalidraw: collision handling when two files share the same stem

## License

MIT
