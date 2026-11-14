//! BiGG (bigg.bio): metabolites and reactions from the bulk JSON downloads.
//!
//! The files:
//! - `metabolites.json`: one entry per charge form of a metabolite
//!   (`atp:0`, `atp:-3`, `atp:-4`), each naming its universal metabolite
//!   (`atp`) and carrying its own cross-references (ATP and ATP(4-) are
//!   different ChEBI entries). The pack has one entity per universal
//!   metabolite, with the forms' ids merged.
//! - `reactions.json`: one entry per reaction copy (`ACOAD8f:2`). Copies of a
//!   universal reaction are merged. Participants are compartmentalized
//!   metabolites: `glc__D_c` is `glc__D` in compartment `c`.
//!
//! Parsing ([`parse_metabolites`], [`parse_reactions`]) and adding to the
//! pack ([`ingest`]) are separate, so each can be tested on its own.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use serde::Deserialize;

use kpack::{
    Entity, EntityFlags, EntityId, EntityKind, NameHit, NameKind, Participant, Reaction,
    ReactionFlags, SourceInfo, Sources,
};

use crate::builder::{PackBuilder, merge_xref};
use crate::namespaces;

pub const METABOLITES_FILE: &str = "metabolites.json";
pub const REACTIONS_FILE: &str = "reactions.json";
pub const METABOLITES_URL: &str = "https://bigg.bio/api/v3/download/metabolites";
pub const REACTIONS_URL: &str = "https://bigg.bio/api/v3/download/reactions";

/// Metabolites kinetic models often leave out of reactions: protons, water,
/// energy and redox cofactors, inorganic ions. The matcher treats them as
/// optional. (BiGG universal ids.)
pub const CURRENCY: &[&str] = &[
    "h", "h2o", "atp", "adp", "amp", "gtp", "gdp", "nad", "nadh", "nadp", "nadph", "fad",
    "fadh2", "pi", "ppi", "co2", "o2", "nh4", "coa", "h2o2", "hco3",
];

// --- the files as they are -----------------------------------------------------

#[derive(Deserialize)]
struct RawXref {
    namespace: String,
    identifier: String,
    #[serde(default)]
    source_obsolete: bool,
}

#[derive(Deserialize)]
struct RawMetabolite {
    bigg_id: String,
    name: Option<String>,
    formula: Option<String>,
    charge: Option<i32>,
    #[serde(rename = "universalcomponent__bigg_id")]
    universal_id: String,
    #[serde(rename = "universalcomponent__name")]
    universal_name: Option<String>,
    #[serde(rename = "universalcomponent__default_component")]
    default_component: Option<String>,
    #[serde(rename = "universalcomponent__old_bigg_ids", default)]
    old_ids: Vec<String>,
    #[serde(default)]
    compartmentalized_components: Vec<String>,
    #[serde(default)]
    cross_references: Vec<RawXref>,
}

#[derive(Deserialize)]
struct RawReaction {
    #[serde(rename = "universalreaction__bigg_id")]
    universal_id: String,
    #[serde(rename = "universalreaction__name")]
    name: Option<String>,
    #[serde(rename = "universalreaction__participants", default)]
    participants: Vec<(f64, String)>,
    #[serde(rename = "universalreaction__is_exchange", default)]
    is_exchange: bool,
    #[serde(rename = "universalreaction__is_pseudo", default)]
    is_pseudo: bool,
    #[serde(rename = "universalreaction__is_transport", default)]
    is_transport: bool,
    #[serde(rename = "universalreaction__old_bigg_ids", default)]
    old_ids: Vec<String>,
    #[serde(default)]
    cross_references: Vec<RawXref>,
}

// --- parsed records ------------------------------------------------------------

/// A database id as BiGG spells it: namespace `CHEBI`, id `15422`.
#[derive(Clone, Debug, PartialEq)]
pub struct BiggXref {
    pub namespace: String,
    pub id: String,
}

/// A universal metabolite and its charge forms.
#[derive(Clone, Debug, PartialEq)]
pub struct Metabolite {
    /// Universal id: `atp`, `glc__D`.
    pub id: String,
    pub name: Option<String>,
    /// The form BiGG balances reactions with: `atp:-4`.
    pub default_form: Option<String>,
    pub forms: Vec<Form>,
    pub old_ids: Vec<String>,
    /// Compartment codes it occurs in: `c`, `m`.
    pub compartments: Vec<String>,
}

