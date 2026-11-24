//! Who made a model and when (the MIRIAM history: creators, created and
//! modified dates), and its notes.

use sbml_io::miriam::{self, History};

use crate::edit::EditError;
use crate::{Element, Workspace};

/// The XHTML namespace notes are written in.
const XHTML: &str = "http://www.w3.org/1999/xhtml";

impl Workspace {
    /// An element's history: its creators and created and modified dates
    /// (W3CDTF, as written). Empty if it has none.
    pub fn history(&self, el: Element) -> History {
        if !self.contains(el) {
            return History::default();
        }
        self.meta(el)
            .annotation
            .as_deref()
            .map(|x| miriam::read_annotation(x, &self.doc.namespaces).history)
            .unwrap_or_default()
    }

    /// Replaces an element's history, keeping the rest of its annotation.
    /// One undoable edit.
    pub fn set_history(&mut self, el: Element, history: History) -> Result<(), EditError> {
        let meta = self.with_annotation(el, |_, h| {
            *h = history;
            Ok(())
        })?;
        self.apply(el, meta);
        Ok(())
    }

    /// An element's notes as plain text: paragraphs separated by blank
    /// lines, line breaks kept, markup and entities gone. `None` if it has
    /// no notes.
    pub fn notes_text(&self, el: Element) -> Option<String> {
        if !self.contains(el) {
            return None;
        }
        self.meta(el).notes.as_deref().map(notes_to_text)
    }

    /// Whether an element's notes use more than paragraphs and line breaks
    /// (links, lists, tables, headings), which [`Workspace::set_notes_text`]
    /// would replace with plain paragraphs.
    pub fn notes_are_rich(&self, el: Element) -> bool {
        self.contains(el) && self.meta(el).notes.as_deref().is_some_and(is_rich)
    }

    /// Sets an element's notes from plain text: each paragraph (separated
    /// by a blank line) becomes an XHTML paragraph, and line breaks inside
    /// one are kept. Empty text removes the notes. One undoable edit.
    pub fn set_notes_text(&mut self, el: Element, text: &str) -> Result<(), EditError> {
        if !self.contains(el) {
            return Err(EditError::NoSuchElement);
        }
        let mut meta = self.meta(el).clone();
        meta.notes = text_to_notes(text);
        self.apply(el, meta);
        Ok(())
    }
}

/// The class of the notes paragraph PathwayAnnotator writes and replaces.
const TOOL_CLASS: &str = "pathwayannotator";

impl Workspace {
    /// The paragraph PathwayAnnotator wrote into an element's notes (why
    /// it was identified as what it is), as plain text.
    pub fn tool_note(&self, el: Element) -> Option<String> {
        let notes = self.meta(el).notes.as_deref()?;
        let (start, end) = tool_paragraph(notes)?;
        Some(notes_to_text(&notes[start..end]).trim().to_string())
    }

    /// Sets the paragraph PathwayAnnotator writes into an element's notes,
    /// replacing the one it wrote before; `None` removes it. The rest of
    /// the notes is kept as it is. One undoable edit.
    pub fn set_tool_note(&mut self, el: Element, text: Option<&str>) -> Result<(), EditError> {
        if !self.contains(el) {
            return Err(EditError::NoSuchElement);
        }
        let mut meta = self.meta(el).clone();
        meta.notes = with_tool_note(meta.notes.as_deref(), text);
        self.apply(el, meta);
        Ok(())
    }
}

/// Where the tool's paragraph is in some notes XHTML: the byte range of
/// `<p ... class="pathwayannotator">...</p>`.
fn tool_paragraph(notes: &str) -> Option<(usize, usize)> {
    let marker = format!("class=\"{TOOL_CLASS}\"");
    let at = notes.find(&marker)?;
    let start = notes[..at].rfind("<p")?;
    let end = at + notes[at..].find("</p>")? + "</p>".len();
    Some((start, end))
}

