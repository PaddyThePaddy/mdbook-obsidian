use regex::Regex;

const EMBED_CSS: &str = "<style>
.yt-embed { margin: 1em auto; }
.yt-embed iframe { width: 100%; aspect-ratio: 16/9; border: none; display: block; }
</style>
";

pub(crate) fn process(content: &str) -> String {
    if !has_youtube(content) {
        return content.to_string();
    }

    let link_re = Regex::new(r"(!?)\[([^\]]*)\]\(([^)]+)\)").expect("valid regex");

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
            // Trim trailing whitespace (including hard-break `  ` markers) for detection.
            let clean = line.trim();

            if let Some(embed_html) = check_embed_line(clean, &link_re) {
                had_embed = true;
                result.push_str("\n\n");
                result.push_str(clean);
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

fn check_embed_line(content: &str, link_re: &Regex) -> Option<String> {
    // Bare YouTube URL on the line.
    if let Some(id) = extract_youtube_id(content) {
        return Some(make_embed(id));
    }

    // Single markdown link whose URL is a YouTube URL.
    if let Some(caps) = link_re.captures(content) {
        let mat = caps.get(0).unwrap();
        if mat.start() == 0 && mat.end() == content.len() {
            let bang = caps.get(1).map_or("", |m| m.as_str());
            if bang == "!" {
                return None;
            }
            let url = caps.get(3).map_or("", |m| m.as_str());
            if let Some(id) = extract_youtube_id(url) {
                return Some(make_embed(id));
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
    }

    #[test]
    fn embeds_youtube_watch_url() {
        let out = process("https://www.youtube.com/watch?v=dQw4w9WgXcQ");
        assert!(out.contains("youtube.com/embed/dQw4w9WgXcQ"));
    }

    #[test]
    fn embeds_autolinked_form() {
        // After the autolink pass, bare URLs are wrapped as [url](url).
        let out = process("[https://youtu.be/abc123](https://youtu.be/abc123)");
        assert!(out.contains("youtube.com/embed/abc123"));
    }

    #[test]
    fn does_not_embed_url_with_surrounding_text() {
        // Only a standalone URL line should be embedded.
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
