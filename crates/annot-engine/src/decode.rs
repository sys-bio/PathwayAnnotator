//! The name decoder: other ways to read a name or id that the dictionary
//! does not know as written.
//!
//! Modelers wrap molecule names in location words and SBML conventions:
//! "Glucose in", "External Pyruvate", "ATP_mit", "M_g6p_c", "Glcout",
//! "Ca2+", "Red. Equ. (NADH)". Each rule here undoes one such habit, and
//! [`readings`] applies them in turn, so "M_glc__D_c" becomes "glc__D_c",
//! then "glc D". Every reading records the steps that made it and a factor
//! (the product of the steps' factors) by which a dictionary hit on it is
//! trusted less than a hit on the name as written.

use std::collections::HashSet;

use kpack::Pack;

use crate::model::Model;

/// One way of changing a name.
#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    /// An SBML id prefix: `M_`, `R_`, `S_`.
    Prefix(String),
    /// Location words dropped from either end: "in", "external", "mit",
    /// the model's own compartment names.
    Location(String),
    /// A location ending written without a separator: "Glcin" -> "Glc".
    JoinedLocation(String),
    /// A bare charge: "Ca2+" -> "Ca".
    Charge(String),
    /// The text inside, or before, brackets.
    Bracket,
    /// A stereo prefix added or removed: "D-".
    Stereo(String),
    /// One word of a name of two or three.
    Word,
}

impl Step {
    /// How much a reading made by this step is trusted, relative to the
    /// name as written.
    pub fn factor(&self) -> f32 {
        match self {
            Step::Prefix(_) => 0.95,
            Step::Charge(_) => 0.9,
            Step::Location(_) => 0.85,
            Step::Stereo(_) => 0.8,
            Step::JoinedLocation(_) | Step::Bracket => 0.7,
            Step::Word => 0.5,
        }
    }

    /// For people: `dropped "in"`.
    pub fn describe(&self) -> String {
        match self {
            Step::Prefix(p) => format!("dropped the SBML prefix \"{p}\""),
            Step::Location(w) => format!("dropped the location \"{w}\""),
            Step::JoinedLocation(w) => format!("dropped the location ending \"{w}\""),
            Step::Charge(c) => format!("dropped the charge \"{c}\""),
            Step::Bracket => "took the part in brackets".into(),
            Step::Stereo(s) => s.clone(),
            Step::Word => "took one word".into(),
        }
    }
}

/// A way of reading a name.
#[derive(Clone, Debug, PartialEq)]
pub struct Reading {
    pub text: String,
    pub steps: Vec<Step>,
    pub factor: f32,
}

impl Reading {
    /// For people: `dropped the location "in"; took one word`.
    pub fn describe(&self) -> String {
        self.steps.iter().map(Step::describe).collect::<Vec<_>>().join("; ")
    }
}

/// Location words that appear in names.
pub const LOCATION_WORDS: &[&str] = &[
    "in", "out", "inside", "outside", "internal", "external", "intracellular", "extracellular",
    "ex", "ext", "int", "cyt", "cyto", "cytosol", "cytosolic", "cytoplasm", "cytoplasmic", "mit",
    "mito", "mitochondria", "mitochondrial", "mitochondrion", "mt", "cell", "cellular", "plasma",
    "medium", "media", "nuc", "nuclear", "nucleus", "er", "pm", "tot", "total", "free", "mat",
    "matrix", "ims",
];

/// Location endings written straight onto a name ("Glcin", "Pyrex",
/// "CarMAT").
pub const JOINED_ENDINGS: &[&str] = &[
    "in", "out", "ex", "ext", "int", "cyt", "cyto", "mit", "mito", "cell", "nuc", "mat",
];

/// Words too generic to stand for a molecule on their own: "Indoleglycerol
/// phosphate" is not phosphate.
pub const GENERIC_WORDS: &[&str] = &[
    "phosphate", "acid", "ion", "ions", "salt", "sugar", "complex", "active", "inactive", "bound",
    "pool", "form", "residue", "group", "ester", "protein", "enzyme",
];