/// `notes` with the tool's paragraph replaced by `text` (or removed).
fn with_tool_note(notes: Option<&str>, text: Option<&str>) -> Option<String> {
    let mut rest = notes.unwrap_or("").to_string();
    if let Some((start, end)) = tool_paragraph(&rest) {
        // Take the line the paragraph was on with it.
        let start = rest[..start].trim_end_matches([' ', '\t']).len();
        let start = if rest[..start].ends_with('\n') { start - 1 } else { start };
        rest.replace_range(start..end, "");
    }
    let Some(text) = text.map(str::trim).filter(|t| !t.is_empty()) else {
        // Nothing left but an empty body: no notes.
        let empty = notes_to_text(&rest).trim().is_empty() && !is_rich(&rest);
        return (!empty).then_some(rest);
    };
    let para = format!("  <p xmlns=\"{XHTML}\" class=\"{TOOL_CLASS}\">{}</p>", escape(text));
    if notes_to_text(&rest).trim().is_empty() && !is_rich(&rest) {
        return Some(format!("
<body xmlns=\"{XHTML}\">
{para}
</body>
"));
    }
    match rest.rfind("</body>") {
        Some(at) => {
            let at = rest[..at].trim_end().len();
            rest.insert_str(at, &format!("
{para}"));
        }
        None => {
            let at = rest.trim_end().len();
            rest.insert_str(at, &format!("
{para}"));
        }
    }
    Some(rest)
}

/// Plain text as XHTML notes, or `None` for empty text.
fn text_to_notes(text: &str) -> Option<String> {
    let text = text.replace("\r\n", "\n");
    let paragraphs: Vec<String> = text
        .split("\n\n")
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(|p| {
            let lines: Vec<String> = p.lines().map(|l| escape(l.trim_end())).collect();
            format!("  <p>{}</p>", lines.join("<br/>"))
        })
        .collect();
    if paragraphs.is_empty() {
        return None;
    }
    Some(format!("\n<body xmlns=\"{XHTML}\">\n{}\n</body>\n", paragraphs.join("\n")))
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// The names of the elements in some XML, lower case, without prefixes.
fn tag_names(xml: &str) -> impl Iterator<Item = String> + '_ {
    xml.split('<').skip(1).filter_map(|t| {
        let t = t.trim_start_matches('/');
        if t.starts_with(['!', '?']) {
            return None;
        }
        let name: String = t.chars().take_while(|c| c.is_alphanumeric() || *c == ':' || *c == '-').collect();
        let name = name.rsplit(':').next().unwrap_or("").to_ascii_lowercase();
        (!name.is_empty()).then_some(name)
    })
}

fn is_rich(xml: &str) -> bool {
    tag_names(xml).any(|n| !matches!(n.as_str(), "html" | "body" | "p" | "br" | "div" | "span"))
}

/// Notes XHTML as plain text.
fn notes_to_text(xml: &str) -> String {
    // Block ends and line breaks become newlines; other tags go.
    let mut out = String::new();
    let mut rest = xml;
    while let Some(at) = rest.find('<') {
        out.push_str(&rest[..at]);
        let Some(end) = rest[at..].find('>') else {
            rest = "";
            break;
        };
        let tag = &rest[at + 1..at + end];
        let name: String = tag
            .trim_start_matches('/')
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == ':')
            .collect();
        let name = name.rsplit(':').next().unwrap_or("").to_ascii_lowercase();
        let closing = tag.starts_with('/');
        match name.as_str() {
            "br" => out.push('\n'),
            "p" | "div" | "ul" | "ol" | "table" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" if closing => {
                out.push_str("\n\n")
            }
            "li" | "tr" if closing => out.push('\n'),
            "li" => out.push_str("- "),
            _ => {}
        }
        rest = &rest[at + end + 1..];
    }
    out.push_str(rest);
    let text = decode_entities(&out);
    // Tidy: trim each line, at most one blank line in a row.
    let mut lines: Vec<&str> = Vec::new();
    for l in text.lines().map(str::trim) {
        if l.is_empty() && lines.last().is_none_or(|p| p.is_empty()) {
            continue;
        }
        lines.push(l);
    }
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    lines.join("\n")
}

fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        let after = &rest[at + 1..];
        let Some(semi) = after.find(';').filter(|n| *n <= 10) else {
            out.push('&');
            rest = after;
            continue;
        };
        let name = &after[..semi];
        let ch = match name {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            _ => name
                .strip_prefix("#x")
                .or_else(|| name.strip_prefix("#X"))
                .and_then(|h| u32::from_str_radix(h, 16).ok())
                .or_else(|| name.strip_prefix('#').and_then(|d| d.parse().ok()))
                .and_then(char::from_u32),
        };
        match ch {
            Some(c) => {
                out.push(c);
                rest = &after[semi + 1..];
            }
            None => {
                out.push('&');
                rest = after;
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
    fn text_and_back() {
        let text = "A model of glycolysis.\nIn yeast.\n\nFitted to data from 2026 & earlier <unpublished>.";
        let xml = text_to_notes(text).unwrap();
        assert!(xml.contains("<body xmlns=\"http://www.w3.org/1999/xhtml\">"));
        assert!(xml.contains("<p>A model of glycolysis.<br/>In yeast.</p>"));
        assert!(xml.contains("&amp; earlier &lt;unpublished&gt;"));
        assert_eq!(notes_to_text(&xml), text);
        assert!(!is_rich(&xml));
        assert_eq!(text_to_notes("  \n\n "), None);
    }

    #[test]
    fn rich_notes() {
        let xml = r#"<body xmlns="http://www.w3.org/1999/xhtml"><h2>About</h2><p>See <a href="https://x.org">x</a>.</p><ul><li>one</li><li>two</li></ul></body>"#;
        assert!(is_rich(xml));
        assert_eq!(notes_to_text(xml), "About\n\nSee x.\n\n- one\n- two");
        assert!(!is_rich(r#"<html:p xmlns:html="http://www.w3.org/1999/xhtml">plain</html:p>"#));
        assert_eq!(notes_to_text("<p>caf&#233; &#x2013; ok</p>"), "caf\u{e9} \u{2013} ok");
    }

    #[test]
    fn the_tool_note_keeps_other_notes_and_replaces_itself() {
        use super::with_tool_note;
        // No notes: a body with the paragraph.
        let a = with_tool_note(None, Some("Identified as ADP & more")).unwrap();
        assert!(a.contains("class=\"pathwayannotator\">Identified as ADP &amp; more</p>"), "{a}");
        assert_eq!(notes_to_text(&a).trim(), "Identified as ADP & more");
        // Replacing it leaves one paragraph.
        let b = with_tool_note(Some(&a), Some("Identified as ATP")).unwrap();
        assert_eq!(b.matches("pathwayannotator").count(), 1);
        assert_eq!(notes_to_text(&b).trim(), "Identified as ATP");
        // Removing it removes the notes it created.
        assert_eq!(with_tool_note(Some(&b), None), None);
        // The modeller's own notes are kept, before and after.
        let own = "<body xmlns=\"http://www.w3.org/1999/xhtml\"><p>Mine</p></body>";
        let c = with_tool_note(Some(own), Some("Identified as ADP")).unwrap();
        assert_eq!(notes_to_text(&c).trim(), "Mine

Identified as ADP");
        assert_eq!(notes_to_text(&with_tool_note(Some(&c), None).unwrap()).trim(), "Mine");
        // Notes without a body: the paragraph is added after them.
        let bare = "<p xmlns=\"http://www.w3.org/1999/xhtml\">Mine</p>";
        let d = with_tool_note(Some(bare), Some("Why")).unwrap();
        assert_eq!(notes_to_text(&d).trim(), "Mine

Why");
    }
}