/// One charge form of a metabolite.
#[derive(Clone, Debug, PartialEq)]
pub struct Form {
    /// `atp:-4`.
    pub id: String,
    pub name: Option<String>,
    pub formula: Option<String>,
    pub charge: Option<i32>,
    pub xrefs: Vec<BiggXref>,
}

/// A universal reaction.
#[derive(Clone, Debug, PartialEq)]
pub struct BiggReaction {
    /// `HEX1`.
    pub id: String,
    pub name: Option<String>,
    pub participants: Vec<Stoich>,
    pub exchange: bool,
    pub transport: bool,
    pub pseudo: bool,
    pub old_ids: Vec<String>,
    pub xrefs: Vec<BiggXref>,
}

/// One participant: `-1 glc__D_c`.
#[derive(Clone, Debug, PartialEq)]
pub struct Stoich {
    pub coef: f64,
    /// Universal metabolite id: `glc__D`.
    pub metabolite: String,
    /// Compartment code: `c`.
    pub compartment: String,
}

/// `glc__D_c` -> (`glc__D`, `c`). BiGG compartment codes contain no
/// underscore, so the code is whatever follows the last one.
pub fn split_compartment(id: &str) -> Option<(&str, &str)> {
    let (met, comp) = id.rsplit_once('_')?;
    if met.is_empty() || comp.is_empty() {
        None
    } else {
        Some((met, comp))
    }
}

fn live_xrefs(raw: Vec<RawXref>) -> Vec<BiggXref> {
    raw.into_iter()
        .filter(|x| !x.source_obsolete)
        .map(|x| BiggXref {
            namespace: x.namespace,
            id: x.identifier,
        })
        .collect()
}

/// Parses `metabolites.json`, grouping charge forms by universal metabolite,
/// in the order the file first mentions them.
pub fn parse_metabolites(json: &str) -> Result<Vec<Metabolite>, serde_json::Error> {
    let raw: Vec<RawMetabolite> = serde_json::from_str(json)?;
    let mut out: Vec<Metabolite> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    for r in raw {
        let i = *index.entry(r.universal_id.clone()).or_insert_with(|| {
            out.push(Metabolite {
                id: r.universal_id.clone(),
                name: None,
                default_form: None,
                forms: Vec::new(),
                old_ids: Vec::new(),
                compartments: Vec::new(),
            });
            out.len() - 1
        });
        let m = &mut out[i];
        if m.name.is_none() {
            m.name = r.universal_name.clone();
        }
        if m.default_form.is_none() {
            m.default_form = r.default_component.clone();
        }
        for old in r.old_ids {
            if !m.old_ids.contains(&old) {
                m.old_ids.push(old);
            }
        }
        for cc in &r.compartmentalized_components {
            // `atp_c:-4`: drop the charge, then split off the compartment.
            let id = cc.split(':').next().unwrap_or(cc);
            if let Some((_, comp)) = split_compartment(id)
                && !m.compartments.iter().any(|c| c == comp) {
                    m.compartments.push(comp.to_string());
                }
        }
        m.forms.push(Form {
            id: r.bigg_id,
            name: r.name,
            formula: r.formula,
            charge: r.charge,
            xrefs: live_xrefs(r.cross_references),
        });
    }
    Ok(out)
}

/// Parses `reactions.json`, merging copies of the same universal reaction.
pub fn parse_reactions(json: &str) -> Result<Vec<BiggReaction>, serde_json::Error> {
    let raw: Vec<RawReaction> = serde_json::from_str(json)?;
    let mut out: Vec<BiggReaction> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    for r in raw {
        let xrefs = live_xrefs(r.cross_references);
        if let Some(&i) = index.get(&r.universal_id) {
            let existing = &mut out[i];
            for x in xrefs {
                if !existing.xrefs.contains(&x) {
                    existing.xrefs.push(x);
                }
            }
            continue;
        }
        let participants = r
            .participants
            .iter()
            .filter_map(|(coef, id)| {
                split_compartment(id).map(|(met, comp)| Stoich {
                    coef: *coef,
                    metabolite: met.to_string(),
                    compartment: comp.to_string(),
                })
            })
            .collect();
        index.insert(r.universal_id.clone(), out.len());
        out.push(BiggReaction {
            id: r.universal_id,
            name: r.name,
            participants,
            exchange: r.is_exchange,
            transport: r.is_transport,
            pseudo: r.is_pseudo,
            old_ids: r.old_ids,
            xrefs,
        });
    }
    Ok(out)
}

