//! Collects entities, reactions and names from every source, then indexes
//! them into a [`Pack`].

use std::collections::HashMap;

use kpack::{
    CompId, CompartmentDef, Entity, EntityId, Header, NameHit, NameIndex, NameKind, Namespace, NsId, Pack,
    Reaction, ReactionFlags, ReactionId, SourceInfo, Sources, StrId, StringTable, Target, Xref, XrefIndex,
    XrefKey, XrefScope,
};

use crate::compartments;
use crate::labels;
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
        // "NADH(2-)" and "NADH dianion" are just NADH.
        let name = labels::tidy_entity_label(name);
        let name = name.as_str();
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

    /// Makes labels readable ([`crate::labels`]): charge states off
    /// molecule names, SBML escapes out of reaction names.
    fn tidy_labels(&mut self) {
        for i in 0..self.entities.len() {
            let raw = self.strings.get(self.entities[i].label).to_string();
            let tidy = labels::tidy_entity_label(&raw);
            if tidy != raw && !tidy.is_empty() {
                self.entities[i].label = self.strings.intern(&tidy);
            }
        }
        for i in 0..self.reactions.len() {
            let raw = self.strings.get(self.reactions[i].label).to_string();
            let tidy = labels::tidy_reaction_label(&raw);
            if tidy != raw && !tidy.is_empty() {
                self.reactions[i].label = self.strings.intern(&tidy);
            }
        }
    }

    /// Gives molecules labelled as ChEBI names their charged form an
    /// everyday label ([`labels::familiar_entity_label`]): "3-phosphonato-
    /// D-glycerate" becomes 3-phospho-D-glycerate. Runs before synonyms
    /// are picked, so the old label can still be offered as one.
    fn familiar_labels(&mut self) {
        let mut names: HashMap<u32, Vec<String>> = HashMap::new();
        for (n, h) in &self.entity_names {
            if matches!(h.kind, NameKind::Preferred | NameKind::Synonym) {
                names.entry(h.target).or_default().push(n.clone());
            }
        }
        for i in 0..self.entities.len() {
            let label = self.strings.get(self.entities[i].label).to_string();
            let own: Vec<&str> = names.get(&(i as u32)).map(|v| v.iter().map(String::as_str).collect()).unwrap_or_default();
            if let Some(better) = labels::familiar_entity_label(&label, &own) {
                self.entities[i].label = self.strings.intern(&better);
            }
        }
    }

    /// Gives reactions labelled only by an id ("PGKf") a name: their most
    /// familiar synonym; else the name of a reaction that is the same
    /// chemistry (shares a Rhea, MetaNetX, MetaCyc, KEGG or SEED reaction
    /// id: PGKf and PGK are both "phosphoglycerate kinase"); else what it
    /// does ("fumarate transport"). Runs after the synonyms are picked; a
    /// synonym that becomes the label leaves the synonym list.
    fn name_id_only_reactions(&mut self) {
        let specific: Vec<NsId> = ["rhea", "metanetx.reaction", "metacyc.reaction", "kegg.reaction", "seed.reaction"]
            .iter()
            .map(|p| self.ns(p))
            .collect();
        let named: Vec<bool> = self
            .reactions
            .iter()
            .map(|r| !labels::looks_like_id(self.strings.get(r.label)))
            .collect();
        // Named reactions by the specific ids they carry.
        let mut by_id: HashMap<(NsId, StrId), Vec<usize>> = HashMap::new();
        for (i, r) in self.reactions.iter().enumerate().filter(|(i, _)| named[*i]) {
            for x in r.xrefs.iter().filter(|x| x.scope == XrefScope::Exact && specific.contains(&x.ns)) {
                by_id.entry((x.ns, x.id)).or_default().push(i);
            }
        }
        for i in 0..self.reactions.len() {
            let label = self.strings.get(self.reactions[i].label).to_string();
            if !labels::looks_like_id(&label) {
                continue;
            }
            let synonym = self.reactions[i]
                .synonyms
                .iter()
                .position(|&s| {
                    let s = self.strings.get(s);
                    !labels::looks_like_id(s) && s.chars().any(|c| c.is_lowercase())
                });
            if let Some(k) = synonym {
                let s = self.reactions[i].synonyms.remove(k);
                self.reactions[i].label = s;
                continue;
            }
            // The named reaction sharing the most ids; the shorter name on
            // a tie ("phosphoglycerate kinase").
            let mut shared: HashMap<usize, usize> = HashMap::new();
            for x in self.reactions[i].xrefs.iter().filter(|x| x.scope == XrefScope::Exact) {
                for &j in by_id.get(&(x.ns, x.id)).into_iter().flatten() {
                    *shared.entry(j).or_default() += 1;
                }
            }
            let twin = shared.into_iter().max_by(|a, b| {
                let len = |j: usize| self.strings.get(self.reactions[j].label).len();
                a.1.cmp(&b.1).then(len(b.0).cmp(&len(a.0))).then(b.0.cmp(&a.0))
            });
            // A transport or exchange is better described by what it moves
            // than by a twin's name, which may be for another compartment.
            let moves = {
                let f = self.reactions[i].flags;
                f.contains(ReactionFlags::TRANSPORT) || f.contains(ReactionFlags::EXCHANGE)
            };
            if let Some((j, _)) = twin.filter(|_| !moves) {
                let name = self.strings.get(self.reactions[j].label).to_string();
                let name = labels::without_compartment(&name).to_string();
                self.reactions[i].label = self.strings.intern(&name);
                continue;
            }
            let strings = &self.strings;
            let entities = &self.entities;
            let derived = labels::reaction_label_from_participants(&self.reactions[i], entities, |e| {
                strings.get(entities[e as usize].label).to_string()
            });
            if let Some(d) = derived {
                self.reactions[i].label = self.strings.intern(&d);
            }
        }
    }

    /// Indexes everything into a pack.
    pub fn finish(mut self, built: String) -> Pack {
        self.tidy_labels();
        self.familiar_labels();
        self.fill_synonyms();
        self.name_id_only_reactions();
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
