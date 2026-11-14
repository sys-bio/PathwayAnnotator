//! Reading real answers (saved 2026-11-14) from the three services.

use annot_online::{Db, Participant, exact_first, parse_chebi, parse_rhea, parse_uniprot, rank_rhea};

const OLS: &str = include_str!("fixtures/ols-fructose-6-phosphate.json");
const UNIPROT: &str = include_str!("fixtures/uniprot-phosphofructokinase.tsv");
const RHEA: &str = include_str!("fixtures/rhea-glucose-g6p.tsv");

#[test]
fn chebi_entries_with_exact_names_first() {
    let mut hits = parse_chebi(OLS, "fructose 6-phosphate").unwrap();
    assert!(hits.len() >= 3);
    assert!(hits.iter().all(|h| h.db == Db::Chebi && h.id.starts_with("CHEBI:")));
    assert_eq!(hits[0].id, "CHEBI:88003");
    assert_eq!(hits[0].uri, "https://identifiers.org/CHEBI:88003");
    // HTML is gone from definitions.
    assert!(hits.iter().filter_map(|h| h.detail.as_deref()).all(|d| !d.contains('<')));
    // "D-fructose 6-phosphate" is a synonym of two entries.
    let mut d = parse_chebi(OLS, "D-fructose 6-phosphate").unwrap();
    exact_first(&mut d);
    let exact: Vec<&str> = d.iter().filter(|h| h.exact).map(|h| h.id.as_str()).collect();
    assert!(exact.contains(&"CHEBI:57579") && exact.contains(&"CHEBI:15946"), "{exact:?}");
    assert!(d[0].exact && d[1].exact);
    // "Fructose-6-phosphate" (a synonym of CHEBI:15946) is the same name.
    hits.retain(|h| h.exact);
    let ids: Vec<&str> = hits.iter().map(|h| h.id.as_str()).collect();
    assert_eq!(ids, ["CHEBI:88003", "CHEBI:15946"]);
}

#[test]
fn uniprot_proteins() {
    let hits = parse_uniprot(UNIPROT, "PFKM").unwrap();
    assert!(!hits.is_empty());
    let m = hits.iter().find(|h| h.id == "P08237").expect("muscle PFK");
    assert_eq!(m.label, "ATP-dependent 6-phosphofructokinase, muscle type");
    assert_eq!(m.uri, "https://identifiers.org/uniprot:P08237");
    assert!(m.detail.as_deref().unwrap().contains("Homo sapiens"));
    assert!(m.exact, "the gene name is the search text");
    assert!(parse_uniprot("<html>", "x").is_err());
}

#[test]
fn rhea_reactions_ranked_by_fit() {
    assert_eq!(parse_rhea(RHEA).unwrap().len(), 11);
    // The model's hexokinase: GLC + ATP -> G6P + ADP, glucose and G6P
    // identified (ChEBI ids of every charge form), ATP and ADP currency.
    let p = |name: &str, ids: &[&str], product, currency| Participant {
        name: name.into(),
        label: None,
        chebi: ids.iter().map(|s| s.to_string()).collect(),
        product,
        currency,
    };
    let parts = [
        p("GLC", &["CHEBI:4167", "CHEBI:17634"], false, false),
        p("ATP", &["CHEBI:30616", "CHEBI:15422"], false, true),
        p("G6P", &["CHEBI:61548", "CHEBI:4170"], true, false),
        p("ADP", &["CHEBI:456216", "CHEBI:16761"], true, true),
    ];
    let currency = |id: &str| {
        ["CHEBI:15377", "CHEBI:15378", "CHEBI:30616", "CHEBI:456216", "CHEBI:456215", "CHEBI:43474"].contains(&id)
    };
    let hits = rank_rhea(RHEA, &parts, currency).unwrap();
    // Glucose and G6P on opposite sides, nothing else: hexokinase (ATP),
    // the ADP-dependent kinase, and glucose-6-phosphatase.
    let top: Vec<&str> = hits.iter().take(3).map(|h| h.id.as_str()).collect();
    for id in ["RHEA:17825", "RHEA:11460", "RHEA:16689"] {
        assert!(top.contains(&id), "{id} in {top:?}");
    }
    assert!(hits[0].exact);
    assert_eq!(hits[0].why.as_deref(), Some("Has 2 of the 2 molecules, on the same sides"));
    // A reaction with another main molecule ranks below.
    let maltose = hits.iter().find(|h| h.id == "RHEA:20421").unwrap();
    assert!(!maltose.exact);
    assert!(maltose.why.as_deref().unwrap().contains("1 other molecule"));
    assert_eq!(hits[0].uri.split('/').next_back(), Some(format!("rhea:{}", &hits[0].id[5..]).as_str()));
}

#[test]
fn a_molecule_identified_in_another_form_is_recognized_by_name() {
    // G6P -> X2 with X2 identified as keto-D-fructose 6-phosphate (an id
    // Rhea does not use); the search left X2 out and found reactions of
    // alpha-D-glucose 6-phosphate.
    let rows = "Reaction identifier\tEquation\tChEBI identifier\tEC number\n\
                RHEA:23536\talpha-D-glucose 1-phosphate = alpha-D-glucose 6-phosphate\tCHEBI:58601;CHEBI:58225\tEC:5.4.2.2\n\
                RHEA:11816\talpha-D-glucose 6-phosphate = beta-D-fructose 6-phosphate\tCHEBI:58225;CHEBI:57634\tEC:5.3.1.9\n";
    let parts = [
        Participant {
            name: "G6P".into(),
            label: Some("alpha-D-glucose 6-phosphate".into()),
            chebi: vec!["CHEBI:58225".into()],
            product: false,
            currency: false,
        },
        Participant {
            name: "X2".into(),
            label: Some("keto-D-fructose 6-phosphate".into()),
            chebi: vec!["CHEBI:15946".into()],
            product: true,
            currency: false,
        },
    ];
    let hits = rank_rhea(rows, &parts, |_| false).unwrap();
    assert_eq!(hits[0].id, "RHEA:11816");
    let why = hits[0].why.as_deref().unwrap();
    assert_eq!(
        why,
        "Has 2 of the 2 molecules (by name: beta-D-fructose 6-phosphate like keto-D-fructose 6-phosphate), on the same sides"
    );
    assert!(why.contains("same sides") && !why.contains("other molecule"), "{why}");
    assert!(hits[1].why.as_deref().unwrap().contains("1 other molecule"));
}

#[test]
fn go_cellular_components() {
    let mut hits = annot_online::parse_go(include_str!("fixtures/ols-go-mitochondria.json"), "mitochondria").unwrap();
    assert!(hits.iter().all(|h| h.db == Db::Go && h.id.starts_with("GO:")));
    exact_first(&mut hits);
    // "mitochondria" is a synonym of mitochondrion.
    assert_eq!(hits[0].id, "GO:0005739");
    assert!(hits[0].exact);
    assert_eq!(hits[0].uri, "https://identifiers.org/GO:0005739");
    assert_eq!(Db::Go.sbo(), 290);
}
