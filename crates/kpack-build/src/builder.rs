//! Collects entities, reactions and names from every source, then indexes
//! them into a [`Pack`].

use std::collections::HashMap;

use kpack::{
    CompId, CompartmentDef, Entity, EntityId, Header, NameHit, NameIndex, NameKind, Namespace, NsId, Pack,
    Reaction, ReactionId, SourceInfo, Sources, StrId, StringTable, Target, Xref, XrefIndex,
    XrefKey, XrefScope,
};

use crate::compartments;
use crate::namespaces;

/// Adds `x` unless the same id is already there, in which case the sources
/// are combined. An id shared by several charge forms (KEGG has one entry
/// for all forms of ATP) denotes no particular form, so its form charge is
/// cleared. An id is secondary only if every source says so, and keeps the
/// closest scope it is given (`Exact` over `Broader` over `Narrower`).
pub fn merge_xref(xrefs: &mut Vec<Xref>, x: Xref) {
    match xrefs.iter_mut().find(|e| e.ns == x.ns && e.id == x.id) {
        Some(e) => {
            if e.form_charge != x.form_charge {
                e.form_charge = None;
            }
            e.secondary &= x.secondary;
            e.scope = e.scope.min(x.scope);
            e.sources |= x.sources;
        }
        None => xrefs.push(x),
    }
}

/// Synonyms kept per entity or reaction.
pub const MAX_SYNONYMS: usize = 6;
/// Longer names (IUPAC names, mostly) are not shown as synonyms.
pub const MAX_SYNONYM_LEN: usize = 40;

/// Words that say where a molecule is rather than what it is: a model's
/// "Extracellular Glucose" is not a name for glucose.
const LOCATION_WORDS: &[&str] = &[
    "extracellular", "intracellular", "external", "internal", "extern", "outside", "inside", "in",
    "out", "ex", "ext", "cytoplasm", "cytoplasmic", "cytosol", "cytosolic", "mitochondria",
    "mitochondrial", "mitochondrion", "golgi", "nucleus", "nuclear", "plasma", "medium", "cell",
    "cellular", "periplasm", "periplasmic", "vacuole", "vacuolar", "lysosome", "peroxisome", "er",
    "cyt", "cyto", "mit", "total", "free",
];

/// Whether a name from a model is a plain name: no brackets or dots, no
/// location words, and no location ending glued to an abbreviation
/// ("DHAPc", "GLCex").
fn plain_model_name(name: &str) -> bool {
    if name.contains(['[', ']', '(', ')', '.', ',']) {
        return false;
    }
    let tokens: Vec<String> = name
        .split(|c: char| c.is_whitespace() || c == '-' || c == '/')
        .filter(|t| !t.is_empty())
        .map(str::to_lowercase)
        .collect();
    if tokens.iter().any(|t| LOCATION_WORDS.contains(&t.as_str())) {
        return false;
    }
    let glued = name.split_whitespace().any(|t| {
        let upper = t.chars().take_while(|c| c.is_ascii_uppercase() || c.is_ascii_digit()).count();
        let rest = &t[upper..];
        upper >= 2 && ["c", "e", "m", "ex", "in", "out", "cyt", "mit"].contains(&rest)
    });
    !glued
}

/// Whether a name is a chemical formula ("CH3COCOOH", "H2PO4", or a word
/// followed by one, "ATP C10H12N5O13P3"): element symbols and counts only.
fn has_formula(name: &str) -> bool {
    name.split_whitespace().any(|word| {
        let w = word.trim_matches(|c: char| !c.is_ascii_alphanumeric());
        let mut chars = w.chars().peekable();
        let (mut elements, mut digits) = (0, 0);
        while let Some(c) = chars.next() {
            if !c.is_ascii_uppercase() {
                return false;
            }
            if chars.peek().is_some_and(|n| n.is_ascii_lowercase()) {
                chars.next();
            }
            elements += 1;
            while chars.peek().is_some_and(|n| n.is_ascii_digit()) {
                chars.next();
                digits += 1;
            }
        }
        elements >= 2 && digits >= 1
    })
}

