//! Jira's wiki markup (Jira Server's and Data Center's text: a description, a comment) as markdown, which the app
//! draws: headings, bold and italic, `{{monospace}}`, links, lists, `{code}` blocks, quotes, tables and rules. What
//! has no markdown (a colour, underline) keeps its text.

/// `wiki` as markdown.
pub fn to_markdown(wiki: &str) -> String {
    let wiki = wiki.replace("\r\n", "\n");
    let mut out: Vec<String> = Vec::new();
    // Inside `{code}` or `{noformat}`: what closes it.
    let mut code: Option<&str> = None;
    let mut quote = false;
    // The last line was a list's item, a table's row.
    let (mut listed, mut table) = (false, false);
    let blank = |out: &mut Vec<String>| {
        if out.last().is_some_and(|l| !l.is_empty()) {
            out.push(String::new());
        }
    };
    for line in wiki.lines() {
        if let Some(close) = code {
            match line.find(close) {
                Some(at) => {
                    if !line[..at].trim().is_empty() {
                        out.push(line[..at].to_owned());
                    }
                    out.push("```".into());
                    out.push(String::new());
                    code = None;
                }
                None => out.push(line.to_owned()),
            }
            continue;
        }
        let trimmed = line.trim();
        if let Some((close, lang, rest)) = code_start(trimmed) {
            blank(&mut out);
            out.push(format!("```{lang}"));
            match rest.find(close) {
                // `{code}one line{code}`.
                Some(at) => {
                    if !rest[..at].trim().is_empty() {
                        out.push(rest[..at].to_owned());
                    }
                    out.push("```".into());
                    out.push(String::new());
                }
                None => {
                    if !rest.trim().is_empty() {
                        out.push(rest.to_owned());
                    }
                    code = Some(close);
                }
            }
            (listed, table) = (false, false);
            continue;
        }
        if trimmed == "{quote}" || trimmed.starts_with("{quote:") {
            quote = !quote;
            blank(&mut out);
            (listed, table) = (false, false);
            continue;
        }
        let lead = if quote { "> " } else { "" };
        if trimmed.is_empty() {
            blank(&mut out);
            (listed, table) = (false, false);
            continue;
        }
        if let Some((level, text)) = heading(trimmed) {
            blank(&mut out);
            out.push(format!("{lead}{} {}", "#".repeat(level), inline(text)));
            out.push(String::new());
            (listed, table) = (false, false);
            continue;
        }
        if let Some(text) = trimmed.strip_prefix("bq. ") {
            blank(&mut out);
            out.push(format!("> {}", inline(text)));
            out.push(String::new());
            (listed, table) = (false, false);
            continue;
        }
        if trimmed.len() >= 4 && trimmed.chars().all(|c| c == '-') {
            blank(&mut out);
            out.push("---".into());
            out.push(String::new());
            (listed, table) = (false, false);
            continue;
        }
        if let Some((marks, text)) = item(trimmed) {
            if !listed {
                blank(&mut out);
            }
            let indent: usize = (marks.chars().rev().skip(1)).map(|m| if m == '#' { 3 } else { 2 }).sum();
            let mark = if marks.ends_with('#') { "1." } else { "-" };
            out.push(format!("{lead}{}{mark} {}", " ".repeat(indent), inline(text)));
            (listed, table) = (true, false);
            continue;
        }
        if trimmed.starts_with('|') {
            if !table {
                blank(&mut out);
            }
            let head = trimmed.starts_with("||");
            let cells = cells(trimmed);
            out.push(format!("{lead}| {} |", cells.join(" | ")));
            if !table {
                // A table's first row is its head in markdown, whether Jira marked it so or not.
                out.push(format!("{lead}|{}", " --- |".repeat(cells.len().max(1))));
                let _ = head;
            }
            (listed, table) = (false, true);
            continue;
        }
        if listed || table {
            blank(&mut out);
            (listed, table) = (false, false);
        }
        // A line break in Jira's text is one where it is drawn: a hard break in markdown.
        if let Some(last) = out.last_mut()
            && !last.is_empty()
            && !last.starts_with('#')
            && !last.starts_with("```")
            && !last.ends_with('\\')
        {
            last.push('\\');
        }
        out.push(format!("{lead}{}", inline(line.trim_end())));
    }
    if code.is_some() {
        out.push("```".into());
    }
    while out.last().is_some_and(|l| l.is_empty()) {
        out.pop();
    }
    out.join("\n")
}

