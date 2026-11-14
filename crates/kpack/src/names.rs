//! The name dictionary: normalized name -> the entities (or reactions) it
//! may mean.

use serde::{Deserialize, Serialize};

use crate::sources::Sources;
use crate::strings::{StrId, StringTable};

/// How a name relates to what it names.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum NameKind {
    /// The name the pack shows for it.
    #[default]
    Preferred,
    /// Another full name.
    Synonym,
    /// A short form: "G6P".
    Abbreviation,
    /// A BiGG id, current or old: "g6p", "glc__D".
    BiggId,
    /// An id seen in a curated model: "M_g6p_c".
    ModelId,
}

/// One meaning of a name.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NameHit {
    /// An entity id in `Pack::names`, a reaction id in `Pack::reaction_names`.
    pub target: u32,
    pub kind: NameKind,
    pub sources: Sources,
    /// How many curated models used the name with this meaning (0 when the
    /// name came from a database rather than from models).
    pub count: u16,
}

/// Normalized name -> meanings. Keys are sorted by their text, so lookup is
/// a binary search; no hash map has to be rebuilt after loading.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NameIndex {
    keys: Vec<StrId>,
    postings: Vec<Vec<NameHit>>,
}

impl NameIndex {
    /// Builds an index from (name, meaning) pairs. Names are normalized here;
    /// `intern` stores a normalized key in the string table and returns its
    /// id. A name that means the same target twice keeps one hit, with the
    /// sources combined, the higher count, and the stronger kind (the one
    /// that sorts first).
    pub fn build(
        entries: impl IntoIterator<Item = (String, NameHit)>,
        mut intern: impl FnMut(&str) -> StrId,
    ) -> NameIndex {
        let mut normalized: Vec<(String, NameHit)> = entries
            .into_iter()
            .map(|(name, hit)| (normalize_name(&name), hit))
            .filter(|(key, _)| !key.is_empty())
            .collect();
        normalized.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.target.cmp(&b.1.target)));

        let mut index = NameIndex::default();
        let mut last_key: Option<String> = None;
        for (key, hit) in normalized {
            if last_key.as_deref() != Some(key.as_str()) {
                index.keys.push(intern(&key));
                index.postings.push(Vec::new());
                last_key = Some(key);
            }
            let hits = index.postings.last_mut().expect("just pushed");
            match hits.iter_mut().find(|h| h.target == hit.target) {
                Some(h) => {
                    h.sources |= hit.sources;
                    h.count = h.count.max(hit.count);
                    h.kind = h.kind.min(hit.kind);
                }
                None => hits.push(hit),
            }
        }
        index
    }

    /// The meanings of `name`, which is normalized first. Empty if unknown.
    pub fn lookup<'a>(&'a self, strings: &StringTable, name: &str) -> &'a [NameHit] {
        let key = normalize_name(name);
        match self
            .keys
            .binary_search_by(|k| strings.get(*k).cmp(key.as_str()))
        {
            Ok(i) => &self.postings[i],
            Err(_) => &[],
        }
    }

    /// The number of distinct normalized names.
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }
}

/// The form names are compared in. Two names that normalize the same are
/// treated as the same name.
///
/// - A trailing charge such as `(4-)` or `(1+)` is removed: BiGG and ChEBI
///   name charge forms "ATP(4-)", models say "ATP".
/// - Letters are lowercased; Greek letters become their names (alpha, beta).
/// - Everything that is not a letter or digit is dropped: spaces, hyphens,
///   commas, brackets, primes. "D-Glucose 6-phosphate" and "d_glucose6phosphate"
///   are the same name; "D-glucose" and "L-glucose" are not.
///
/// It deliberately does not remove compartment suffixes or expand
/// abbreviations; that is the name decoder's job, which knows the context.
pub fn normalize_name(name: &str) -> String {
    let name = strip_charge_suffix(name.trim());
    let mut out = String::with_capacity(name.len());
    for c in name.chars().flat_map(char::to_lowercase) {
        if let Some(word) = greek_letter_name(c) {
            out.push_str(word);
        } else if c.is_alphanumeric() {
            out.push(c);
        }
    }
    out
}

