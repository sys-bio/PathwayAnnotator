//! The annotation a pack target becomes when the user accepts it.

use kpack::{Pack, ReactionFlags, Target, XrefScope};

/// An identifiers.org URI in compact form: `https://identifiers.org/CHEBI:15422`,
/// `https://identifiers.org/kegg.compound:C00002`. The id is in
/// identifiers.org form (as the pack stores it).
pub fn uri(prefix: &str, id: &str) -> String {
    // Ids that carry their own prefix (CHEBI:15422, GO:0005829) stand alone.
    let own_prefix = id
        .split_once(':')
        .is_some_and(|(p, _)| p.eq_ignore_ascii_case(prefix));
    if own_prefix {
        format!("https://identifiers.org/{id}")
    } else {
        format!("https://identifiers.org/{prefix}:{id}")
    }
}

/// The `bqbiol:is` URIs for accepting `target`:
///
/// - a molecule: one ChEBI id (the neutral form if the pack has one, else
///   the first) and its KEGG compound id;
/// - a reaction: its Rhea ids, EC numbers and KEGG reaction id, or its
///   BiGG id if it has none of those.
///
/// Only exact, current ids: never old (secondary) or broader ones.
pub fn uris_for(pack: &Pack, target: Target) -> Vec<String> {
    let usable = |x: &&kpack::Xref| x.scope == XrefScope::Exact && !x.secondary;
    let mut out = Vec::new();
    match target {
        Target::Entity(e) => {
            let chebi: Vec<_> = pack.entity_xrefs(e, "chebi").filter(usable).collect();
            if let Some(x) = chebi.iter().find(|x| x.form_charge == Some(0)).or(chebi.first()) {
                out.push(uri("chebi", pack.str(x.id)));
            }
            if let Some(x) = pack.entity_xrefs(e, "kegg.compound").find(usable) {
                out.push(uri("kegg.compound", pack.str(x.id)));
            }
            if out.is_empty()
                && let Some(x) = pack.entity_xrefs(e, "bigg.metabolite").find(usable)
            {
                out.push(uri("bigg.metabolite", pack.str(x.id)));
            }
        }
        Target::Reaction(r) => {
            let xrefs = &pack.reaction(r).xrefs;
            for prefix in ["rhea", "ec-code", "kegg.reaction"] {
                for x in xrefs.iter().filter(|x| pack.namespace_prefix(x.ns) == prefix).filter(usable) {
                    out.push(uri(prefix, pack.str(x.id)));
                }
            }
            if out.is_empty()
                && let Some(x) = xrefs.iter().find(|x| pack.namespace_prefix(x.ns) == "bigg.reaction")
            {
                out.push(uri("bigg.reaction", pack.str(x.id)));
            }
        }
    }
    out
}

/// The annotation URI for what someone typed: a full URI as it is
/// ("https://...", "http://...", "urn:miriam:..."), or a compact
/// identifier ("CHEBI:15422", "uniprot:P12345", "GO:0005829",
/// "ec-code:2.7.1.1") as an identifiers.org URI. `Err` says how to write
/// it.
pub fn uri_from_text(text: &str) -> Result<String, String> {
    let t = text.trim();
    if t.is_empty() {
        return Err("Write an identifier such as CHEBI:15422, or a full URL.".into());
    }
    if t.contains(char::is_whitespace) {
        return Err("An identifier has no spaces: write it as prefix:id (CHEBI:15422) or a full URL.".into());
    }
    let lower = t.to_ascii_lowercase();
    if lower.starts_with("https://") || lower.starts_with("http://") || lower.starts_with("urn:miriam:") {
        return Ok(t.to_string());
    }
    match t.split_once(':') {
        Some((prefix, id))
            if !prefix.is_empty()
                && !id.is_empty()
                && prefix.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_')) =>
        {
            Ok(format!("https://identifiers.org/{t}"))
        }
        _ => Err("Write it as prefix:id (CHEBI:15422, uniprot:P12345) or a full URL.".into()),
    }
}

/// The SBO term for accepting `target`: simple chemical (247) for a
/// molecule, transport reaction (185) or biochemical reaction (176).
pub fn sbo_for(pack: &Pack, target: Target) -> u32 {
    match target {
        Target::Entity(_) => 247,
        Target::Reaction(r) if pack.reaction(r).flags.contains(ReactionFlags::TRANSPORT) => 185,
        Target::Reaction(_) => 176,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_identifiers() {
        assert_eq!(uri_from_text(" CHEBI:15422 ").unwrap(), "https://identifiers.org/CHEBI:15422");
        assert_eq!(uri_from_text("uniprot:P12345").unwrap(), "https://identifiers.org/uniprot:P12345");
        assert_eq!(uri_from_text("ec-code:2.7.1.1").unwrap(), "https://identifiers.org/ec-code:2.7.1.1");
        assert_eq!(uri_from_text("http://identifiers.org/chebi/CHEBI:1").unwrap(), "http://identifiers.org/chebi/CHEBI:1");
        assert_eq!(uri_from_text("urn:miriam:obo.chebi:CHEBI%3A1").unwrap(), "urn:miriam:obo.chebi:CHEBI%3A1");
        for bad in ["", "glucose", "CHEBI 15422", ":15422", "CHEBI:", "a b:c"] {
            assert!(uri_from_text(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn compact_uris() {
        assert_eq!(uri("chebi", "CHEBI:15422"), "https://identifiers.org/CHEBI:15422");
        assert_eq!(uri("go", "GO:0005829"), "https://identifiers.org/GO:0005829");
        assert_eq!(uri("kegg.compound", "C00002"), "https://identifiers.org/kegg.compound:C00002");
        assert_eq!(uri("ec-code", "2.7.1.1"), "https://identifiers.org/ec-code:2.7.1.1");
        // And they read back.
        assert_eq!(
            kpack::split_resource(&uri("kegg.compound", "C00002")),
            Some(("kegg.compound".into(), "C00002".into()))
        );
    }
}