/// `{code:kotlin}rest` or `{noformat}rest`: what closes it, the language, what follows on the line.
fn code_start(line: &str) -> Option<(&'static str, String, &str)> {
    for (open, close) in [("{code", "{code}"), ("{noformat", "{noformat}")] {
        let Some(after) = line.strip_prefix(open) else {
            continue;
        };
        let end = after.find('}')?;
        let params = after[..end].strip_prefix(':').unwrap_or_default();
        let lang = if open == "{code" {
            (params.split('|'))
                .find_map(|p| match p.split_once('=') {
                    Some(("language", l)) => Some(l.trim()),
                    Some(_) => None,
                    None => Some(p.trim()),
                })
                .unwrap_or_default()
                .to_owned()
        } else {
            String::new()
        };
        return Some((close, lang, &after[end + 1..]));
    }
    None
}

/// `h2. Steps`: the level and the text.
fn heading(line: &str) -> Option<(usize, &str)> {
    let rest = line.strip_prefix('h')?;
    let level = rest.chars().next()?.to_digit(10)? as usize;
    let text = rest[1..].strip_prefix(". ")?;
    (1..=6).contains(&level).then_some((level, text))
}

/// `## item` or `*# item` or `- item`: the marks and the text.
fn item(line: &str) -> Option<(&str, &str)> {
    let end = line.find(|c: char| c != '*' && c != '#' && c != '-')?;
    let marks = &line[..end];
    let rest = &line[end..];
    if marks.is_empty() || !rest.starts_with(' ') || (marks.contains('-') && marks != "-") {
        return None;
    }
    Some((marks, rest.trim_start()))
}

/// A table row's cells, each as markdown: `||a||b||` or `|a|b|`.
fn cells(line: &str) -> Vec<String> {
    // Links first: their `|` is not a cell's.
    let line = inline(line);
    let inner = line.trim_matches('|');
    inner
        .split('|')
        .filter(|c| !c.is_empty())
        .map(|c| c.trim().to_owned())
        .collect()
}

/// A line's text as markdown.
pub fn inline(text: &str) -> String {
    let c: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < c.len() {
        // `{{monospace}}`: as written inside.
        if starts(&c, i, "{{")
            && let Some(end) = find(&c, i + 2, "}}")
        {
            out.push('`');
            out.extend(&c[i + 2..end]);
            out.push('`');
            i = end + 2;
            continue;
        }
        // `{color:red}`, `{color}`, `{anchor:x}`: their text alone.
        if (starts(&c, i, "{color") || starts(&c, i, "{anchor"))
            && let Some(end) = find(&c, i, "}")
        {
            i = end + 1;
            continue;
        }
        if c[i] == '['
            && let Some(end) = find(&c, i + 1, "]")
            && let Some(link) = link(&c[i + 1..end].iter().collect::<String>())
        {
            out.push_str(&link);
            i = end + 1;
            continue;
        }
        // `!picture.png!`, `!picture.png|thumbnail!`: its name.
        if c[i] == '!'
            && let Some(end) = find(&c, i + 1, "!")
            && let Some(name) = picture(&c[i + 1..end].iter().collect::<String>())
        {
            out.push_str(&format!("*{name}*"));
            i = end + 1;
            continue;
        }
        // `\\`: a line break.
        if starts(&c, i, "\\\\") {
            out.push_str("\\\n");
            i += 2;
            continue;
        }
        let pair = match c[i] {
            '*' => Some("**"),
            '_' => Some("*"),
            '-' => Some("~~"),
            '+' | '^' | '~' => Some(""),
            _ => None,
        };
        if let Some(md) = pair
            && opens(&c, i)
            && let Some(end) = closes(&c, i)
        {
            out.push_str(md);
            out.push_str(&inline(&c[i + 1..end].iter().collect::<String>()));
            out.push_str(md);
            i = end + 1;
            continue;
        }
        match c[i] {
            // `List<String>` is no HTML.
            '<' => out.push_str("\\<"),
            ch => out.push(ch),
        }
        i += 1;
    }
    out
}

fn starts(c: &[char], at: usize, s: &str) -> bool {
    let s: Vec<char> = s.chars().collect();
    c.get(at..at + s.len()) == Some(&s[..])
}

fn find(c: &[char], from: usize, s: &str) -> Option<usize> {
    (from..c.len()).find(|&j| starts(c, j, s))
}