/// The names to show beside a label, most familiar first.
///
/// Database names (ChEBI, BiGG) come first: they are curated. A name found
/// only in models must be used in at least two of them (one-offs are often
/// typos) and be a plain name ([`plain_model_name`]). Within each group:
/// full names before short abbreviations (four characters or fewer), then
/// names used in more curated models, then shorter ones. Left out always:
/// the label itself written differently, ids (anything with "_"), names
/// differing only by a charge suffix, and long names. One spelling per
/// normalized name, a database's spelling when there is one.
pub fn pick_synonyms(label: &str, names: &[(String, NameHit)]) -> Vec<String> {
    struct Choice {
        spelling: String,
        curated: bool,
        count: u16,
    }
    let label_key = kpack::normalize_name(label);
    let mut best: HashMap<String, Choice> = HashMap::new();
    for (name, hit) in names {
        let name = kpack::strip_charge_suffix(name.trim());
        let key = kpack::normalize_name(name);
        let usable = matches!(hit.kind, NameKind::Preferred | NameKind::Synonym | NameKind::Abbreviation)
            && !key.is_empty()
            && key != label_key
            && !name.contains(['_', '[', ']', '<', '>'])
            && !has_formula(name)
            && name.chars().count() <= MAX_SYNONYM_LEN;
        if !usable {
            continue;
        }
        let curated = hit.sources.contains(Sources::CHEBI) || hit.sources.contains(Sources::BIGG);
        let c = best.entry(key).or_insert_with(|| Choice {
            spelling: name.to_string(),
            curated,
            count: 0,
        });
        c.count = c.count.max(hit.count);
        // The spelling with more lower-case letters ("D-glucose" over
        // "D-GLUCOSE"); it reads as a name, not a shout.
        let lower = |s: &str| s.chars().filter(|ch| ch.is_lowercase()).count();
        if lower(name) > lower(&c.spelling) {
            c.spelling = name.to_string();
        }
        c.curated |= curated;
    }
    let mut picked: Vec<Choice> = best
        .into_values()
        .filter(|c| c.curated || (c.count >= 2 && plain_model_name(&c.spelling)))
        .collect();
    // How much a name reads like words: lower-case letters, capped so long
    // systematic names do not win ("Adenosine triphosphate" before
    // "H4atp", "glucose" before "Glc-OH").
    let wordy = |s: &str| s.chars().filter(|ch| ch.is_lowercase()).count().min(10);
    picked.sort_by(|a, b| {
        let abbrev = |s: &str| s.chars().count() <= 4;
        b.curated
            .cmp(&a.curated)
            .then(abbrev(&a.spelling).cmp(&abbrev(&b.spelling)))
            .then(b.count.cmp(&a.count))
            .then(wordy(&b.spelling).cmp(&wordy(&a.spelling)))
            .then(a.spelling.len().cmp(&b.spelling.len()))
            .then(a.spelling.cmp(&b.spelling))
    });
    picked.into_iter().take(MAX_SYNONYMS).map(|c| c.spelling).collect()
}

#[cfg(test)]
mod synonym_tests {
    use super::*;

    #[test]
    fn formulas() {
        assert!(has_formula("CH3COCOOH"));
        assert!(has_formula("H2PO4"));
        assert!(has_formula("ATP C10H12N5O13P3"));
        assert!(!has_formula("ATP"));
        assert!(!has_formula("NADH"));
        assert!(!has_formula("H4atp"));
        assert!(!has_formula("glucose 6-phosphate"));
        assert!(!has_formula("Fru6-P"));
    }

    #[test]
    fn plain_model_names() {
        assert!(plain_model_name("glucose"));
        assert!(plain_model_name("Fructose-6-Phosphate"));
        assert!(!plain_model_name("Extracellular Glucose"));
        assert!(!plain_model_name("D-glucose [extracellular]"));
        assert!(!plain_model_name("DHAPc"));
        assert!(!plain_model_name("GLCex"));
        assert!(!plain_model_name("DHAP cyt."));
    }
}

/// Stores each string once and hands out its id.
#[derive(Default)]
pub struct Interner {
    strings: Vec<String>,
    ids: HashMap<String, StrId>,
}

impl Interner {
    pub fn intern(&mut self, s: &str) -> StrId {
        if let Some(&id) = self.ids.get(s) {
            return id;
        }
        let id = self.strings.len() as StrId;
        self.strings.push(s.to_string());
        self.ids.insert(s.to_string(), id);
        id
    }

