//! A web page as text: its title, then its words, a block a line, a list's item by a dash, a
//! heading by its level; scripts, styles, and what a page's chrome holds (its navigation, its
//! header and footer, its forms) left out.

/// Elements whose words are not the page's.
const SKIPPED: [&str; 10] =
    ["script", "style", "noscript", "template", "svg", "nav", "header", "footer", "form", "iframe"];
/// Elements that stand as blocks of their own.
const BLOCKS: [&str; 22] = [
    "p",
    "div",
    "section",
    "article",
    "main",
    "aside",
    "blockquote",
    "pre",
    "table",
    "tr",
    "ul",
    "ol",
    "dl",
    "dt",
    "dd",
    "figure",
    "figcaption",
    "hr",
    "address",
    "details",
    "summary",
    "caption",
];

/// `html` as text.
pub fn read(html: &str) -> String {
    let lower = html.to_ascii_lowercase();
    let title = lower
        .find("<title")
        .and_then(|at| lower[at..].find('>').map(|end| at + end + 1))
        .and_then(|start| lower[start..].find("</title").map(|end| entities(html[start..start + end].trim())))
        .filter(|t| !t.is_empty());
    let body_at = lower.find("<body").unwrap_or(0);
    let mut out = String::new();
    let mut skip: Vec<String> = Vec::new();
    let mut i = body_at;
    let s = html.as_bytes();
    while i < s.len() {
        if s[i] == b'<' {
            if html[i..].starts_with("<!--") {
                i = html[i..].find("-->").map_or(s.len(), |e| i + e + 3);
                continue;
            }
            let end = html[i..].find('>').map_or(s.len(), |e| i + e + 1);
            let tag = &lower[i + 1..end.saturating_sub(1).max(i + 1)];
            let closing = tag.starts_with('/');
            let name: String = tag.trim_start_matches('/').chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
            i = end;
            if SKIPPED.contains(&name.as_str()) {
                if closing {
                    if let Some(at) = skip.iter().rposition(|n| *n == name) {
                        skip.truncate(at);
                    }
                } else if !tag.ends_with('/') {
                    skip.push(name);
                }
                continue;
            }
            if !skip.is_empty() {
                continue;
            }
            match name.as_str() {
                "br" => out.push('\n'),
                "li" if !closing => out.push_str("\n- "),
                "td" | "th" if !closing => out.push('\t'),
                h if h.len() == 2 && h.starts_with('h') && h.as_bytes()[1].is_ascii_digit() => {
                    if closing {
                        out.push('\n');
                    } else {
                        let level = (h.as_bytes()[1] - b'0') as usize;
                        out.push('\n');
                        out.push_str(&"#".repeat(level.clamp(1, 6)));
                        out.push(' ');
                    }
                }
                b if BLOCKS.contains(&b) => out.push('\n'),
                _ => {}
            }
            continue;
        }
        let end = html[i..].find('<').map_or(s.len(), |e| i + e);
        if skip.is_empty() {
            out.push_str(&entities(&html[i..end]));
        }
        i = end;
    }
    // Spaces as one, lines trimmed, no more than one blank line in a row.
    let mut lines: Vec<String> = Vec::new();
    for line in out.lines() {
        let words: Vec<&str> = line.split_whitespace().collect();
        let line = words.join(" ");
        if line.is_empty() && lines.last().is_none_or(|l| l.is_empty()) {
            continue;
        }
        lines.push(if line == "-" { String::new() } else { line });
    }
    let text = lines.join("\n").trim().to_string();
    match title {
        Some(t) => format!("# {t}\n\n{text}"),
        None => text,
    }
}

/// `text` with the entities a page writes most written out.
fn entities(text: &str) -> String {
    if !text.contains('&') {
        return text.to_string();
    }
    let named = [
        ("&nbsp;", " "),
        ("&amp;", "&"),
        ("&lt;", "<"),
        ("&gt;", ">"),
        ("&quot;", "\""),
        ("&apos;", "'"),
        ("&#39;", "'"),
        ("&mdash;", "\u{2014}"),
        ("&ndash;", "\u{2013}"),
        ("&hellip;", "\u{2026}"),
        ("&rsquo;", "\u{2019}"),
        ("&lsquo;", "\u{2018}"),
        ("&rdquo;", "\u{201d}"),
        ("&ldquo;", "\u{201c}"),
        ("&copy;", "\u{a9}"),
        ("&reg;", "\u{ae}"),
        ("&trade;", "\u{2122}"),
        ("&euro;", "\u{20ac}"),
        ("&pound;", "\u{a3}"),
        ("&yen;", "\u{a5}"),
        ("&times;", "\u{d7}"),
        ("&middot;", "\u{b7}"),
        ("&bull;", "\u{2022}"),
    ];
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        if let Some((entity, ch)) = named.iter().find(|(e, _)| rest.starts_with(e)) {
            out.push_str(ch);
            rest = &rest[entity.len()..];
            continue;
        }
        let numeric = rest.strip_prefix("&#").and_then(|r| r.find(';').map(|e| (&r[..e], e)));
        let decoded = numeric.and_then(|(n, e)| {
            let code = match n.strip_prefix('x').or_else(|| n.strip_prefix('X')) {
                Some(hex) => u32::from_str_radix(hex, 16).ok(),
                None => n.parse().ok(),
            };
            code.and_then(char::from_u32).map(|c| (c, e + 3))
        });
        match decoded {
            Some((c, len)) => {
                out.push(c);
                rest = &rest[len..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_reads_as_its_words() {
        let html = r#"<html><head><title>Tidewater &mdash; About</title><style>p{}</style></head>
<body><nav><a href="/">Home</a></nav><h1>Carbon-negative freight</h1>
<p>We move cargo&nbsp;and remove CO<sub>2</sub>.</p><ul><li>Four ports</li><li>38 shippers</li></ul>
<script>var x = "<p>no</p>";</script><footer>© 2026</footer></body></html>"#;
        assert_eq!(
            read(html),
            "# Tidewater \u{2014} About\n\n# Carbon-negative freight\n\nWe move cargo and remove CO2.\n\n- Four ports\n- 38 shippers"
        );
    }
}
