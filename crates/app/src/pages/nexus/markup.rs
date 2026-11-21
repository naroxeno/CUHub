//! Maps display-only Nexus formatting to native TextView input.

pub(super) fn description_html(source: &str) -> String {
    let mut html = String::new();
    let mut rest = source;
    while let Some(first) = rest.chars().next() {
        if let Some((tag, consumed)) = formatted_tag(rest) {
            html.push_str(tag);
            rest = &rest[consumed..];
            if tag == "<br>" {
                rest = skip_newline(rest);
            }
            continue;
        }
        match first {
            '<' => html.push_str("&lt;"),
            '>' => html.push_str("&gt;"),
            '\n' if formatted_tag(rest[1..].trim_start_matches([' ', '\t']))
                .is_some_and(|(tag, _)| tag == "<br>") => {}
            '\n' => html.push_str("<br>"),
            '\r' => {}
            _ => html.push(first),
        }
        rest = &rest[first.len_utf8()..];
    }
    html
}

fn skip_newline(rest: &str) -> &str {
    rest.strip_prefix("\r\n")
        .or_else(|| rest.strip_prefix('\n'))
        .unwrap_or(rest)
}

fn formatted_tag(rest: &str) -> Option<(&'static str, usize)> {
    let closing = match rest.as_bytes().first()? {
        b'[' => ']',
        b'<' => '>',
        _ => return None,
    };
    let end = rest
        .get(..rest.floor_char_boundary(rest.len().min(512)))?
        .find(closing)?;
    let raw = rest[1..end].trim().trim_end_matches('/').trim();
    let name = raw.split(['=', ' ', '\t']).next()?.to_ascii_lowercase();
    let tag = match name.as_str() {
        "br" => "<br>",
        "b" | "strong" => "<strong>",
        "/b" | "/strong" => "</strong>",
        "i" | "em" => "<em>",
        "/i" | "/em" => "</em>",
        "u" => "<u>",
        "/u" => "</u>",
        "s" | "del" => "<del>",
        "/s" | "/del" => "</del>",
        "heading" | "h1" | "h2" | "h3" => "<h3>",
        "/heading" | "/h1" | "/h2" | "/h3" => "</h3>",
        "p" => "<p>",
        "/p" => "</p>",
        "list" | "ul" => "<ul>",
        "ol" => "<ol>",
        "/list" | "/ul" => "</li></ul>",
        "/ol" => "</li></ol>",
        "*" | "li" => "<li>",
        "/li" | "/*" => "</li>",
        "quote" | "blockquote" => "<blockquote>",
        "/quote" | "/blockquote" => "</blockquote>",
        "code" => "<code>",
        "/code" => "</code>",
        "table" => "<table>",
        "/table" => "</table>",
        "tr" => "<tr>",
        "/tr" => "</tr>",
        "th" => "<th>",
        "/th" => "</th>",
        "td" => "<td>",
        "/td" => "</td>",
        "line" | "hr" => "<hr>",
        "size" | "/size" | "color" | "/color" | "font" | "/font" | "left" | "/left" | "right"
        | "/right" | "center" | "/center" | "url" | "/url" | "a" | "/a" | "img" | "/img"
        | "spoiler" | "/spoiler" => "",
        _ => return None,
    };
    Some((tag, end + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_supported_tags_without_active_content_or_remote_images() {
        assert_eq!(
            description_html(
                "[b]Bold[/b]\n<br />[list][*]Item[/list]<img src=\"file:///private\"><script>alert(1)</script>"
            ),
            "<strong>Bold</strong><br><ul><li>Item</li></ul>&lt;script&gt;alert(1)&lt;/script&gt;",
        );
    }
}
