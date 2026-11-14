//! The identifier namespaces the pack knows, and how each source spells them.
//!
//! Prefixes and patterns follow identifiers.org. The patterns are recorded
//! for the annotator's identifier checks; the builder does not enforce them
//! yet. Check them against the identifiers.org registry before relying on
//! them for validation.

/// One namespace.
#[derive(Clone, Copy, Debug)]
pub struct NsDef {
    /// identifiers.org prefix.
    pub prefix: &'static str,
    pub label: &'static str,
    pub pattern: &'static str,
    /// Old MIRIAM URN prefixes for the same namespace.
    pub legacy_urns: &'static [&'static str],
    /// Text the identifiers.org form of an id starts with, when sources
    /// sometimes leave it off: BiGG gives ChEBI ids as `15422`, identifiers.org
    /// wants `CHEBI:15422`.
    pub id_prefix: &'static str,
}

pub const KNOWN: &[NsDef] = &[
    ns("bigg.metabolite", "BiGG metabolite", r"^[a-zA-Z0-9_]+$", &[], ""),
    ns("bigg.reaction", "BiGG reaction", r"^[a-zA-Z0-9_]+$", &[], ""),
    ns("chebi", "ChEBI", r"^CHEBI:\d+$", &["urn:miriam:obo.chebi", "urn:miriam:chebi"], "CHEBI:"),
    ns("kegg.compound", "KEGG Compound", r"^C\d+$", &["urn:miriam:kegg.compound"], ""),
    ns("kegg.drug", "KEGG Drug", r"^D\d+$", &["urn:miriam:kegg.drug"], ""),
    ns("kegg.reaction", "KEGG Reaction", r"^R\d+$", &["urn:miriam:kegg.reaction"], ""),
    ns("metacyc.compound", "MetaCyc compound", r"^[A-Za-z0-9+_.%:-]+$", &[], ""),
    ns("metacyc.reaction", "MetaCyc reaction", r"^[A-Za-z0-9+_.%:-]+$", &[], ""),
    ns("metanetx.chemical", "MetaNetX chemical", r"^(MNXM\d+|BIOMASS|WATER)$", &[], ""),
    ns("metanetx.reaction", "MetaNetX reaction", r"^(MNXR\d+|EMPTY)$", &[], ""),
    ns("seed.compound", "ModelSEED compound", r"^cpd\d+$", &[], ""),
    ns("seed.reaction", "ModelSEED reaction", r"^rxn\d+$", &[], ""),
    ns("hmdb", "HMDB", r"^HMDB\d+$", &["urn:miriam:hmdb"], ""),
    ns("drugbank", "DrugBank", r"^DB\d{5}$", &["urn:miriam:drugbank"], ""),
    ns("wikipedia.en", "Wikipedia", r"^[A-Za-z0-9_%(),.'-]+$", &[], ""),
    ns("rhea", "Rhea", r"^\d{5,6}$", &["urn:miriam:rhea"], ""),
    ns("ec-code", "EC number", r"^\d+(\.(\d+|-|n\d+)){0,3}$", &["urn:miriam:ec-code"], ""),
    ns("go", "Gene Ontology", r"^GO:\d{7}$", &["urn:miriam:obo.go", "urn:miriam:go"], "GO:"),
];

const fn ns(
    prefix: &'static str,
    label: &'static str,
    pattern: &'static str,
    legacy_urns: &'static [&'static str],
    id_prefix: &'static str,
) -> NsDef {
    NsDef {
        prefix,
        label,
        pattern,
        legacy_urns,
        id_prefix,
    }
}

/// The namespace with this identifiers.org prefix.
pub fn by_prefix(prefix: &str) -> Option<&'static NsDef> {
    KNOWN.iter().find(|n| n.prefix == prefix)
}

/// The namespace BiGG calls `name` (`CHEBI`, `GO`, `kegg.compound`, ...).
pub fn from_bigg(name: &str) -> Option<&'static NsDef> {
    match name {
        "CHEBI" => by_prefix("chebi"),
        "GO" => by_prefix("go"),
        other => by_prefix(&other.to_ascii_lowercase()),
    }
}

/// The namespace and standard id for a namespace as a model wrote it (from
/// [`kpack::split_resource`]): an identifiers.org prefix (`chebi`), an old
/// alias of one (`obo.chebi`), or a MIRIAM URN prefix
/// (`urn:miriam:obo.chebi`).
pub fn resolve(written: &str, id: &str) -> Option<(&'static NsDef, String)> {
    let def = if written.starts_with("urn:miriam:") {
        KNOWN.iter().find(|n| n.legacy_urns.contains(&written))
    } else {
        by_prefix(written).or_else(|| {
            KNOWN.iter().find(|n| {
                n.legacy_urns
                    .iter()
                    .any(|u| u.strip_prefix("urn:miriam:") == Some(written))
            })
        })
    }?;
    Some((def, normalize_id(def, id)))
}

/// An id in identifiers.org form: adds the namespace's id prefix if the
/// source left it off.
pub fn normalize_id(def: &NsDef, id: &str) -> String {
    let id = id.trim();
    if def.id_prefix.is_empty() || id.starts_with(def.id_prefix) {
        id.to_string()
    } else {
        format!("{}{}", def.id_prefix, id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bigg_spellings_and_id_forms() {
        let chebi = from_bigg("CHEBI").unwrap();
        assert_eq!(chebi.prefix, "chebi");
        assert_eq!(normalize_id(chebi, "15422"), "CHEBI:15422");
        assert_eq!(normalize_id(chebi, "CHEBI:15422"), "CHEBI:15422");
        let go = from_bigg("GO").unwrap();
        assert_eq!(normalize_id(go, "0004340"), "GO:0004340");
        assert_eq!(from_bigg("kegg.compound").unwrap().prefix, "kegg.compound");
        assert!(from_bigg("no.such.namespace").is_none());
    }

    #[test]
    fn written_forms_resolve() {
        let r = |w: &str, id: &str| resolve(w, id).map(|(d, id)| (d.prefix, id));
        assert_eq!(r("chebi", "15422"), Some(("chebi", "CHEBI:15422".into())));
        assert_eq!(r("obo.chebi", "CHEBI:15422"), Some(("chebi", "CHEBI:15422".into())));
        assert_eq!(r("urn:miriam:obo.chebi", "CHEBI:15422"), Some(("chebi", "CHEBI:15422".into())));
        assert_eq!(r("urn:miriam:kegg.compound", "C00002"), Some(("kegg.compound", "C00002".into())));
        assert_eq!(r("obo.go", "GO:0005829"), Some(("go", "GO:0005829".into())));
        assert_eq!(r("uniprot", "P12345"), None);
    }
}
