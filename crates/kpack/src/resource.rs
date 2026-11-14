//! Annotation resources: the URIs in SBML `bqbiol:is` terms, taken apart
//! into a namespace and an id.
//!
//! Models in the wild write the same id many ways:
//!
//! | Form | Example |
//! |---|---|
//! | identifiers.org path | `http://identifiers.org/chebi/CHEBI:15422` |
//! | identifiers.org compact | `https://identifiers.org/CHEBI:15422` |
//! | MIRIAM URN (old) | `urn:miriam:obo.chebi:CHEBI%3A15422` |
//! | ChEBI web page | `http://www.ebi.ac.uk/chebi/searchId.do?chebiId=CHEBI:15422` |
//! | KEGG web page | `http://www.genome.jp/dbget-bin/www_bget?cpd:C00002` |
//!
//! [`split_resource`] returns the namespace as written (an identifiers.org
//! prefix such as `chebi` or `obo.chebi`, or a whole URN prefix such as
//! `urn:miriam:obo.chebi`) and the id. Mapping that onto a known namespace,
//! and putting the id in its standard form, is the caller's step, because it
//! needs the namespace table.

/// `(namespace as written, id)`, or `None` for a URI in no form this knows.
pub fn split_resource(uri: &str) -> Option<(String, String)> {
    let uri = percent_decode(uri.trim());

    if let Some(rest) = uri.strip_prefix("urn:miriam:") {
        let (ns, id) = rest.split_once(':')?;
        return non_empty(format!("urn:miriam:{ns}"), id);
    }

    let rest = uri.split_once("://").map_or(uri.as_str(), |(_, r)| r);
    let rest = rest.strip_prefix("www.").unwrap_or(rest);

    if let Some(path) = rest.strip_prefix("identifiers.org/") {
        let path = path.trim_end_matches('/');
        // `chebi/CHEBI:15422`; else compact, `CHEBI:15422`.
        let (prefix, id) = path.split_once('/').or_else(|| path.split_once(':'))?;
        return non_empty(prefix.to_ascii_lowercase(), id);
    }

    if let Some((_, query)) = rest.split_once("chebiId=") {
        let id = query.split('&').next()?;
        return non_empty("chebi".into(), id);
    }

    if let Some((_, query)) = rest.split_once("dbget-bin/www_bget?") {
        let (db, id) = query.split_once(':').unwrap_or(("", query));
        let prefix = match db {
            "cpd" => "kegg.compound",
            "rn" => "kegg.reaction",
            "ec" => "ec-code",
            "" if id.starts_with('C') => "kegg.compound",
            "" if id.starts_with('R') => "kegg.reaction",
            _ => return None,
        };
        return non_empty(prefix.into(), id);
    }
    None
}

fn non_empty(ns: String, id: &str) -> Option<(String, String)> {
    let id = id.trim();
    (!ns.is_empty() && !id.is_empty()).then(|| (ns, id.to_string()))
}

/// Decodes `%XX` escapes (`CHEBI%3A15422` -> `CHEBI:15422`). A malformed
/// escape is left as it is.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
            if let Some(b) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn split(uri: &str) -> Option<(String, String)> {
        split_resource(uri)
    }

    fn pair(ns: &str, id: &str) -> Option<(String, String)> {
        Some((ns.into(), id.into()))
    }

    #[test]
    fn every_form() {
        assert_eq!(split("http://identifiers.org/chebi/CHEBI:15422"), pair("chebi", "CHEBI:15422"));
        assert_eq!(split("https://identifiers.org/CHEBI:15422"), pair("chebi", "15422"));
        assert_eq!(split("https://identifiers.org/kegg.compound/C00002/"), pair("kegg.compound", "C00002"));
        assert_eq!(split("http://identifiers.org/obo.chebi/CHEBI:15422"), pair("obo.chebi", "CHEBI:15422"));
        assert_eq!(split("urn:miriam:obo.chebi:CHEBI%3A15422"), pair("urn:miriam:obo.chebi", "CHEBI:15422"));
        assert_eq!(split("urn:miriam:kegg.compound:C00002"), pair("urn:miriam:kegg.compound", "C00002"));
        assert_eq!(split("urn:miriam:ec-code:2.7.1.1"), pair("urn:miriam:ec-code", "2.7.1.1"));
        assert_eq!(
            split("http://www.ebi.ac.uk/chebi/searchId.do?chebiId=CHEBI:15422"),
            pair("chebi", "CHEBI:15422")
        );
        assert_eq!(
            split("http://www.genome.jp/dbget-bin/www_bget?cpd:C00002"),
            pair("kegg.compound", "C00002")
        );
        assert_eq!(
            split("http://www.genome.jp/dbget-bin/www_bget?R00299"),
            pair("kegg.reaction", "R00299")
        );
    }

    #[test]
    fn junk_is_refused() {
        assert_eq!(split("http://example.org/thing"), None);
        assert_eq!(split("urn:miriam:chebi"), None);
        assert_eq!(split("https://identifiers.org/"), None);
        assert_eq!(split(""), None);
    }

    #[test]
    fn percent_escapes() {
        assert_eq!(percent_decode("CHEBI%3A15422"), "CHEBI:15422");
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("a%zzb"), "a%zzb");
        assert_eq!(percent_decode("x%3"), "x%3");
    }
}