/// The mark at `i` opens a span: after the line's start or a space or a mark, before a letter.
fn opens(c: &[char], i: usize) -> bool {
    let before = i.checked_sub(1).map(|b| c[b]);
    let starts_word = before.is_none_or(|b| b.is_whitespace() || (!b.is_alphanumeric() && b != c[i]));
    let next = c.get(i + 1);
    starts_word && next.is_some_and(|n| !n.is_whitespace() && *n != c[i])
}

/// Where the span the mark at `i` opens closes: the same mark after a letter, before the word's end.
fn closes(c: &[char], i: usize) -> Option<usize> {
    (i + 2..c.len()).find(|&j| {
        c[j] == c[i] && !c[j - 1].is_whitespace() && c.get(j + 1).is_none_or(|n| !n.is_alphanumeric())
    })
}

/// `[text|url]`, `[url]`, `[~user]`, `[#anchor]`, `[^file]` as markdown; None when it is no link.
fn link(inner: &str) -> Option<String> {
    let web = |u: &str| ["http://", "https://", "mailto:", "file:"].iter().any(|p| u.starts_with(p));
    if let Some(user) = inner.strip_prefix('~') {
        return Some(format!("@{}", user.trim_start_matches("accountid:")));
    }
    if let Some(name) = inner.strip_prefix('^').or_else(|| inner.strip_prefix('#')) {
        return Some(name.to_owned());
    }
    match inner.split('|').collect::<Vec<_>>()[..] {
        [url] if web(url.trim()) => Some(format!("<{}>", url.trim())),
        [text, url, ..] if web(url.trim()) => Some(format!("[{}]({})", inline(text.trim()), url.trim())),
        [text, other, ..] => Some(format!("{} ({})", inline(text.trim()), other.trim())),
        _ => None,
    }
}

/// `picture.png|thumbnail`: the name, when it is a picture's.
fn picture(inner: &str) -> Option<&str> {
    let name = inner.split('|').next()?;
    let ext = name.rsplit_once('.')?.1.to_ascii_lowercase();
    let pictures = ["png", "jpg", "jpeg", "gif", "svg", "webp", "bmp"];
    (!name.contains(' ') && pictures.contains(&ext.as_str())).then_some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_description_in_jiras_wiki_reads_as_markdown() {
        let wiki = "h2. What happens\r\nA partial capture of *12.345* refunds _12.34_, a cent short. See {{Money.round}} \
                    and [the rounding rules|https://example.com/rounding].\r\n\r\nh3. Steps\r\n# Capture 12.345\r\n\
                    # Refund it in part\r\n## Twice\r\n\r\n* Seen on EUR\r\n* Not on JPY\r\nAfter the list.\r\n\r\n\
                    {code:kotlin}\r\nval refund = capture.setScale(2)\r\n\r\nval x = 1\r\n{code}\r\n\
                    ||Currency||Seen||\r\n|EUR|yes|\r\n----\r\nDone when it equals.\r\nTo the cent.";
        assert_eq!(
            to_markdown(wiki),
            "## What happens\n\nA partial capture of **12.345** refunds *12.34*, a cent short. See `Money.round` and \
             [the rounding rules](https://example.com/rounding).\n\n### Steps\n\n1. Capture 12.345\n1. Refund it in \
             part\n   1. Twice\n\n- Seen on EUR\n- Not on JPY\n\nAfter the list.\n\n```kotlin\nval refund = \
             capture.setScale(2)\n\nval x = 1\n```\n\n| Currency | Seen |\n| --- | --- |\n| EUR | yes |\n\n---\n\n\
             Done when it equals.\\\nTo the cent."
        );
    }

    #[test]
    fn marks_inside_words_and_code_stay_as_written() {
        assert_eq!(inline("snake_case_name and a-b-c and 2 * 3"), "snake_case_name and a-b-c and 2 * 3");
        assert_eq!(inline("{{a_b*c}} -gone- +under+"), "`a_b*c` ~~gone~~ under");
        assert_eq!(inline("List<String> [~jhale] [PAY-412] !shot.png|thumbnail!"), "List\\<String> @jhale [PAY-412] *shot.png*");
        assert_eq!(inline("{color:red}*late*{color}"), "**late**");
        assert_eq!(to_markdown("{code}one(){code}"), "```\none()\n```");
        assert_eq!(to_markdown("{quote}\nSaid so.\n{quote}"), "> Said so.");
    }
}
