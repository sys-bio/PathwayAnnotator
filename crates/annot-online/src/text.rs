//! Small text helpers: URL encoding, HTML in answers, search text from ids.

/// Percent-encodes everything but unreserved characters (RFC 3986), for
/// one query parameter value.
pub fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Text without HTML tags or the few entities databases use:
/// "<small>D</small>-fructose" -> "D-fructose".
pub(crate) fn strip_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'")
}

/// Search text for an element known only by its SBML id: "M_glc__D_c" ->
/// "glc D", "Glc_ext" -> "Glc", "PFK" -> "PFK". SBML prefixes and a
/// final compartment code are dropped and underscores become spaces. The
/// user can edit it before searching.
pub fn query_from_id(id: &str) -> String {
    let s = ["M_", "R_", "S_"].iter().find_map(|p| id.strip_prefix(p)).unwrap_or(id);
    let mut words: Vec<&str> = s.split('_').filter(|w| !w.is_empty()).collect();
    const PLACES: &[&str] = &["c", "e", "m", "p", "x", "n", "r", "g", "l", "v", "ext", "cyt", "cyto", "mit", "in", "out"];
    if words.len() > 1 && words.last().is_some_and(|w| PLACES.contains(&w.to_ascii_lowercase().as_str())) {
        words.pop();
    }
    words.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoding() {
        assert_eq!(percent_encode("fructose 6-phosphate"), "fructose%206-phosphate");
        assert_eq!(percent_encode("(chebi:1 OR chebi:2)"), "%28chebi%3A1%20OR%20chebi%3A2%29");
        assert_eq!(percent_encode("\"a\""), "%22a%22");
    }

    #[test]
    fn html() {
        assert_eq!(strip_html("<small>D</small>-fructose &amp; more"), "D-fructose & more");
    }

    #[test]
    fn ids() {
        assert_eq!(query_from_id("M_glc__D_c"), "glc D");
        assert_eq!(query_from_id("Glc_ext"), "Glc");
        assert_eq!(query_from_id("PFK"), "PFK");
        assert_eq!(query_from_id("ATP"), "ATP");
        assert_eq!(query_from_id("c"), "c");
    }
}
