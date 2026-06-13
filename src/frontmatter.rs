use std::fmt::Write as FmtWrite;

const STYLE: &str = r#"<style>
.ob-properties{display:flex;flex-direction:column;gap:.3em;padding:.65em 1em;margin-bottom:1.4em;border-radius:6px;background:rgba(128,128,128,.08);border:1px solid rgba(128,128,128,.2);font-size:.9em;line-height:1.5}
.ob-prop-row{display:flex;align-items:center;flex-wrap:wrap;gap:.35em}
.ob-prop-label{font-size:.78em;opacity:.5;min-width:4.5em;flex-shrink:0;font-weight:500}
.ob-tag{display:inline-flex;align-items:center;padding:.1em .55em;border-radius:999px;background:rgba(100,149,237,.18);color:var(--links,cornflowerblue);text-decoration:none;font-size:.82em;cursor:pointer;border:none;font-family:inherit;transition:opacity .15s}
.ob-tag:hover{opacity:.7;text-decoration:none}
.ob-tag::before{content:'#';opacity:.5;margin-right:.08em}
.ob-alias{display:inline-block;padding:.1em .5em;border-radius:4px;background:rgba(128,128,128,.12);font-size:.82em;font-style:italic}
</style>
"#;

const SCRIPT: &str = r#"<script>
function obTagSearch(t){var s=document.getElementById('searchbar');if(s){s.value=t;s.dispatchEvent(new Event('input',{bubbles:true}));s.focus();window.scrollTo(0,0);}}
</script>
"#;

pub(crate) fn process(content: &str) -> String {
    let Some((fm, rest)) = extract_frontmatter(content) else {
        return content.to_string();
    };

    let tags = parse_list(fm, "tags");
    let aliases = parse_list(fm, "aliases");

    let rest = rest.trim_start_matches('\n');

    if tags.is_empty() && aliases.is_empty() {
        return rest.to_string();
    }

    let mut out = String::with_capacity(STYLE.len() + SCRIPT.len() + rest.len() + 256);
    out.push_str(STYLE);
    out.push_str(SCRIPT);
    out.push_str("<div class=\"ob-properties\">\n");

    if !aliases.is_empty() {
        out.push_str("<div class=\"ob-prop-row\"><span class=\"ob-prop-label\">Aliases</span>");
        for alias in &aliases {
            write!(out, "<span class=\"ob-alias\">{}</span>", escape_html(alias)).unwrap();
        }
        out.push_str("</div>\n");
    }

    if !tags.is_empty() {
        out.push_str("<div class=\"ob-prop-row\"><span class=\"ob-prop-label\">Tags</span>");
        for tag in &tags {
            write!(
                out,
                "<button class=\"ob-tag\" onclick=\"obTagSearch({});\">{}</button>",
                js_str(tag),
                escape_html(tag)
            )
            .unwrap();
        }
        out.push_str("</div>\n");
    }

    out.push_str("</div>\n\n");
    out.push_str(rest);
    out
}

fn extract_frontmatter(content: &str) -> Option<(&str, &str)> {
    let body = content
        .strip_prefix("---\r\n")
        .or_else(|| content.strip_prefix("---\n"))?;

    if let Some(pos) = body.find("\n---\r\n") {
        return Some((&body[..pos], &body[pos + 6..]));
    }
    if let Some(pos) = body.find("\n---\n") {
        return Some((&body[..pos], &body[pos + 5..]));
    }
    // frontmatter at end of file
    if body.ends_with("\n---") {
        let pos = body.len() - 4;
        return Some((&body[..pos], ""));
    }
    None
}

fn parse_list(yaml: &str, key: &str) -> Vec<String> {
    let search = format!("{}:", key);
    let lines: Vec<&str> = yaml.lines().collect();

    for (i, &line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if !trimmed.starts_with(search.as_str()) {
            continue;
        }
        let value = trimmed[search.len()..].trim();
        if value.starts_with('[') {
            return parse_inline_array(value);
        } else if !value.is_empty() {
            return vec![normalize_tag(value)];
        } else {
            // Block list: subsequent lines starting with "- "
            let mut result = Vec::new();
            for &next in &lines[i + 1..] {
                let t = next.trim();
                if let Some(item) = t.strip_prefix("- ") {
                    result.push(normalize_tag(item.trim()));
                } else if t.is_empty() {
                    continue;
                } else {
                    break;
                }
            }
            return result;
        }
    }
    vec![]
}

fn parse_inline_array(s: &str) -> Vec<String> {
    let s = s.trim();
    let inner = s
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(s);
    inner
        .split(',')
        .map(|t| normalize_tag(t.trim()))
        .filter(|t| !t.is_empty())
        .collect()
}

// Strip optional leading '#' (older Obsidian inline-tag style) and quotes.
fn normalize_tag(s: &str) -> String {
    unquote(s).trim_start_matches('#').to_string()
}

fn unquote(s: &str) -> &str {
    let s = s.trim();
    if s.len() >= 2
        && ((s.starts_with('"') && s.ends_with('"'))
            || (s.starts_with('\'') && s.ends_with('\'')))
    {
        return &s[1..s.len() - 1];
    }
    s
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn js_str(s: &str) -> String {
    let escaped = s
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "");
    format!("\"{}\"", escaped)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_frontmatter_no_properties() {
        let input = "---\ntitle: My Note\ndate: 2024-01-01\n---\n\n# Content\n";
        assert_eq!(process(input), "# Content\n");
    }

    #[test]
    fn extracts_inline_array_tags() {
        let input = "---\ntags: [rust, mdbook]\n---\n\nBody\n";
        let out = process(input);
        assert!(out.contains("ob-tag"));
        assert!(out.contains("rust"));
        assert!(out.contains("mdbook"));
        assert!(out.contains("Body"));
    }

    #[test]
    fn extracts_block_list_tags() {
        let input = "---\ntags:\n  - rust\n  - mdbook\n---\n\nBody\n";
        let out = process(input);
        assert!(out.contains("rust"));
        assert!(out.contains("mdbook"));
    }

    #[test]
    fn strips_hash_prefix_from_tags() {
        let input = "---\ntags: [#rust, #mdbook]\n---\n\nBody\n";
        let out = process(input);
        // Should show "rust" not "#rust" (# comes from CSS ::before)
        assert!(out.contains(">rust<"));
        assert!(!out.contains(">#rust<"));
    }

    #[test]
    fn extracts_aliases() {
        let input = "---\naliases: [My Note, Note]\n---\n\nBody\n";
        let out = process(input);
        assert!(out.contains("ob-alias"));
        assert!(out.contains("My Note"));
    }

    #[test]
    fn no_frontmatter_unchanged() {
        let input = "# Heading\n\nParagraph\n";
        assert_eq!(process(input), input);
    }

    #[test]
    fn tag_onclick_calls_search() {
        let input = "---\ntags: [rust]\n---\n\nBody\n";
        let out = process(input);
        assert!(out.contains("obTagSearch"));
        assert!(out.contains("\"rust\""));
    }

    #[test]
    fn tag_with_special_html_chars() {
        let input = "---\ntags: [\"a<b>\"]\n---\n\nBody\n";
        let out = process(input);
        assert!(out.contains("a&lt;b&gt;"));
    }
}
