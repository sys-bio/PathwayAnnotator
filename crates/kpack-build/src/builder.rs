//! Collects entities, reactions and names from every source, then indexes
//! them into a [`Pack`].

use std::collections::HashMap;

use kpack::{
    CompId, CompartmentDef, Entity, EntityId, Header, NameHit, NameIndex, Namespace, NsId, Pack,
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

    /// Indexes everything into a pack.
    pub fn finish(mut self, built: String) -> Pack {
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
