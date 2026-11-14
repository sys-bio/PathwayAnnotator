//! BiGG's compartment codes and what they mean.

/// One compartment code.
#[derive(Clone, Copy, Debug)]
pub struct CompDef {
    pub code: &'static str,
    pub label: &'static str,
    /// GO cellular component, where the mapping is certain.
    pub go: Option<&'static str>,
    /// Other names modelers use for it.
    pub synonyms: &'static [&'static str],
}

/// The codes BiGG uses (bigg.bio/api/v3/compartments, 2026-11-13: 22
/// codes). Labels follow BiGG's names. GO terms are given only where the
/// mapping is certain; the rest are left for review. A code not listed here
/// still gets a compartment, with the code as its label and no GO term.
pub const KNOWN: &[CompDef] = &[
    comp("c", "cytosol", Some("GO:0005829"), &["cytoplasm", "cyt", "cyto", "cell", "intracellular"]),
    comp("e", "extracellular space", Some("GO:0005576"), &["extracellular", "external", "ext", "medium", "out"]),
    comp("p", "periplasm", Some("GO:0042597"), &["periplasmic space", "peri"]),
    comp("m", "mitochondrion", Some("GO:0005739"), &["mitochondria", "mito", "mt", "mit"]),
    comp("n", "nucleus", Some("GO:0005634"), &["nuclear", "nuc"]),
    comp("r", "endoplasmic reticulum", Some("GO:0005783"), &["er", "reticulum"]),
    comp("x", "peroxisome/glyoxysome", Some("GO:0005777"), &["peroxisome", "glyoxysome", "perox"]),
    comp("l", "lysosome", Some("GO:0005764"), &["lys"]),
    comp("g", "Golgi apparatus", Some("GO:0005794"), &["golgi"]),
    comp("h", "chloroplast", Some("GO:0009507"), &["chl", "plastid"]),
    comp("u", "thylakoid", Some("GO:0009579"), &[]),
    comp("v", "vacuole", Some("GO:0005773"), &["vac"]),
    comp("im", "intermembrane space of mitochondria", Some("GO:0005758"), &[]),
    comp("mm", "mitochondrial intermembrane", Some("GO:0005758"), &[]),
    comp("um", "thylakoid membrane", Some("GO:0042651"), &[]),
    comp("cx", "carboxysome", Some("GO:0031470"), &["carboxyzome"]),
    // Not mapped to GO yet.
    comp("f", "flagellum", None, &[]),
    comp("s", "eyespot", None, &[]),
    comp("cm", "cytosolic membrane", None, &[]),
    comp("i", "inner mitochondrial compartment", None, &[]),
    comp("y", "cytochrome complex", None, &[]),
    // BiGG really does use `w` for this; it is not "cell wall".
    comp("w", "wildtype staph aureus", None, &[]),
];

const fn comp(
    code: &'static str,
    label: &'static str,
    go: Option<&'static str>,
    synonyms: &'static [&'static str],
) -> CompDef {
    CompDef {
        code,
        label,
        go,
        synonyms,
    }
}

pub fn by_code(code: &str) -> Option<&'static CompDef> {
    KNOWN.iter().find(|c| c.code == code)
}