/// Database ids that turn up inside SBML ids ("M_C03506_cytop"), with the
/// namespace each pattern belongs to, for species and for reactions.
pub fn embedded_ids(text: &str, reaction: bool) -> Vec<(&'static str, String)> {
    let mut out = Vec::new();
    // "CHEBI_15422" is one id, though "_" separates words.
    let text = text.replace("CHEBI_", "CHEBI");
    for t in tokens(&text) {
        let digits_after = |p: &str, n: Option<usize>| {
            t.strip_prefix(p).is_some_and(|d| {
                !d.is_empty() && d.chars().all(|c| c.is_ascii_digit()) && n.is_none_or(|n| d.len() == n)
            })
        };
        let found = if reaction {
            if digits_after("R", Some(5)) {
                Some(("kegg.reaction", t.to_string()))
            } else if digits_after("rxn", Some(5)) {
                Some(("seed.reaction", t.to_string()))
            } else if digits_after("MNXR", None) {
                Some(("metanetx.reaction", t.to_string()))
            } else {
                None
            }
        } else if digits_after("C", Some(5)) {
            Some(("kegg.compound", t.to_string()))
        } else if digits_after("cpd", Some(5)) {
            Some(("seed.compound", t.to_string()))
        } else if digits_after("MNXM", None) {
            Some(("metanetx.chemical", t.to_string()))
        } else if digits_after("CHEBI", None) {
            Some(("chebi", format!("CHEBI:{}", &t[5..])))
        } else {
            None
        };
        out.extend(found);
    }
    out
}

/// Id prefixes from SBML converters (BiGG/COBRA models).
pub const SBML_PREFIXES: &[&str] = &["M_", "R_", "S_"];

/// The words that mean a location in this model.
pub struct Vocab {
    /// Lowercase words of two letters or more.
    words: HashSet<String>,
    /// Short compartment codes (BiGG's `c`, `m`, `e`, ...), recognized in ids
    /// only: "vitamin C" is not vitamin in the cytosol.
    codes: HashSet<String>,
}

impl Vocab {
    /// The built-in words, the pack's compartment names and codes, and the
    /// model's own compartment ids and names.
    pub fn new(pack: &Pack, model: &Model) -> Vocab {
        let mut v = Vocab {
            words: LOCATION_WORDS.iter().map(|w| w.to_string()).collect(),
            codes: HashSet::new(),
        };
        for c in &pack.compartments {
            v.codes.insert(pack.str(c.code).to_ascii_lowercase());
            v.add_words(pack.str(c.label));
            for s in &c.synonyms {
                v.add_words(pack.str(*s));
            }
        }
        for c in &model.compartments {
            if c.id.chars().count() <= 2 {
                v.codes.insert(c.id.to_lowercase());
            }
            v.add_words(&c.id);
            if let Some(n) = &c.name {
                v.add_words(n);
            }
        }
        v
    }

    fn add_words(&mut self, text: &str) {
        for w in tokens(text) {
            if w.chars().count() >= 2 {
                self.words.insert(w.to_lowercase());
            }
        }
    }

    fn is_location(&self, token: &str, in_id: bool) -> bool {
        let t = token.to_lowercase();
        (t.chars().count() >= 2 && self.words.contains(&t)) || (in_id && self.codes.contains(&t))
    }
}

/// Words of a name or id: split on underscores and white space. Hyphens and
/// commas stay ("D-glucose", "1,6-bisphosphate").
fn tokens(text: &str) -> Vec<&str> {
    text.split(|c: char| c == '_' || c.is_whitespace())
        .filter(|t| !t.is_empty())
        .collect()
}

/// At most this many readings per name.
const MAX_READINGS: usize = 24;

/// The other readings of `text` (an element's name, or its id if `in_id`),
/// best trusted first. The text as written is not among them.
pub fn readings(text: &str, in_id: bool, vocab: &Vocab) -> Vec<Reading> {
    let original = text.trim();
    let mut out = vec![Reading {
        text: original.to_string(),
        steps: Vec::new(),
        factor: 1.0,
    }];
    if in_id {
        apply(&mut out, |t| {
            SBML_PREFIXES
                .iter()
                .filter_map(|p| t.strip_prefix(p).map(|rest| (rest.to_string(), Step::Prefix(p.to_string()))))
                .collect()
        });
    }
    apply(&mut out, |t| drop_locations(t, in_id, vocab).into_iter().collect());
    apply(&mut out, |t| joined_location(t).into_iter().collect());
    apply(&mut out, |t| drop_charge(t).into_iter().collect());
    apply(&mut out, brackets);
    if !in_id {
        apply(&mut out, stereo);
    }
    apply(&mut out, |t| single_words(t, in_id, vocab));
    out.remove(0);
    out.sort_by(|a, b| b.factor.total_cmp(&a.factor));
    out
}

