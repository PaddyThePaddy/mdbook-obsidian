use regex::Regex;

const EMBED_CSS: &str = "<style>
.yt-embed { margin: 1em auto; }
.yt-embed iframe { width: 100%; aspect-ratio: 16/9; border: none; display: block; }
</style>
";

fn link_re() -> &'static Regex {
    use std::sync::OnceLock;
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(!?)\[([^\]]*)\]\(([^)]+)\)").unwrap())
}

pub(crate) fn process(content: &str) -> String {
    if !has_youtube(content) {
        return content.to_string();
    }

    let link_re = link_re();

    let lines: Vec<&str> = content.split('\n').collect();
    let n = lines.len();
    let mut result = String::with_capacity(content.len() + 256);
    let mut in_code_block = false;
    let mut fence: Option<(char, usize)> = None;
    let mut had_embed = false;

    for (li, &line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        let (is_fence, fc, flen) = crate::detect_fence(trimmed);

        if is_fence {
            if !in_code_block {
                in_code_block = true;
                fence = Some((fc, flen));
            } else if let Some((f, c)) = fence {
                if fc == f && flen >= c {
                    in_code_block = false;
                    fence = None;
                }
            }
            result.push_str(line);
        } else if in_code_block {
            result.push_str(line);
        } else {
            let clean = line.trim();

            if let Some((url_line, embed_html)) = check_embed_line(clean, &link_re) {
                had_embed = true;
                result.push_str("\n\n");
                result.push_str(&url_line);
                result.push_str("\n\n");
                result.push_str(&embed_html);
                result.push_str("\n\n");
            } else {
                result.push_str(line);
            }
        }

        if li < n - 1 {
            result.push('\n');
        }
    }

    if had_embed {
        result.push('\n');
        result.push_str(EMBED_CSS);
    }

    result
}

/// Returns `(url_link_line, embed_html)` when the line is a standalone YouTube link,
/// in any of these forms:
///   - bare URL:           `https://youtu.be/ID`
///   - markdown link:      `[text](https://youtu.be/ID)`
///   - image embed syntax: `![alt](https://youtu.be/ID)`
fn check_embed_line(content: &str, link_re: &Regex) -> Option<(String, String)> {
    // Bare YouTube URL.
    if let Some(id) = extract_youtube_id(content) {
        let url_line = format!("[{}]({})", content, content);
        return Some((url_line, make_embed(id)));
    }

    // Markdown link [text](url) or Obsidian image embed ![alt](url).
    if let Some(caps) = link_re.captures(content) {
        let mat = caps.get(0).unwrap();
        if mat.start() == 0 && mat.end() == content.len() {
            let url = caps.get(3).map_or("", |m| m.as_str());
            if let Some(id) = extract_youtube_id(url) {
                let url_line = format!("[{}]({})", url, url);
                return Some((url_line, make_embed(id)));
            }
        }
    }

    None
}

fn extract_youtube_id(url: &str) -> Option<&str> {
    // Short URL: https://youtu.be/VIDEO_ID
    for prefix in &["https://youtu.be/", "http://youtu.be/"] {
        if let Some(rest) = url.strip_prefix(prefix) {
            let id = rest.split(|c: char| c == '?' || c == '#' || c == '&').next()?;
            if !id.is_empty() {
                return Some(id);
            }
        }
    }
    // Standard URL: https://[www.]youtube.com/watch?v=VIDEO_ID
    if url.contains("youtube.com") {
        if let Some(pos) = url.find("v=") {
            let after = &url[pos + 2..];
            let id = after.split(|c: char| c == '&' || c == '#').next()?;
            if !id.is_empty() {
                return Some(id);
            }
        }
    }
    None
}

fn make_embed(video_id: &str) -> String {
    format!(
        "<div class=\"yt-embed\"><iframe \
         src=\"https://www.youtube.com/embed/{video_id}\" \
         frameborder=\"0\" \
         allow=\"accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture\" \
         allowfullscreen loading=\"lazy\"></iframe></div>"
    )
}

fn has_youtube(content: &str) -> bool {
    content.contains("youtu.be/") || content.contains("youtube.com/watch")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embeds_youtu_be_url() {
        let out = process("https://youtu.be/dQw4w9WgXcQ");
        assert!(out.contains("youtube.com/embed/dQw4w9WgXcQ"));
        assert!(out.contains("<iframe"));
        assert!(out.contains(".yt-embed"));
        // URL shown as clickable link above player
        assert!(out.contains("[https://youtu.be/dQw4w9WgXcQ](https://youtu.be/dQw4w9WgXcQ)"));
    }

    #[test]
    fn embeds_youtube_watch_url() {
        let out = process("https://www.youtube.com/watch?v=dQw4w9WgXcQ");
        assert!(out.contains("youtube.com/embed/dQw4w9WgXcQ"));
        assert!(out.contains("[https://www.youtube.com/watch?v=dQw4w9WgXcQ]"));
    }

    #[test]
    fn embeds_autolinked_form() {
        // After the autolink pass, bare URLs are wrapped as [url](url).
        let out = process("[https://youtu.be/abc123](https://youtu.be/abc123)");
        assert!(out.contains("youtube.com/embed/abc123"));
        assert!(out.contains("[https://youtu.be/abc123](https://youtu.be/abc123)"));
    }

    #[test]
    fn embeds_image_embed_syntax() {
        // Obsidian-style image embed with a YouTube URL.
        let out = process("![](https://youtu.be/abc123)");
        assert!(out.contains("youtube.com/embed/abc123"));
        assert!(out.contains("[https://youtu.be/abc123](https://youtu.be/abc123)"));
    }

    #[test]
    fn embeds_named_link() {
        // Named link: display text is discarded, raw URL is shown above player.
        let out = process("[Watch this video](https://youtu.be/abc123)");
        assert!(out.contains("youtube.com/embed/abc123"));
        assert!(out.contains("[https://youtu.be/abc123](https://youtu.be/abc123)"));
    }

    #[test]
    fn does_not_embed_url_with_surrounding_text() {
        let out = process("Watch https://youtu.be/abc here");
        assert!(!out.contains("<iframe"));
    }

    #[test]
    fn skips_youtube_in_fenced_code() {
        let input = "```\nhttps://youtu.be/abc\n```";
        let out = process(input);
        assert!(!out.contains("<iframe"));
    }

    #[test]
    fn no_change_without_youtube() {
        let input = "Just some text\nwith no video links.";
        assert_eq!(process(input), input);
    }
}