// --- into the pack -----------------------------------------------------------

/// What [`ingest`] did.
#[derive(Debug, Default)]
pub struct IngestStats {
    pub metabolites: usize,
    pub reactions: usize,
    /// Reactions left out because a participant is not a known metabolite.
    pub skipped_reactions: usize,
    /// BiGG namespaces the pack does not know, with how many ids were dropped.
    pub unknown_namespaces: BTreeMap<String, usize>,
}

/// Adds BiGG's metabolites and reactions to the pack.
pub fn ingest(b: &mut PackBuilder, mets: &[Metabolite], rxns: &[BiggReaction]) -> IngestStats {
    let mut stats = IngestStats::default();
    let mut by_bigg: HashMap<&str, EntityId> = HashMap::new();

    for m in mets {
        let id = add_metabolite(b, m, &mut stats);
        by_bigg.insert(m.id.as_str(), id);
        stats.metabolites += 1;
    }

    'reactions: for r in rxns {
        let mut participants = Vec::with_capacity(r.participants.len());
        for p in &r.participants {
            let Some(&entity) = by_bigg.get(p.metabolite.as_str()) else {
                b.warnings.push(format!(
                    "BiGG reaction {} skipped: unknown metabolite {}",
                    r.id, p.metabolite
                ));
                stats.skipped_reactions += 1;
                continue 'reactions;
            };
            participants.push(Participant {
                entity,
                coef: p.coef as f32,
                compartment: Some(b.compartment(&p.compartment)),
            });
        }
        add_reaction(b, r, participants, &mut stats);
        stats.reactions += 1;
    }
    stats
}

fn add_metabolite(b: &mut PackBuilder, m: &Metabolite, stats: &mut IngestStats) -> EntityId {
    // BiGG's universal name is the default charge form's: "ATP(4-)",
    // "pyruvate". Without the charge it is what biologists say. (The neutral
    // form's name is worse: "pyruvic acid".)
    let label = m
        .name
        .as_deref()
        .map(|n| kpack::strip_charge_suffix(n).to_string())
        .or_else(|| m.forms.iter().find_map(|f| f.name.clone()))
        .unwrap_or_else(|| m.id.clone());
    let default = m
        .default_form
        .as_ref()
        .and_then(|d| m.forms.iter().find(|f| &f.id == d))
        .or_else(|| m.forms.first());

    let mut xrefs = vec![b.xref("bigg.metabolite", &m.id, Sources::BIGG, None)];
    for form in &m.forms {
        let charge = form.charge.and_then(|c| i8::try_from(c).ok());
        for x in &form.xrefs {
            let Some(def) = namespaces::from_bigg(&x.namespace) else {
                *stats.unknown_namespaces.entry(x.namespace.clone()).or_default() += 1;
                continue;
            };
            let xref = b.xref(def.prefix, &x.id, Sources::BIGG, charge);
            merge_xref(&mut xrefs, xref);
        }
    }

    let mut compartments: Vec<_> = m.compartments.iter().map(|c| b.compartment(c)).collect();
    compartments.sort_unstable();
    compartments.dedup();

    let mut flags = EntityFlags::default();
    if CURRENCY.contains(&m.id.as_str()) {
        flags.insert(EntityFlags::CURRENCY);
    }

    let entity = Entity {
        kind: EntityKind::SmallMolecule,
        label: b.strings.intern(&label),
        xrefs,
        formula: default
            .and_then(|f| f.formula.as_deref())
            .map(|f| b.strings.intern(f)),
        charge: default.and_then(|f| f.charge).and_then(|c| i8::try_from(c).ok()),
        flags,
        compartments,
        sources: Sources::BIGG,
        ..Entity::default()
    };
    let id = b.add_entity(entity);

    let hit = |kind| NameHit {
        target: id,
        kind,
        sources: Sources::BIGG,
        count: 0,
    };
    b.name_entity(&label, hit(NameKind::Preferred));
    if let Some(name) = &m.name {
        b.name_entity(name, hit(NameKind::Synonym));
    }
    for form in &m.forms {
        if let Some(name) = &form.name {
            b.name_entity(name, hit(NameKind::Synonym));
        }
    }
    b.name_entity(&m.id, hit(NameKind::BiggId));
    for old in &m.old_ids {
        b.name_entity(old, hit(NameKind::BiggId));
    }
    id
}