/// Adds, for every reading so far, the readings `f` makes of it. A reading
/// whose normalized text is already present is not added again.
fn apply(out: &mut Vec<Reading>, f: impl Fn(&str) -> Vec<(String, Step)>) {
    let mut seen: HashSet<String> = out.iter().map(|r| kpack::normalize_name(&r.text)).collect();
    let current = out.clone();
    for r in &current {
        for (text, step) in f(&r.text) {
            let key = kpack::normalize_name(&text);
            if key.is_empty() || out.len() >= MAX_READINGS || !seen.insert(key) {
                continue;
            }
            let mut steps = r.steps.clone();
            let factor = r.factor * step.factor();
            steps.push(step);
            out.push(Reading { text, steps, factor });
        }
    }
}

/// "Glucose in" -> "Glucose"; "iron_in_Plasma" -> "iron"; "M_g6p_c" ->
/// "g6p" (in an id). Location words come off either end until a word that
/// is not one; at least one word always stays.
fn drop_locations(text: &str, in_id: bool, v: &Vocab) -> Option<(String, Step)> {
    let t = tokens(text);
    if t.len() < 2 {
        return None;
    }
    let (mut lo, mut hi) = (0, t.len());
    let mut dropped = Vec::new();
    loop {
        let mut changed = false;
        if hi - lo > 1 && v.is_location(t[hi - 1], in_id) {
            dropped.push(t[hi - 1]);
            hi -= 1;
            changed = true;
        }
        if hi - lo > 1 && v.is_location(t[lo], in_id) {
            dropped.push(t[lo]);
            lo += 1;
            changed = true;
        }
        if !changed {
            break;
        }
    }
    (!dropped.is_empty()).then(|| (t[lo..hi].join(" "), Step::Location(dropped.join(", "))))
}

/// "Glcin" -> "Glc", "Pyrex" -> "Pyr". Only for a single word, and only if
/// at least two characters remain.
fn joined_location(text: &str) -> Option<(String, Step)> {
    if tokens(text).len() != 1 {
        return None;
    }
    let lower = text.to_lowercase();
    JOINED_ENDINGS.iter().find_map(|end| {
        let cut = lower.len().checked_sub(end.len())?;
        let rest = text.get(..cut)?;
        (lower.ends_with(end) && rest.chars().count() >= 2 && rest.chars().any(char::is_alphabetic))
            .then(|| (rest.to_string(), Step::JoinedLocation(end.to_string())))
    })
}

/// "Ca2+" -> "Ca", "H+" -> "H", "Cl-" -> "Cl".
fn drop_charge(text: &str) -> Option<(String, Step)> {
    let signs = text.trim_end_matches(['+', '-']);
    if signs.len() == text.len() {
        return None;
    }
    let rest = signs.trim_end_matches(|c: char| c.is_ascii_digit());
    (rest.chars().any(char::is_alphabetic)).then(|| (rest.to_string(), Step::Charge(text[rest.len()..].to_string())))
}

/// "Red. Equ. (NADH)" -> "NADH" and "Red. Equ."
fn brackets(text: &str) -> Vec<(String, Step)> {
    let Some(body) = text.trim_end().strip_suffix(')') else {
        return Vec::new();
    };
    let Some(open) = body.rfind('(') else {
        return Vec::new();
    };
    let inner = body[open + 1..].trim();
    let outer = body[..open].trim();
    let mut out = Vec::new();
    // A charge in brackets ("ATP(4-)") is the dictionary's business.
    if inner.chars().filter(|c| c.is_alphabetic()).count() >= 2 {
        out.push((inner.to_string(), Step::Bracket));
    }
    if outer.chars().filter(|c| c.is_alphabetic()).count() >= 2 {
        out.push((outer.to_string(), Step::Bracket));
    }
    out
}

/// "fructose 1,6-bisphosphate" -> "D-fructose 1,6-bisphosphate" (and L-);
/// "D-fructose" -> "fructose".
fn stereo(text: &str) -> Vec<(String, Step)> {
    let lower = text.to_lowercase();
    for p in ["d-", "l-", "alpha-", "beta-"] {
        if lower.starts_with(p) && text.len() > p.len() + 2 {
            return vec![(text[p.len()..].to_string(), Step::Stereo(format!("removed \"{}\"", &text[..p.len()])))];
        }
    }
    let starts_with_letter = text.chars().next().is_some_and(char::is_alphabetic);
    if !starts_with_letter || text.chars().count() < 4 {
        return Vec::new();
    }
    ["D-", "L-"]
        .iter()
        .map(|p| (format!("{p}{text}"), Step::Stereo(format!("added \"{p}\""))))
        .collect()
}

