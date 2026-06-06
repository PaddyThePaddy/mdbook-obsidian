# mdbook-obsidian

An [mdBook](https://rust-lang.github.io/mdBook/) preprocessor that transforms [Obsidian](https://obsidian.md/)-style markdown syntax so it renders correctly in mdBook.

**Warning: This repo is completed drafted by AI agent. Do not use it in production.**

## Features

### Internal link normalization

Obsidian encodes spaces in link paths as `%20` and preserves the original file casing. mdBook expects lowercase, hyphen-separated file names. This preprocessor converts links automatically:

| Before | After |
|--------|-------|
| `[My Note](A%20file-name%20with%20space.md)` | `[My Note](a-file-name-with-space.md)` |
| `[Link](MyNote.md)` | `[Link](mynote.md)` |
| `[Section](My%20Note.md#section-title)` | `[Section](my-note.md#section-title)` |

The transformation: URL-decode percent-encoded characters → lowercase → spaces become hyphens.

External URLs (`https://…`, `mailto:…`) and same-page anchors (`#heading`) are left unchanged.

Links inside fenced code blocks and inline code spans are also left unchanged.

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

- [ ] Wikilink conversion: `[[Note Name]]` → `[Note Name](note-name.md)`
- [ ] Wikilink aliases: `[[Note Name|display text]]`
- [ ] Obsidian callout/admonition blocks
- [ ] Tag stripping or conversion

## License

MIT