/// `ATP(4-)` -> `ATP`, `NAD(+)` -> `NAD`, `Fe(2+)` -> `Fe`. Anything else is
/// returned as it is.
pub fn strip_charge_suffix(name: &str) -> &str {
    let Some(body) = name.strip_suffix(')') else {
        return name;
    };
    let Some(open) = body.rfind('(') else {
        return name;
    };
    let inside = &body[open + 1..];
    let digits = inside.trim_end_matches(['+', '-']);
    let is_charge = inside.len() > digits.len()
        && inside.len() - digits.len() == 1
        && digits.chars().all(|c| c.is_ascii_digit());
    if is_charge && open > 0 {
        body[..open].trim_end()
    } else {
        name
    }
}

/// The ASCII name of a lowercase Greek letter.
fn greek_letter_name(c: char) -> Option<&'static str> {
    Some(match c {
        '\u{3b1}' => "alpha",
        '\u{3b2}' => "beta",
        '\u{3b3}' => "gamma",
        '\u{3b4}' => "delta",
        '\u{3b5}' => "epsilon",
        '\u{3b6}' => "zeta",
        '\u{3b7}' => "eta",
        '\u{3b8}' => "theta",
        '\u{3b9}' => "iota",
        '\u{3ba}' => "kappa",
        '\u{3bb}' => "lambda",
        '\u{3bc}' => "mu",
        '\u{3bd}' => "nu",
        '\u{3be}' => "xi",
        '\u{3bf}' => "omicron",
        '\u{3c0}' => "pi",
        '\u{3c1}' => "rho",
        '\u{3c2}' | '\u{3c3}' => "sigma",
        '\u{3c4}' => "tau",
        '\u{3c5}' => "upsilon",
        '\u{3c6}' => "phi",
        '\u{3c7}' => "chi",
        '\u{3c8}' => "psi",
        '\u{3c9}' => "omega",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization() {
        assert_eq!(normalize_name("ATP(4-)"), "atp");
        assert_eq!(normalize_name("NAD(+)"), "nad");
        assert_eq!(normalize_name("Fe(2+)"), "fe");
        assert_eq!(normalize_name("D-Glucose 6-phosphate"), "dglucose6phosphate");
        assert_eq!(normalize_name("d_glucose6phosphate"), "dglucose6phosphate");
        assert_ne!(normalize_name("D-glucose"), normalize_name("L-glucose"));
        assert_eq!(normalize_name("\u{3b1}-D-glucose"), "alphadglucose");
        assert_eq!(normalize_name("\u{391}-D-glucose"), "alphadglucose");
        // A bracket that is not a charge stays part of the name.
        assert_eq!(normalize_name("(2S)-malate"), "2smalate");
        assert_eq!(normalize_name("glucose (2)"), "glucose2");
        assert_eq!(normalize_name("  "), "");
    }

    #[test]
    fn index_merges_and_finds() {
        let mut table = Vec::<String>::new();
        let index = NameIndex::build(
            [
                ("ATP".to_string(), hit(1, NameKind::Preferred, Sources::BIGG, 0)),
                ("ATP(4-)".to_string(), hit(1, NameKind::Synonym, Sources::CHEBI, 3)),
                ("ADP".to_string(), hit(2, NameKind::Preferred, Sources::BIGG, 0)),
                ("g6p".to_string(), hit(3, NameKind::BiggId, Sources::BIGG, 0)),
            ],
            |s| {
                table.push(s.to_string());
                (table.len() - 1) as StrId
            },
        );
        let strings = StringTable::from_vec(table);
        assert_eq!(index.len(), 3);
        let atp = index.lookup(&strings, "atp");
        assert_eq!(atp.len(), 1);
        assert_eq!(atp[0].target, 1);
        assert_eq!(atp[0].kind, NameKind::Preferred);
        assert_eq!(atp[0].sources, Sources::BIGG | Sources::CHEBI);
        assert_eq!(atp[0].count, 3);
        assert_eq!(index.lookup(&strings, "G6P")[0].target, 3);
        assert!(index.lookup(&strings, "GTP").is_empty());
    }

    fn hit(target: u32, kind: NameKind, sources: Sources, count: u16) -> NameHit {
        NameHit {
            target,
            kind,
            sources,
            count,
        }
    }
}