    /// The id of `s`, if it has been interned.
    pub fn find(&self, s: &str) -> Option<StrId> {
        self.ids.get(s).copied()
    }

    /// The text of an id handed out by [`Interner::intern`].
    pub fn get(&self, id: StrId) -> &str {
        &self.strings[id as usize]
    }

    pub fn into_table(self) -> StringTable {
        StringTable::from_vec(self.strings)
    }
}

/// The pack under construction.
pub struct PackBuilder {
    pub strings: Interner,
    namespaces: Vec<Namespace>,
    ns_ids: HashMap<&'static str, NsId>,
    compartments: Vec<CompartmentDef>,
    comp_ids: HashMap<String, CompId>,
    pub entities: Vec<Entity>,
    pub reactions: Vec<Reaction>,
    entity_names: Vec<(String, NameHit)>,
    reaction_names: Vec<(String, NameHit)>,
    sources: Vec<SourceInfo>,
    /// Problems worth reporting that did not stop the build.
    pub warnings: Vec<String>,
}

impl Default for PackBuilder {
    fn default() -> Self {
        PackBuilder::new()
    }
}

impl PackBuilder {
    /// A builder that knows every namespace in [`namespaces::KNOWN`] and
    /// every compartment in [`compartments::KNOWN`].
    pub fn new() -> PackBuilder {
        let mut b = PackBuilder {
            strings: Interner::default(),
            namespaces: Vec::new(),
            ns_ids: HashMap::new(),
            compartments: Vec::new(),
            comp_ids: HashMap::new(),
            entities: Vec::new(),
            reactions: Vec::new(),
            entity_names: Vec::new(),
            reaction_names: Vec::new(),
            sources: Vec::new(),
            warnings: Vec::new(),
        };
        for def in namespaces::KNOWN {
            let ns = Namespace {
                prefix: b.strings.intern(def.prefix),
                label: b.strings.intern(def.label),
                pattern: b.strings.intern(def.pattern),
                legacy_urns: def.legacy_urns.iter().map(|u| b.strings.intern(u)).collect(),
                id_prefix: b.strings.intern(def.id_prefix),
            };
            b.ns_ids.insert(def.prefix, b.namespaces.len() as NsId);
            b.namespaces.push(ns);
        }
        for def in compartments::KNOWN {
            b.compartment(def.code);
        }
        b
    }

    /// Records a source for the pack header.
    pub fn add_source(&mut self, info: SourceInfo) {
        self.sources.push(info);
    }

    /// The id of a known namespace.
    ///
    /// # Panics
    /// If `prefix` is not in [`namespaces::KNOWN`]: that is a bug in the
    /// source module, not bad data.
    pub fn ns(&self, prefix: &str) -> NsId {
        *self
            .ns_ids
            .get(prefix)
            .unwrap_or_else(|| panic!("namespace {prefix} is not in namespaces::KNOWN"))
    }

    /// An xref in identifiers.org form.
    pub fn xref(&mut self, prefix: &str, id: &str, sources: Sources, form_charge: Option<i8>) -> Xref {
        let def = namespaces::by_prefix(prefix).expect("a known namespace");
        let id = namespaces::normalize_id(def, id);
        Xref {
            ns: self.ns(prefix),
            id: self.strings.intern(&id),
            form_charge,
            secondary: false,
            scope: XrefScope::Exact,
            sources,
        }
    }

    /// The id of a compartment code, adding it if new.
    pub fn compartment(&mut self, code: &str) -> CompId {
        if let Some(&id) = self.comp_ids.get(code) {
            return id;
        }
        let def = compartments::by_code(code);
        let label = def.map_or(code, |d| d.label);
        let comp = CompartmentDef {
            code: self.strings.intern(code),
            label: self.strings.intern(label),
            go: def.and_then(|d| d.go).map(|g| self.strings.intern(g)),
            synonyms: def
                .map(|d| d.synonyms.iter().map(|s| self.strings.intern(s)).collect())
                .unwrap_or_default(),
        };
        let id = CompId::try_from(self.compartments.len()).expect("fewer than 256 compartments");
        self.compartments.push(comp);
        self.comp_ids.insert(code.to_string(), id);
        id
    }

    pub fn add_entity(&mut self, entity: Entity) -> EntityId {
        self.entities.push(entity);
        (self.entities.len() - 1) as EntityId
    }