/// Each word of a name of two or three words that are not locations:
/// "DMNB cAMP" -> "DMNB", "cAMP".
fn single_words(text: &str, in_id: bool, v: &Vocab) -> Vec<(String, Step)> {
    let t = tokens(text);
    if !(2..=3).contains(&t.len()) {
        return Vec::new();
    }
    t.iter()
        .filter(|w| w.chars().count() >= 2 && !v.is_location(w, in_id))
        .filter(|w| !GENERIC_WORDS.contains(&w.to_lowercase().as_str()))
        .map(|w| (w.to_string(), Step::Word))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Compartment;

    fn vocab() -> Vocab {
        let model = Model {
            compartments: vec![
                Compartment {
                    id: "Cytosol".into(),
                    name: None,
                },
                Compartment {
                    id: "c".into(),
                    name: Some("Plasma".into()),
                },
            ],
            ..Model::default()
        };
        Vocab::new(&Pack::default(), &model)
    }

    fn texts(text: &str, in_id: bool) -> Vec<String> {
        readings(text, in_id, &vocab()).into_iter().map(|r| r.text).collect()
    }

    fn has(text: &str, in_id: bool, want: &str) {
        let got = texts(text, in_id);
        assert!(got.iter().any(|t| t == want), "{text:?}: {got:?} lacks {want:?}");
    }

    #[test]
    fn location_words() {
        has("Glucose in", false, "Glucose");
        has("External Pyruvate", false, "Pyruvate");
        has("ATP_mit", false, "ATP");
        has("O2_ex", true, "O2");
        has("iron_in_Plasma", false, "iron");
        // The model's own compartment name.
        has("IP3X_Cytosol", false, "IP3X");
    }

    #[test]
    fn sbml_ids() {
        // Prefix, then the compartment code (an id, so codes count).
        has("M_g6p_c", true, "g6p_c");
        has("M_g6p_c", true, "g6p");
        has("M_glc__D_c", true, "glc D");
        has("R_PFK", true, "PFK");
    }

    #[test]
    fn codes_only_in_ids() {
        // "vitamin C" in a name keeps its C: no reading drops it as a
        // location.
        let r = readings("vitamin C", false, &vocab());
        assert!(!r.iter().any(|r| r.steps.iter().any(|s| matches!(s, Step::Location(_)))), "{r:?}");
        has("vitamin_c", true, "vitamin");
    }

    #[test]
    fn joined_endings() {
        has("Glcin", false, "Glc");
        has("Glcout", false, "Glc");
        has("Pyrex", false, "Pyr");
        assert!(texts("in", false).is_empty());
    }

    #[test]
    fn charges_brackets_stereo_words() {
        has("Ca2+", false, "Ca");
        has("Fe3+", false, "Fe");
        // "H+" needs no reading: the dictionary already ignores the "+".
        assert!(texts("H+", false).is_empty());
        has("Red. Equ. (NADH)", false, "NADH");
        has("fructose 1,6-bisphosphate", false, "D-fructose 1,6-bisphosphate");
        has("D-fructose", false, "fructose");
        has("DMNB_cAMP_cell", false, "cAMP");
        has("CarMAT", false, "Car");
        // Generic words do not stand alone.
        assert!(!texts("Indoleglycerol phosphate", false).iter().any(|t| t == "phosphate"));
    }

    #[test]
    fn database_ids_inside_sbml_ids() {
        assert_eq!(embedded_ids("M_C03506_cytop", false), [("kegg.compound", "C03506".to_string())]);
        assert_eq!(embedded_ids("cpd00027_c0", false), [("seed.compound", "cpd00027".to_string())]);
        assert_eq!(embedded_ids("CHEBI_15422", false), [("chebi", "CHEBI:15422".to_string())]);
        assert_eq!(embedded_ids("R_R00299_c", true), [("kegg.reaction", "R00299".to_string())]);
        // Not ids: wrong length, or a reaction pattern on a species.
        assert!(embedded_ids("C123", false).is_empty());
        assert!(embedded_ids("R00299", false).is_empty());
    }

    #[test]
    fn the_original_is_not_a_reading_and_trust_falls_with_steps() {
        let r = readings("M_g6p_c", true, &vocab());
        assert!(r.iter().all(|r| r.text != "M_g6p_c"));
        let one = r.iter().find(|r| r.text == "g6p_c").unwrap();
        let two = r.iter().find(|r| r.text == "g6p").unwrap();
        assert!(one.factor > two.factor);
        assert_eq!(two.steps.len(), 2);
        assert!(two.describe().contains("SBML prefix"));
    }
}
