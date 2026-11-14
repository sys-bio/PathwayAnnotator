//! BioModels (biomodels.org): how curators actually name things.
//!
//! Curated BioModels pair each species' name and id with a `bqbiol:is`
//! annotation. Mined over all curated models, that gives the names modelers
//! really use ("G6P", "Glc", "PEP", "NADH", "M_g6p_c"), each with the number
//! of models that use it with that meaning. This is the dictionary behind
//! "named G6P in 12 curated BioModels".
//!
//! Like ChEBI, BioModels adds no entities or reactions. For each species and
//! reaction:
//!
//! 1. Its `bqbiol:is` and `bqbiol:isVersionOf` resources are resolved against
//!    the pack's ids (see [`kpack::split_resource`]). They must all point to
//!    one entity (for a species) or one reaction (for a reaction); an
//!    element whose annotations point to several is skipped as ambiguous.
//! 2. Its `name` and its `id` become names of that target, counted once per
//!    model.
//! 3. Noise is dropped. Ids such as `s1` or `x` mean something different in
//!    every model. A (name, target) pair is kept if it accounts for at
//!    least [`MIN_SHARE`] of the models that annotate the name at all, and
//!    if it is seen in at least [`MIN_MODELS`] models or in half of them.
//!    ("Glc" is glucose in 12 models; "s1" is glucose in 2 and calcium in 2
//!    of many more.)
//!
//! For the benchmark, [`is_test_model`] splits models into a training and a
//! test set by id; a pack built with the test set excluded can be scored on
//! the test models without having seen their names.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use serde::Deserialize;

use kpack::{NameHit, NameKind, NsId, SourceInfo, Sources, StrId, Target, XrefScope};

use crate::builder::PackBuilder;
use crate::namespaces;

pub const API: &str = "https://www.biomodels.org";
pub const INDEX_FILE: &str = "index.json";
/// The search for every manually curated model.
pub const QUERY: &str = "curationstatus%3A%22Manually%20curated%22";
/// A (name, target) pair seen in fewer models than this must account for
/// half of the name's models to be kept.
pub const MIN_MODELS: usize = 2;
/// The smallest share of a name's models a (name, target) pair needs.
pub const MIN_SHARE: f64 = 0.2;

/// Whether a (name, target) pair seen in `n` of the `total` models that
/// annotate the name is a real usage rather than noise.
pub fn keep_name(n: usize, total: usize) -> bool {
    let share = n as f64 / total.max(1) as f64;
    share >= MIN_SHARE && (n >= MIN_MODELS || share >= 0.5)
}

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Whether a model is in the benchmark's test set: every fifth model by id
/// number (BIOMD0000000005, ...0010, ...).
pub fn is_test_model(id: &str) -> bool {
    let digits: String = id.chars().filter(|c| c.is_ascii_digit()).collect();
    digits.parse::<u64>().is_ok_and(|n| n % 5 == 0)
}

// --- the API -------------------------------------------------------------------

#[derive(Deserialize)]
struct SearchPage {
    matches: usize,
    models: Vec<SearchModel>,
}

#[derive(Deserialize)]
struct SearchModel {
    id: String,
    #[serde(default)]
    format: Option<String>,
}

#[derive(Deserialize)]
struct Files {
    #[serde(default)]
    main: Vec<FileEntry>,
}

#[derive(Deserialize)]
struct FileEntry {
    name: String,
}

fn get_text(url: &str) -> Result<String> {
    let mut response = ureq::get(url).call()?;
    Ok(response
        .body_mut()
        .with_config()
        .limit(64 << 20)
        .read_to_string()?)
}