    pub fn add_reaction(&mut self, reaction: Reaction) -> ReactionId {
        self.reactions.push(reaction);
        (self.reactions.len() - 1) as ReactionId
    }

    /// Records that `name` may mean entity `hit.target`.
    pub fn name_entity(&mut self, name: &str, hit: NameHit) {
        self.entity_names.push((name.to_string(), hit));
    }

    /// Records that `name` may mean reaction `hit.target`.
    pub fn name_reaction(&mut self, name: &str, hit: NameHit) {
        self.reaction_names.push((name.to_string(), hit));
    }

    /// What each (namespace, id) refers to so far, for sources that resolve
    /// annotations against the entities and reactions already added.
    pub fn xref_targets(&self) -> HashMap<(NsId, StrId), Vec<(Target, XrefScope)>> {
        let mut map: HashMap<(NsId, StrId), Vec<(Target, XrefScope)>> = HashMap::new();
        for (eid, e) in self.entities.iter().enumerate() {
            for x in &e.xrefs {
                map.entry((x.ns, x.id))
                    .or_default()
                    .push((Target::Entity(eid as EntityId), x.scope));
            }
        }
        for (rid, r) in self.reactions.iter().enumerate() {
            for x in &r.xrefs {
                map.entry((x.ns, x.id))
                    .or_default()
                    .push((Target::Reaction(rid as ReactionId), x.scope));
            }
        }
        map
    }

    /// Chooses each entity's and reaction's synonyms from all the names
    /// recorded for it ([`pick_synonyms`]).
    fn fill_synonyms(&mut self) {
        let mut by_entity: HashMap<u32, Vec<(String, NameHit)>> = HashMap::new();
        for (n, h) in &self.entity_names {
            by_entity.entry(h.target).or_default().push((n.clone(), *h));
        }
        for (e, names) in by_entity {
            let label = self.strings.get(self.entities[e as usize].label).to_string();
            let picked = pick_synonyms(&label, &names);
            self.entities[e as usize].synonyms = picked.iter().map(|s| self.strings.intern(s)).collect();
        }
        let mut by_reaction: HashMap<u32, Vec<(String, NameHit)>> = HashMap::new();
        for (n, h) in &self.reaction_names {
            by_reaction.entry(h.target).or_default().push((n.clone(), *h));
        }
        for (r, names) in by_reaction {
            let label = self.strings.get(self.reactions[r as usize].label).to_string();
            let picked = pick_synonyms(&label, &names);
            self.reactions[r as usize].synonyms = picked.iter().map(|s| self.strings.intern(s)).collect();
        }
    }

    /// Indexes everything into a pack.
    pub fn finish(mut self, built: String) -> Pack {
        self.fill_synonyms();
        let names = NameIndex::build(std::mem::take(&mut self.entity_names), |s| {
            self.strings.intern(s)
        });
        let reaction_names = NameIndex::build(std::mem::take(&mut self.reaction_names), |s| {
            self.strings.intern(s)
        });

        let mut by_participant = vec![Vec::new(); self.entities.len()];
        for (rid, r) in self.reactions.iter().enumerate() {
            for p in &r.participants {
                by_participant[p.entity as usize].push(rid as ReactionId);
            }
        }
        for list in &mut by_participant {
            list.sort_unstable();
            list.dedup();
        }

        let mut rows = Vec::new();
        for (eid, e) in self.entities.iter().enumerate() {
            for x in &e.xrefs {
                rows.push(XrefKey {
                    ns: x.ns,
                    id: x.id,
                    target: Target::Entity(eid as EntityId),
                    scope: x.scope,
                });
            }
        }
        for (rid, r) in self.reactions.iter().enumerate() {
            for x in &r.xrefs {
                rows.push(XrefKey {
                    ns: x.ns,
                    id: x.id,
                    target: Target::Reaction(rid as ReactionId),
                    scope: x.scope,
                });
            }
        }
        let strings = self.strings.into_table();
        let xrefs = XrefIndex::build(rows, &strings);

        Pack {
            header: Header {
                built,
                sources: self.sources,
            },
            strings,
            namespaces: self.namespaces,
            organisms: Vec::new(),
            compartments: self.compartments,
            entities: self.entities,
            reactions: self.reactions,
            pathways: Vec::new(),
            names,
            reaction_names,
            by_participant,
            xrefs,
        }
    }
}