fn add_reaction(
    b: &mut PackBuilder,
    r: &BiggReaction,
    participants: Vec<Participant>,
    stats: &mut IngestStats,
) {
    let label = r.name.clone().unwrap_or_else(|| r.id.clone());
    let mut xrefs = vec![b.xref("bigg.reaction", &r.id, Sources::BIGG, None)];
    for x in &r.xrefs {
        let Some(def) = namespaces::from_bigg(&x.namespace) else {
            *stats.unknown_namespaces.entry(x.namespace.clone()).or_default() += 1;
            continue;
        };
        let xref = b.xref(def.prefix, &x.id, Sources::BIGG, None);
        merge_xref(&mut xrefs, xref);
    }
    let mut flags = ReactionFlags::default();
    if r.exchange {
        flags.insert(ReactionFlags::EXCHANGE);
    }
    if r.transport {
        flags.insert(ReactionFlags::TRANSPORT);
    }
    if r.pseudo {
        flags.insert(ReactionFlags::PSEUDO);
    }
    let reaction = Reaction {
        label: b.strings.intern(&label),
        xrefs,
        participants,
        reversible: None,
        flags,
        sources: Sources::BIGG,
        ..Reaction::default()
    };
    let id = b.add_reaction(reaction);
    let hit = |kind| NameHit {
        target: id,
        kind,
        sources: Sources::BIGG,
        count: 0,
    };
    b.name_reaction(&label, hit(NameKind::Preferred));
    // "hexokinase (D-glucose:ATP)" is also just "hexokinase". Several
    // reactions share the short name; the matcher ranks them by context.
    if let Some(short) = without_trailing_bracket(&label) {
        b.name_reaction(short, hit(NameKind::Synonym));
    }
    b.name_reaction(&r.id, hit(NameKind::BiggId));
    for old in &r.old_ids {
        b.name_reaction(old, hit(NameKind::BiggId));
    }
}

/// "hexokinase (D-glucose:ATP)" -> "hexokinase". `None` if there is no
/// trailing bracket, or nothing before it.
fn without_trailing_bracket(name: &str) -> Option<&str> {
    let body = name.trim_end().strip_suffix(')')?;
    let open = body.rfind('(')?;
    let short = body[..open].trim_end();
    (!short.is_empty()).then_some(short)
}

// --- from the cache ----------------------------------------------------------

/// The license notice BiGG's legacy content requires in all copies.
///
/// TODO before publishing a pack: replace with the verbatim copyright notice
/// and license paragraphs from https://bigg.ucsd.edu/license.
pub const LEGACY_NOTICE: &str = "Contains data from BiGG Models. Copyright (c) 2019 The Regents of \
     the University of California. Licensed for educational, research and non-profit purposes; \
     see https://bigg.ucsd.edu/license for the full terms.";

/// Parses the cached BiGG files in `dir` and adds them to the pack.
/// `version` is recorded in the pack header (the download date).
pub fn load(
    b: &mut PackBuilder,
    dir: &Path,
    version: &str,
) -> Result<IngestStats, Box<dyn std::error::Error>> {
    let read = |name: &str| {
        std::fs::read_to_string(dir.join(name))
            .map_err(|e| format!("cannot read {}: {e}", dir.join(name).display()))
    };
    let mets = parse_metabolites(&read(METABOLITES_FILE)?)?;
    let rxns = parse_reactions(&read(REACTIONS_FILE)?)?;
    let stats = ingest(b, &mets, &rxns);
    b.add_source(SourceInfo {
        name: "BiGG".into(),
        version: version.into(),
        url: "https://bigg.bio".into(),
        license: "CC BY 4.0 (BRIGHT data); BiGG Models license (legacy UCSD content)".into(),
        notice: Some(LEGACY_NOTICE.into()),
    });
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compartment_suffixes() {
        assert_eq!(split_compartment("glc__D_c"), Some(("glc__D", "c")));
        assert_eq!(split_compartment("co2_cx"), Some(("co2", "cx")));
        assert_eq!(split_compartment("nocompartment"), None);
        assert_eq!(split_compartment("_c"), None);
    }

    #[test]
    fn short_reaction_names() {
        assert_eq!(without_trailing_bracket("hexokinase (D-glucose:ATP)"), Some("hexokinase"));
        assert_eq!(without_trailing_bracket("phosphofructokinase"), None);
        assert_eq!(without_trailing_bracket("(R)-something"), None);
    }
}