/// Lists the curated SBML models and downloads each one not already in
/// `<cache>/biomodels`. Safe to run again: it fetches only what is missing.
pub fn fetch(cache: &Path) -> Result<()> {
    let dir = cache.join("biomodels");
    std::fs::create_dir_all(&dir)?;

    let mut ids = Vec::new();
    let mut offset = 0;
    loop {
        let url = format!("{API}/search?query={QUERY}&offset={offset}&numResults=100&format=json");
        let page: SearchPage = serde_json::from_str(&get_text(&url)?)?;
        let n = page.models.len();
        for m in page.models {
            if m.format.as_deref() == Some("SBML") {
                ids.push(m.id);
            }
        }
        offset += 100;
        if n == 0 || offset >= page.matches {
            break;
        }
    }
    ids.sort();
    ids.dedup();
    eprintln!("BioModels: {} curated SBML models", ids.len());
    std::fs::write(dir.join(INDEX_FILE), serde_json::to_string_pretty(&ids)?)?;

    let (mut fetched, mut failed) = (0, 0);
    for (i, id) in ids.iter().enumerate() {
        let path = dir.join(format!("{id}.xml"));
        if path.exists() {
            continue;
        }
        match fetch_model(id, &path) {
            Ok(()) => fetched += 1,
            Err(e) => {
                failed += 1;
                eprintln!("  {id}: {e}");
            }
        }
        if (i + 1) % 100 == 0 {
            eprintln!("  {} of {} done", i + 1, ids.len());
        }
        // Be polite to a free service.
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    eprintln!("BioModels: {fetched} downloaded, {failed} failed");
    std::fs::write(dir.join("fetched.txt"), crate::date::today())?;
    Ok(())
}

fn fetch_model(id: &str, path: &Path) -> Result<()> {
    let files: Files = serde_json::from_str(&get_text(&format!("{API}/model/files/{id}?format=json"))?)?;
    let main = files
        .main
        .iter()
        .find(|f| f.name.ends_with(".xml"))
        .ok_or("no SBML file")?;
    crate::fetch::download(
        &format!("{API}/model/download/{id}?filename={}", query_escape(&main.name)),
        path,
    )?;
    Ok(())
}

/// Escapes a query value: some model files are named "Alharbi2019 TNM.xml".
fn query_escape(s: &str) -> String {
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

// --- mining a model ------------------------------------------------------------

/// What an annotated element is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ElementKind {
    Species,
    Reaction,
}

/// One species or reaction and what it is annotated as.
#[derive(Clone, Debug, PartialEq)]
pub struct Element {
    pub kind: ElementKind,
    pub id: String,
    pub name: Option<String>,
    /// Resources of its `bqbiol:is` and `bqbiol:isVersionOf` terms.
    pub resources: Vec<String>,
}

/// The annotated elements of one model.
#[derive(Clone, Debug, PartialEq)]
pub struct MinedModel {
    pub id: String,
    pub elements: Vec<Element>,
}

/// Reads one SBML model's species and reactions with their annotations.
/// Elements with no `is` or `isVersionOf` term are left out.
pub fn mine(model_id: &str, xml: &str) -> std::result::Result<MinedModel, String> {
    let read = sbml_io::read(xml).map_err(|d| {
        d.first()
            .map(|d| format!("{d:?}"))
            .unwrap_or_else(|| "unreadable".into())
    })?;
    let doc = read.document;
    let mut out = MinedModel {
        id: model_id.to_string(),
        elements: Vec::new(),
    };
    let Some(model) = &doc.model else {
        return Ok(out);
    };
    let resources = |meta: &sbml_io::Meta| -> Vec<String> {
        let Some(xml) = &meta.annotation else {
            return Vec::new();
        };
        sbml_io::miriam::read_annotation(xml, &doc.namespaces)
            .terms
            .into_iter()
            .filter(|t| !t.model && (t.qualifier == "is" || t.qualifier == "isVersionOf"))
            .flat_map(|t| t.resources)
            .collect()
    };
    for s in &model.species {
        let r = resources(&s.meta);
        if !r.is_empty() {
            out.elements.push(Element {
                kind: ElementKind::Species,
                id: s.id.clone(),
                name: s.name.clone(),
                resources: r,
            });
        }
    }
    for x in &model.reactions {
        let r = resources(&x.meta);
        if !r.is_empty() {
            out.elements.push(Element {
                kind: ElementKind::Reaction,
                id: x.id.clone(),
                name: x.name.clone(),
                resources: r,
            });
        }
    }
    Ok(out)
}

// --- into the pack -------------------------------------------------------------

/// What [`ingest`] did.
#[derive(Debug, Default)]
pub struct IngestStats {
    pub models: usize,
    /// Annotated species and reactions seen.
    pub elements: usize,
    /// Elements whose annotations led to exactly one entity or reaction.
    pub resolved: usize,
    /// Elements whose annotations led to several, with no clear winner.
    pub ambiguous: usize,
    /// Of those, reactions (an EC number alone fits many reactions).
    pub ambiguous_reactions: usize,
    /// Elements whose annotations led to nothing in the pack (proteins,
    /// genes, and molecules BiGG does not have).
    pub unresolved: usize,
    /// Resources by namespace as written that matched no known namespace.
    pub unknown_namespaces: BTreeMap<String, usize>,
    /// (name, target) pairs kept and dropped as noise.
    pub names_kept: usize,
    pub names_dropped: usize,
}

/// A target and which models named it so.
#[derive(Default)]
struct Uses {
    models: BTreeSet<usize>,
    /// The strongest kind it was used as (a `name` beats an `id`), and one
    /// spelling of it.
    kind: Option<NameKind>,
    spelling: String,
}

/// Adds the names curated models use to the entities and reactions already
/// in the builder.
pub fn ingest(b: &mut PackBuilder, models: &[MinedModel]) -> IngestStats {
    let mut stats = IngestStats {
        models: models.len(),
        ..IngestStats::default()
    };
    let index = b.xref_targets();
    // normalized name -> target -> uses
    let mut uses: BTreeMap<String, BTreeMap<Target, Uses>> = BTreeMap::new();
    // normalized name -> every model that annotates an element so named,
    // whatever the annotation leads to. "s1" is annotated in many models,
    // mostly as proteins outside the pack; those count against its noise.
    let mut annotated_in: HashMap<String, BTreeSet<usize>> = HashMap::new();

    for (mi, m) in models.iter().enumerate() {
        for el in &m.elements {
            stats.elements += 1;
            for name in el.name.iter().chain([&el.id]) {
                let key = kpack::normalize_name(name);
                if !key.is_empty() {
                    annotated_in.entry(key).or_default().insert(mi);
                }
            }
            let target = match resolve_element(b, &index, el, &mut stats) {
                Resolved::One(t) => t,
                Resolved::Many => {
                    stats.ambiguous += 1;
                    if el.kind == ElementKind::Reaction {
                        stats.ambiguous_reactions += 1;
                    }
                    continue;
                }
                Resolved::None => {
                    stats.unresolved += 1;
                    continue;
                }
            };
            stats.resolved += 1;
            let names = el
                .name
                .iter()
                .map(|n| (n.as_str(), NameKind::Synonym))
                .chain([(el.id.as_str(), NameKind::ModelId)]);
            for (name, kind) in names {
                let key = kpack::normalize_name(name);
                if key.is_empty() {
                    continue;
                }
                let u = uses.entry(key).or_default().entry(target).or_default();
                u.models.insert(mi);
                if u.kind.is_none_or(|k| kind < k) {
                    u.kind = Some(kind);
                    u.spelling = name.to_string();
                }
            }
        }
    }

    for (key, targets) in &uses {
        let total = annotated_in.get(key).map_or(0, BTreeSet::len);
        for (target, u) in targets {
            let n = u.models.len();
            if !keep_name(n, total) {
                stats.names_dropped += 1;
                continue;
            }
            stats.names_kept += 1;
            let (id, is_reaction) = match *target {
                Target::Entity(e) => (e, false),
                Target::Reaction(r) => (r, true),
            };
            let hit = NameHit {
                target: id,
                kind: u.kind.unwrap_or(NameKind::Synonym),
                sources: Sources::BIOMODELS,
                count: u16::try_from(n).unwrap_or(u16::MAX),
            };
            if is_reaction {
                b.name_reaction(&u.spelling, hit);
            } else {
                b.name_entity(&u.spelling, hit);
            }
        }
    }
    stats
}

enum Resolved {
    One(Target),
    Many,
    None,
}

/// The one entity (for a species) or reaction (for a reaction) an element's
/// resources lead to. Exact matches win over broader or narrower ones.
/// Each resource votes for every target it leads to, and the target with
/// the most votes wins: a species annotated with CHEBI:4167 (BiGG's
/// D-glucopyranose only) and KEGG C00031 (shared by several glucose forms in
/// BiGG) is D-glucopyranose. A tie is ambiguous.
fn resolve_element(
    b: &PackBuilder,
    index: &HashMap<(NsId, StrId), Vec<(Target, XrefScope)>>,
    el: &Element,
    stats: &mut IngestStats,
) -> Resolved {
    let mut exact: BTreeMap<Target, usize> = BTreeMap::new();
    let mut related: BTreeMap<Target, usize> = BTreeMap::new();
    for uri in &el.resources {
        let Some((written, id)) = kpack::split_resource(uri) else {
            continue;
        };
        let Some((def, id)) = namespaces::resolve(&written, &id) else {
            *stats.unknown_namespaces.entry(written).or_default() += 1;
            continue;
        };
        let Some(sid) = b.strings.find(&id) else {
            continue;
        };
        // One vote per resource for each target, even if the resource
        // reaches it twice.
        let mut voted = BTreeSet::new();
        for &(target, scope) in index.get(&(b.ns(def.prefix), sid)).into_iter().flatten() {
            let fits = matches!(
                (el.kind, target),
                (ElementKind::Species, Target::Entity(_)) | (ElementKind::Reaction, Target::Reaction(_))
            );
            if !fits || !voted.insert((target, scope == XrefScope::Exact)) {
                continue;
            }
            let tally = if scope == XrefScope::Exact { &mut exact } else { &mut related };
            *tally.entry(target).or_default() += 1;
        }
    }
    let votes = if exact.is_empty() { related } else { exact };
    let Some(&best) = votes.values().max() else {
        return Resolved::None;
    };
    let mut winners = votes.iter().filter(|(_, n)| **n == best);
    match (winners.next(), winners.next()) {
        (Some((t, _)), None) => Resolved::One(*t),
        _ => Resolved::Many,
    }
}

// --- from the cache ------------------------------------------------------------

/// What [`load`] read.
#[derive(Debug, Default)]
pub struct LoadStats {
    pub listed: usize,
    pub missing: usize,
    pub unreadable: Vec<(String, String)>,
    pub held_out: usize,
    pub ingest: IngestStats,
}

/// Mines the cached models in `dir` and adds their names to the pack. With
/// `exclude_test`, models in the benchmark's test set are left out.
pub fn load(b: &mut PackBuilder, dir: &Path, version: &str, exclude_test: bool) -> Result<LoadStats> {
    let index = std::fs::read_to_string(dir.join(INDEX_FILE))
        .map_err(|e| format!("cannot read {}: {e}; run `kpack-build fetch` first", dir.join(INDEX_FILE).display()))?;
    let ids: Vec<String> = serde_json::from_str(&index)?;
    let mut stats = LoadStats {
        listed: ids.len(),
        ..LoadStats::default()
    };
    let mut models = Vec::new();
    for id in &ids {
        if exclude_test && is_test_model(id) {
            stats.held_out += 1;
            continue;
        }
        let Ok(xml) = std::fs::read_to_string(dir.join(format!("{id}.xml"))) else {
            stats.missing += 1;
            continue;
        };
        match mine(id, &xml) {
            Ok(m) => models.push(m),
            Err(e) => stats.unreadable.push((id.clone(), e)),
        }
    }
    stats.ingest = ingest(b, &models);
    let split = if exclude_test { ", test set excluded" } else { "" };
    b.add_source(SourceInfo {
        name: "BioModels".into(),
        version: format!("{version}, {} curated models{split}", models.len()),
        url: API.into(),
        license: "CC0 1.0".into(),
        notice: None,
    });
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_rule() {
        assert!(keep_name(12, 18), "Glc is glucose in 12 of 18");
        assert!(keep_name(1, 1), "a name used once, consistently");
        assert!(keep_name(1, 2), "half");
        assert!(!keep_name(1, 3), "one of three");
        assert!(!keep_name(2, 20), "s1: two of twenty");
        assert!(keep_name(4, 20), "a fifth");
    }

    #[test]
    fn test_split() {
        assert!(is_test_model("BIOMD0000000005"));
        assert!(is_test_model("BIOMD0000000010"));
        assert!(!is_test_model("BIOMD0000000012"));
        assert!(!is_test_model("nonsense"));
    }

    const MODEL: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<sbml xmlns="http://www.sbml.org/sbml/level2/version4" level="2" version="4">
  <model id="m">
    <listOfCompartments><compartment id="cell" size="1"/></listOfCompartments>
    <listOfSpecies>
      <species metaid="m1" id="G6P" name="glucose 6-phosphate" compartment="cell" initialConcentration="1">
        <annotation>
          <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#" xmlns:bqbiol="http://biomodels.net/biology-qualifiers/">
            <rdf:Description rdf:about="#m1">
              <bqbiol:is><rdf:Bag><rdf:li rdf:resource="urn:miriam:obo.chebi:CHEBI%3A4170"/></rdf:Bag></bqbiol:is>
            </rdf:Description>
          </rdf:RDF>
        </annotation>
      </species>
      <species id="plain" compartment="cell" initialConcentration="1"/>
    </listOfSpecies>
  </model>
</sbml>"##;

    #[test]
    fn mining_finds_annotated_species() {
        let m = mine("BIOMD0000000001", MODEL).unwrap();
        assert_eq!(m.elements.len(), 1, "the unannotated species is left out");
        let e = &m.elements[0];
        assert_eq!(e.kind, ElementKind::Species);
        assert_eq!(e.id, "G6P");
        assert_eq!(e.name.as_deref(), Some("glucose 6-phosphate"));
        assert_eq!(e.resources, ["urn:miriam:obo.chebi:CHEBI%3A4170"]);
    }
}
