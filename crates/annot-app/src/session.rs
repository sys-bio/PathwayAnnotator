//! The state of one annotation session: the model, the pack, the engine's
//! suggestions, and what each element's status is. No UI code here.

use std::collections::BTreeMap;
use std::rc::Rc;

use annot_engine::{Candidate, Confidence, Model, Role, Suggestions, annotate, confidence};
use annot_model::{Element, Resolution, Workspace};
use kpack::{EntityFlags, Pack, Target, XrefScope};

/// Where an element stands.
#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    /// Already annotated with something in the pack.
    Identified { label: String },
    /// Annotated, but with nothing in our database (often a protein). The
    /// label is known when the user picked it from a search of other
    /// databases this session.
    AnnotatedElsewhere { label: Option<String> },
    /// Not annotated; the engine has a suggestion.
    Suggested { label: String, level: Confidence },
    /// Not annotated, no suggestion.
    Unknown,
}

impl Status {
    /// A CSS class for the status.
    pub fn class(&self) -> &'static str {
        match self {
            Status::Identified { .. } | Status::AnnotatedElsewhere { .. } => "done",
            Status::Suggested { level: Confidence::High, .. } => "high",
            Status::Suggested { level: Confidence::Likely, .. } => "likely",
            Status::Suggested { level: Confidence::Uncertain, .. } => "uncertain",
            Status::Unknown => "unknown",
        }
    }
}

/// What the card on a molecule chip shows.
#[derive(Clone, Debug, PartialEq)]
pub struct MoleculeInfo {
    pub name: String,
    /// The SBML id, if it differs from the name.
    pub id: Option<String>,
    pub compartment: Option<String>,
    pub status: Status,
    /// What it is identified as, else the engine's first suggestion.
    pub target: Option<Target>,
    pub synonyms: Vec<String>,
    /// Where to check it: its annotation, else the suggestion's entry.
    pub link: Option<String>,
}

/// The biology qualifiers in words, for people: (qualifier, words). "is"
/// first, then the ones modelers use most.
pub const QUALIFIER_WORDS: &[(&str, &str)] = &[
    ("is", "is"),
    ("isVersionOf", "is a version of (a kind of)"),
    ("hasVersion", "has version (a more specific kind)"),
    ("hasPart", "has part"),
    ("isPartOf", "is part of"),
    ("isHomologTo", "is homologous to"),
    ("isDescribedBy", "is described by"),
    ("isEncodedBy", "is encoded by"),
    ("encodes", "encodes"),
    ("occursIn", "occurs in"),
    ("hasProperty", "has property"),
    ("isPropertyOf", "is a property of"),
    ("hasTaxon", "is in organism"),
];

/// The model qualifiers in words (`bqmodel:`), for the model itself.
pub const MODEL_QUALIFIER_WORDS: &[(&str, &str)] = &[
    ("isDescribedBy", "is described by (a paper)"),
    ("isDerivedFrom", "is derived from (another model)"),
    ("is", "is (this model entry)"),
    ("isInstanceOf", "is an instance of"),
    ("hasInstance", "has instance"),
];

/// A qualifier as the editor names it: "bqbiol:isVersionOf",
/// "bqmodel:isDescribedBy".
pub fn qualifier_key(model: bool, q: &str) -> String {
    format!("{}:{q}", if model { "bqmodel" } else { "bqbiol" })
}

/// (model qualifier?, name) from a [`qualifier_key`]; a bare name is a
/// biology qualifier.
pub fn split_qualifier(key: &str) -> (bool, &str) {
    match key.split_once(':') {
        Some(("bqmodel", q)) => (true, q),
        Some(("bqbiol", q)) => (false, q),
        _ => (false, key),
    }
}

/// A qualifier in words ("bqbiol:isVersionOf" -> "is a version of (a kind
/// of)", "bqmodel:isDescribedBy" -> "is described by (a paper)").
pub fn qualifier_words(key: &str) -> &str {
    let (model, q) = split_qualifier(key);
    let table = if model { MODEL_QUALIFIER_WORDS } else { QUALIFIER_WORDS };
    table.iter().find(|(k, _)| *k == q).map_or(q, |(_, w)| w)
}

/// The qualifiers to offer for an element, as ([`qualifier_key`], words):
/// the model qualifiers first for the model itself, then the biology ones.
pub fn qualifier_choices(el: Element) -> Vec<(String, &'static str)> {
    let model = matches!(el, Element::Model);
    let mut out: Vec<(String, &'static str)> = Vec::new();
    if model {
        out.extend(MODEL_QUALIFIER_WORDS.iter().map(|(q, w)| (qualifier_key(true, q), *w)));
    }
    out.extend(QUALIFIER_WORDS.iter().map(|(q, w)| (qualifier_key(false, q), *w)));
    out
}

/// The SBO terms that suit an element, with their names (checked against
/// the SBO ontology, 2026-11-14).
pub fn sbo_choices(el: Element) -> &'static [(u32, &'static str)] {
    match el {
        Element::Species(_) => &[
            (247, "simple chemical"),
            (327, "non-macromolecular ion"),
            (252, "polypeptide chain (protein)"),
            (297, "protein complex"),
            (253, "non-covalent complex"),
            (245, "macromolecule"),
            (250, "ribonucleic acid (RNA)"),
            (278, "messenger RNA"),
            (251, "deoxyribonucleic acid (DNA)"),
            (243, "gene"),
            (240, "material entity"),
            (285, "material entity of unspecified nature"),
        ],
        Element::Reaction(_) => &[
            (176, "biochemical reaction"),
            (185, "translocation reaction (transport)"),
            (167, "biochemical or transport reaction"),
            (627, "exchange reaction"),
            (632, "sink reaction"),
            (628, "demand reaction"),
            (177, "non-covalent binding"),
            (180, "dissociation"),
            (182, "conversion"),
            (200, "redox reaction"),
            (216, "phosphorylation"),
            (179, "degradation"),
            (183, "transcription"),
            (184, "translation"),
            (375, "process"),
        ],
        Element::Compartment(_) => &[
            (290, "physical compartment"),
            (289, "functional compartment"),
            (410, "implicit compartment"),
        ],
        Element::Model => &[
            (293, "non-spatial continuous framework (ODEs)"),
            (62, "continuous framework"),
            (292, "spatial continuous framework"),
            (63, "discrete framework"),
            (624, "flux balance framework"),
            (234, "logical framework"),
            (4, "modelling framework"),
        ],
    }
}

/// Counts for the overview.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Counts {
    pub total: usize,
    pub done: usize,
    pub high: usize,
    pub likely: usize,
    pub uncertain: usize,
    pub unknown: usize,
}

pub struct Session {
    pub pack: Rc<Pack>,
    pub ws: Workspace,
    pub model: Model,
    pub suggestions: Suggestions,
    pub file_name: String,
    /// Statuses, recomputed after every edit rather than on every render
    /// (each one reads the element's annotation XML).
    species_status: Vec<Status>,
    reaction_status: Vec<Status>,
    compartment_status: Vec<Status>,
    /// The GO term suggested for each compartment, from the pack's table
    /// of compartment names: (URI, label).
    compartment_go: Vec<Option<(String, String)>>,
    /// Compartments whose suggestion the user rejected (this session).
    rejected_compartments: Vec<Element>,
    /// Suggestions the user rejected, per element. Kept for the session
    /// only: they are not written into the model.
    rejected: BTreeMap<Element, Vec<Target>>,
    /// What entries accepted from a search of other databases are called,
    /// by URI, so the list can say "fructose 6-phosphate (ChEBI ...)"
    /// rather than "annotated elsewhere".
    online_labels: BTreeMap<String, String>,
    /// The same entries' plain names ("keto-D-fructose 6-phosphate").
    online_names: BTreeMap<String, String>,
    /// Papers looked up for the model's references (Europe PMC), by URI.
    papers: BTreeMap<String, annot_online::Paper>,
}

impl Session {
    pub fn new(pack: Rc<Pack>, ws: Workspace, file_name: String) -> Session {
        let model = ws.engine_model();
        let mut s = Session {
            pack,
            ws,
            model,
            suggestions: Suggestions::default(),
            file_name,
            species_status: Vec::new(),
            reaction_status: Vec::new(),
            compartment_status: Vec::new(),
            compartment_go: Vec::new(),
            rejected_compartments: Vec::new(),
            rejected: BTreeMap::new(),
            online_labels: BTreeMap::new(),
            online_names: BTreeMap::new(),
            papers: BTreeMap::new(),
        };
        s.refresh();
        s
    }

    /// Re-runs the engine with what is now known, then recomputes every
    /// status. Called after every edit: each molecule and reaction the user
    /// confirms becomes certain for the engine, so the elements around it are
    /// suggested better (a confirmed reaction pins down its unconfirmed
    /// molecules; a reaction whose molecules are all confirmed and match one
    /// database reaction exactly becomes high confidence). Rejected
    /// suggestions are passed on so they never come back.
    fn refresh(&mut self) {
        // Rebuilt from the document every time: a rename (or undoing one)
        // changes ids the engine sees.
        self.model = self.ws.engine_model();
        for i in 0..self.model.species.len() {
            let el = Element::Species(i);
            let known = match self.ws.resolution(&self.pack, el) {
                Resolution::Known { targets, .. } => match targets.as_slice() {
                    [Target::Entity(e)] => Some(*e),
                    _ => None,
                },
                _ => None,
            };
            let rejected = self.rejected_entities(el);
            let s = &mut self.model.species[i];
            s.known = known;
            s.rejected = rejected;
        }
        for i in 0..self.model.reactions.len() {
            let el = Element::Reaction(i);
            // A reaction's Rhea / EC / KEGG ids often fit several BiGG
            // variants (one per compartment) equally; any of them pins down
            // the molecules, since compartments are not compared.
            let known = match self.ws.resolution(&self.pack, el) {
                Resolution::Known { targets, .. } => match self.representative(&targets) {
                    Some(Target::Reaction(r)) => Some(r),
                    _ => None,
                },
                _ => None,
            };
            let rejected = self
                .rejected
                .get(&el)
                .into_iter()
                .flatten()
                .filter_map(|t| match t {
                    Target::Reaction(r) => Some(*r),
                    _ => None,
                })
                .collect();
            let r = &mut self.model.reactions[i];
            r.known = known;
            r.rejected = rejected;
        }
        self.suggestions = annotate(&self.pack, &self.model);
        self.species_status = self.species().into_iter().map(|el| self.compute_status(el)).collect();
        self.compartment_go = (0..self.model.compartments.len()).map(|i| self.match_compartment(i)).collect();
        self.compartment_status = self.compartments().into_iter().map(|el| self.compartment_status_of(el)).collect();
        self.reaction_status = self.reactions().into_iter().map(|el| self.compute_status(el)).collect();
    }

    pub fn undo(&mut self) {
        if self.ws.undo() {
            self.refresh();
        }
    }

    pub fn redo(&mut self) {
        if self.ws.redo() {
            self.refresh();
        }
    }

    /// Other names people know a target by, most familiar first:
    /// "glucose", "D-glucose" for "D-glucopyranose".
    pub fn synonyms(&self, t: Target) -> Vec<String> {
        let ids = match t {
            Target::Entity(e) => &self.pack.entity(e).synonyms,
            Target::Reaction(r) => &self.pack.reaction(r).synonyms,
        };
        ids.iter().map(|s| self.pack.str(*s).to_string()).collect()
    }

    /// What people call a pack target: "glucose 6-phosphate".
    pub fn label(&self, t: Target) -> String {
        match t {
            Target::Entity(e) => self.pack.str(self.pack.entity(e).label).to_string(),
            Target::Reaction(r) => self.pack.str(self.pack.reaction(r).label).to_string(),
        }
    }

    /// The database entries accepting a target would write, as (text, URL)
    /// pairs for people to check: ("ChEBI 4170",
    /// "https://identifiers.org/CHEBI:4170"). The URLs are the annotation
    /// URIs themselves; identifiers.org forwards them to the database's own
    /// page, so what people check is exactly what gets saved.
    pub fn links(&self, t: Target) -> Vec<(String, String)> {
        annot_model::uris_for(&self.pack, t)
            .into_iter()
            .map(|uri| {
                let text = match self.pack.resolve_resource(&uri) {
                    Some((ns, id)) => {
                        let ns = &self.pack.namespaces[ns as usize];
                        let id = id.strip_prefix(self.pack.str(ns.id_prefix)).unwrap_or(&id).to_string();
                        format!("{} {id}", self.pack.str(ns.label))
                    }
                    None => uri.clone(),
                };
                (text, uri)
            })
            .collect()
    }

    /// Where to check a target first: its main database entry.
    pub fn main_link(&self, t: Target) -> Option<String> {
        annot_model::uris_for(&self.pack, t).into_iter().next()
    }

    pub fn candidates(&self, el: Element) -> &[Candidate] {
        match el {
            Element::Species(i) => &self.suggestions.species[i],
            Element::Reaction(i) => &self.suggestions.reactions[i],
            _ => &[],
        }
    }

    /// The element's status (cached).
    pub fn status(&self, el: Element) -> Status {
        match el {
            Element::Species(i) => self.species_status.get(i).cloned().unwrap_or(Status::Unknown),
            Element::Reaction(i) => self.reaction_status.get(i).cloned().unwrap_or(Status::Unknown),
            Element::Compartment(i) => self.compartment_status.get(i).cloned().unwrap_or(Status::Unknown),
            _ => Status::Unknown,
        }
    }

    fn compute_status(&self, el: Element) -> Status {
        match self.ws.resolution(&self.pack, el) {
            Resolution::Known { targets, .. } => Status::Identified {
                label: match (self.representative(&targets), targets.len()) {
                    (Some(t), 1) => self.label(t),
                    (Some(t), n) if matches!(t, Target::Reaction(_)) => {
                        format!("{} (and {} compartment variants)", self.label(t), n - 1)
                    }
                    (Some(t), n) => format!("{} (and {} more)", self.label(t), n - 1),
                    (None, _) => String::new(),
                },
            },
            Resolution::OutsidePack => Status::AnnotatedElsewhere {
                label: self.ws.identity_uris(el).iter().find_map(|u| self.online_labels.get(u).cloned()),
            },
            Resolution::Unannotated => {
                let cands = self.candidates(el);
                match (cands.first(), confidence(cands)) {
                    (Some(c), Some(level)) => Status::Suggested {
                        label: self.label(c.target),
                        level,
                    },
                    _ => Status::Unknown,
                }
            }
        }
    }

    /// The name to show for a species or reaction: its name, else its id.
    pub fn display_name(&self, el: Element) -> String {
        match el {
            Element::Species(i) => {
                let s = &self.model.species[i];
                s.name.clone().filter(|n| !n.trim().is_empty()).unwrap_or_else(|| s.id.clone())
            }
            Element::Reaction(i) => {
                let r = &self.model.reactions[i];
                r.name.clone().filter(|n| !n.trim().is_empty()).unwrap_or_else(|| r.id.clone())
            }
            Element::Compartment(i) => {
                let c = &self.model.compartments[i];
                c.name.clone().filter(|n| !n.trim().is_empty()).unwrap_or_else(|| c.id.clone())
            }
            Element::Model => "The model".into(),
        }
    }

    pub fn compartments(&self) -> Vec<Element> {
        (0..self.model.compartments.len()).map(Element::Compartment).collect()
    }

    /// The GO term for a compartment from the pack's table of compartment
    /// names (BiGG's codes, labels and the names modelers use), with how
    /// sure the match is: its code or label is likely; another name
    /// ("cytoplasm" for cytosol, which GO keeps apart) is uncertain.
    fn match_compartment(&self, i: usize) -> Option<(String, String)> {
        self.compartment_match(i).map(|(uri, label, _)| (uri, label))
    }

    fn compartment_match(&self, i: usize) -> Option<(String, String, Confidence)> {
        let c = &self.model.compartments[i];
        let words: Vec<String> = [Some(c.id.clone()), c.name.clone()]
            .into_iter()
            .flatten()
            .map(|w| kpack::normalize_name(&w))
            .filter(|w| !w.is_empty())
            .collect();
        let mut best: Option<(String, String, Confidence)> = None;
        for def in &self.pack.compartments {
            let Some(go) = def.go else { continue };
            let label = self.pack.str(def.label);
            let main = [self.pack.str(def.code), label].map(kpack::normalize_name);
            let other: Vec<String> = def.synonyms.iter().map(|s| kpack::normalize_name(self.pack.str(*s))).collect();
            let level = if words.iter().any(|w| main.contains(w)) {
                Confidence::Likely
            } else if words.iter().any(|w| other.contains(w)) {
                Confidence::Uncertain
            } else {
                continue;
            };
            let better = best.as_ref().is_none_or(|b| level == Confidence::Likely && b.2 != Confidence::Likely);
            if better {
                best = Some((format!("https://identifiers.org/{}", self.pack.str(go)), label.to_string(), level));
            }
        }
        best
    }

    fn compartment_status_of(&self, el: Element) -> Status {
        let Element::Compartment(i) = el else { return Status::Unknown };
        let uris = self.ws.identity_uris(el);
        let go = uris.iter().find(|u| u.contains("GO:") || u.contains("GO%3A"));
        if let Some(go) = go {
            let id = go.rsplit(['/', ':']).next().unwrap_or_default();
            let label = self
                .pack
                .compartments
                .iter()
                .find(|d| d.go.is_some_and(|g| self.pack.str(g).ends_with(id)))
                .map(|d| format!("{} (GO:{id})", self.pack.str(d.label)))
                .or_else(|| self.online_labels.get(go).cloned())
                .unwrap_or_else(|| format!("GO:{id}"));
            return Status::Identified { label };
        }
        if !uris.is_empty() {
            return Status::AnnotatedElsewhere { label: uris.iter().find_map(|u| self.online_labels.get(u).cloned()) };
        }
        if self.rejected_compartments.contains(&el) {
            return Status::Unknown;
        }
        match self.compartment_match(i) {
            Some((uri, label, level)) => {
                let id = uri.rsplit('/').next().unwrap_or_default().to_string();
                Status::Suggested { label: format!("{label} ({id})"), level }
            }
            None => Status::Unknown,
        }
    }

    /// The GO term suggested for a compartment: (URI, label).
    pub fn compartment_suggestion(&self, el: Element) -> Option<(String, String)> {
        match el {
            Element::Compartment(i) if !self.rejected_compartments.contains(&el) => {
                self.compartment_go.get(i).cloned().flatten()
            }
            _ => None,
        }
    }

    /// Accepts a compartment's GO suggestion. Returns a message for the user.
    pub fn accept_compartment(&mut self, el: Element) -> String {
        let name = self.display_name(el);
        let Some((uri, label)) = self.compartment_suggestion(el) else {
            return format!("There is nothing to accept for {name}.");
        };
        let msg = match self.ws.accept_uri(el, &uri, Some(annot_online::Db::Go.sbo())) {
            Ok(()) => format!("{name} identified as {label}."),
            Err(e) => format!("Could not annotate {name}: {e}."),
        };
        self.refresh();
        msg
    }

    /// Rejects a compartment's GO suggestion for this session.
    pub fn reject_compartment(&mut self, el: Element) -> String {
        let name = self.display_name(el);
        if !self.rejected_compartments.contains(&el) {
            self.rejected_compartments.push(el);
        }
        self.refresh();
        format!("The suggestion for {name} is set aside; search GO in Details for another.")
    }

    pub fn species(&self) -> Vec<Element> {
        (0..self.model.species.len()).map(Element::Species).collect()
    }

    pub fn reactions(&self) -> Vec<Element> {
        (0..self.model.reactions.len()).map(Element::Reaction).collect()
    }

    pub fn counts(&self, elements: &[Element]) -> Counts {
        let mut c = Counts {
            total: elements.len(),
            ..Counts::default()
        };
        for el in elements {
            match self.status(*el) {
                Status::Identified { .. } | Status::AnnotatedElsewhere { .. } => c.done += 1,
                Status::Suggested { level: Confidence::High, .. } => c.high += 1,
                Status::Suggested { level: Confidence::Likely, .. } => c.likely += 1,
                Status::Suggested { level: Confidence::Uncertain, .. } => c.uncertain += 1,
                Status::Unknown => c.unknown += 1,
            }
        }
        c
    }

    /// One target to stand for several equally good ones: the plainest
    /// (shortest) BiGG id, as the engine prefers ("PGK" over "PGKf").
    fn representative(&self, targets: &[Target]) -> Option<Target> {
        let bigg = |t: &Target| -> (usize, String) {
            let id = match *t {
                Target::Entity(e) => self.pack.entity_xrefs(e, "bigg.metabolite").next().map(|x| self.pack.str(x.id)),
                Target::Reaction(r) => self
                    .pack
                    .reaction(r)
                    .xrefs
                    .iter()
                    .find(|x| self.pack.namespace_prefix(x.ns) == "bigg.reaction")
                    .map(|x| self.pack.str(x.id)),
            };
            let id = id.unwrap_or_default().to_string();
            (if id.is_empty() { usize::MAX } else { id.len() }, id)
        };
        targets.iter().copied().min_by_key(bigg)
    }

    fn rejected_entities(&self, el: Element) -> Vec<kpack::EntityId> {
        self.rejected
            .get(&el)
            .into_iter()
            .flatten()
            .filter_map(|t| match t {
                Target::Entity(e) => Some(*e),
                _ => None,
            })
            .collect()
    }

    /// Rejects a suggestion: it is not suggested again for this element,
    /// and the engine does not reason from it. Returns a message.
    pub fn reject(&mut self, el: Element, target: Target) -> String {
        let list = self.rejected.entry(el).or_default();
        if !list.contains(&target) {
            list.push(target);
        }
        self.refresh();
        format!(
            "{} will not be suggested again for {}. Restore it under Details.",
            self.label(target),
            self.display_name(el)
        )
    }

    /// Takes back a rejection.
    pub fn restore(&mut self, el: Element, target: Target) -> String {
        if let Some(list) = self.rejected.get_mut(&el) {
            list.retain(|t| *t != target);
        }
        self.refresh();
        format!("{} can be suggested for {} again.", self.label(target), self.display_name(el))
    }

    /// The suggestions rejected for an element, with their labels.
    pub fn rejected(&self, el: Element) -> Vec<(Target, String)> {
        self.rejected
            .get(&el)
            .into_iter()
            .flatten()
            .map(|t| (*t, self.label(*t)))
            .collect()
    }

    /// Everything the card on a molecule chip shows.
    pub fn molecule_info(&self, el: Element) -> MoleculeInfo {
        let name = self.display_name(el);
        let id = self.ws.element_id(el).to_string();
        let compartment = match el {
            Element::Species(i) => self.model.species[i].compartment.clone(),
            _ => None,
        };
        let identified = self.identified_as(el);
        let target = self
            .representative(&identified)
            .or_else(|| self.candidates(el).first().map(|c| c.target));
        let link = if identified.is_empty() {
            target.and_then(|t| self.main_link(t))
        } else {
            self.ws.identity_uris(el).into_iter().next()
        };
        MoleculeInfo {
            id: (id != name).then_some(id),
            name,
            compartment,
            status: self.status(el),
            target,
            synonyms: target.map(|t| self.synonyms(t)).unwrap_or_default(),
            link,
        }
    }

    /// What an element is currently identified as in the pack (empty if it
    /// is not).
    pub fn identified_as(&self, el: Element) -> Vec<Target> {
        match self.ws.resolution(&self.pack, el) {
            Resolution::Known { targets, .. } => targets,
            _ => Vec::new(),
        }
    }

    /// Accepts a candidate. If the element is already identified as
    /// something else, the candidate replaces it (one undoable step) rather
    /// than being added beside it. Returns a message for the user.
    pub fn accept(&mut self, el: Element, target: Target) -> String {
        let label = self.label(target);
        let name = self.display_name(el);
        let current = self.identified_as(el);
        let msg = if current.contains(&target) {
            format!("{name} is already identified as {label}.")
        } else if current.is_empty() {
            match self.ws.accept(&self.pack, el, target) {
                Ok(_) => format!("{name} identified as {label}."),
                Err(e) => format!("Could not annotate {name}: {e}."),
            }
        } else {
            match self.ws.replace_identity(&self.pack, el, target) {
                Ok(_) => format!("{name} is now identified as {label} instead."),
                Err(e) => format!("Could not change {name}: {e}."),
            }
        };
        self.refresh();
        msg
    }

    /// Renames an element's SBML id, updating every reference to it.
    /// Returns a message for the user, or why it could not be done.
    pub fn rename(&mut self, el: Element, new_id: &str) -> Result<String, String> {
        let old = self.ws.element_id(el).to_string();
        let n = self.ws.rename(el, new_id).map_err(|e| e.to_string())?;
        self.refresh();
        let refs = n.saturating_sub(1);
        Ok(format!(
            "Renamed {old} to {}, and updated {refs} reference{} to it. Undo reverts the whole rename.",
            new_id.trim(),
            if refs == 1 { "" } else { "s" }
        ))
    }

    /// Accepts every High suggestion, as one undoable step.
    pub fn accept_all_high(&mut self) -> usize {
        let todo: Vec<(Element, Target)> = self
            .species()
            .into_iter()
            .chain(self.reactions())
            .filter(|&el| matches!(self.status(el), Status::Suggested { level: Confidence::High, .. }))
            .map(|el| (el, self.candidates(el)[0].target))
            .collect();
        let pack = &self.pack;
        let n = self.ws.batch(|ws| todo.iter().filter(|&&(el, t)| ws.accept(pack, el, t).is_ok()).count());
        self.refresh();
        n
    }

    /// The text to search other databases with: the element's name, else
    /// its id made readable ("M_glc__D_c" -> "glc D").
    pub fn search_text(&self, el: Element) -> String {
        let name = match el {
            Element::Species(i) => self.model.species[i].name.clone(),
            Element::Reaction(i) => self.model.reactions[i].name.clone(),
            Element::Compartment(i) => self.model.compartments[i].name.clone(),
            _ => None,
        };
        name.filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| annot_online::query_from_id(self.ws.element_id(el)))
    }

    /// What to search Rhea's enzymes with for a reaction: an EC number it
    /// is annotated with, else its name (not its id: "vPFK" finds nothing
    /// useful). Empty when there is neither.
    pub fn enzyme_text(&self, reaction: Element) -> String {
        let ec = self.ws.identity_uris(reaction).iter().find_map(|u| {
            let (_, id) = kpack::split_resource(u)?;
            annot_online::ec_from_text(&id)
        });
        if let Some(ec) = ec {
            return ec;
        }
        match reaction {
            Element::Reaction(i) => self.model.reactions[i].name.clone().unwrap_or_default(),
            _ => String::new(),
        }
    }

    /// Names of entries accepted from other databases earlier (kept in
    /// the browser): (URI, plain name, name with database and id).
    pub fn remember_online(&mut self, entries: Vec<(String, String, String)>) {
        for (uri, name, label) in entries {
            self.online_names.insert(uri.clone(), name);
            self.online_labels.insert(uri, label);
        }
        self.refresh();
    }

    /// The plain name and the full label of an entry accepted from another
    /// database, for remembering it.
    pub fn online_entry(&self, uri: &str) -> Option<(String, String)> {
        Some((self.online_names.get(uri)?.clone(), self.online_labels.get(uri)?.clone()))
    }

    /// A species' ChEBI ids, every charge form, if it is identified (in
    /// the pack, or with a ChEBI URI the pack does not know).
    fn chebi_ids(&self, species: Element) -> Vec<String> {
        let mut ids = Vec::new();
        for t in self.identified_as(species) {
            if let Target::Entity(e) = t {
                for x in self.pack.entity_xrefs(e, "chebi").filter(|x| x.scope == XrefScope::Exact && !x.secondary) {
                    ids.push(self.pack.str(x.id).to_string());
                }
            }
        }
        for uri in self.ws.identity_uris(species) {
            let uri = uri.replace("%3A", ":");
            if let Some(i) = uri.find("CHEBI:") {
                let id: String = uri[i..].chars().take_while(|c| *c != '/' && *c != '#').collect();
                ids.push(id);
            }
        }
        ids.sort();
        ids.dedup();
        ids
    }

    /// Whether a species is water, a proton, ATP or the like: by what it
    /// is identified as, else by the engine's first suggestion.
    fn is_currency(&self, species: Element) -> bool {
        let currency = |t: &Target| matches!(t, Target::Entity(e) if self.pack.entity(*e).flags.contains(EntityFlags::CURRENCY));
        let known = self.identified_as(species);
        if !known.is_empty() {
            return known.iter().any(currency);
        }
        self.candidates(species).first().is_some_and(|c| currency(&c.target))
    }

    /// A reaction's molecules, as a Rhea search needs them.
    pub fn rhea_participants(&self, reaction: Element) -> Vec<annot_online::Participant> {
        let Element::Reaction(i) = reaction else { return Vec::new() };
        self.model.reactions[i]
            .participants
            .iter()
            .filter(|p| p.role != Role::Modifier)
            .map(|p| {
                let sp = Element::Species(p.species);
                annot_online::Participant {
                    name: self.search_text(sp),
                    label: self.identified_name(sp).or_else(|| {
                        self.candidates(sp).first().map(|c| self.label(c.target))
                    }),
                    chebi: self.chebi_ids(sp),
                    product: p.role == Role::Product,
                    currency: self.is_currency(sp),
                }
            })
            .collect()
    }

    /// What a species is identified as, in words: the pack's label, or the
    /// name of an entry accepted from another database this session.
    fn identified_name(&self, species: Element) -> Option<String> {
        if let Some(t) = self.representative(&self.identified_as(species)) {
            return Some(self.label(t));
        }
        self.ws.identity_uris(species).iter().find_map(|u| self.online_names.get(u).cloned())
    }

    /// Whether a ChEBI id is a currency molecule in the pack.
    pub fn chebi_is_currency(&self, id: &str) -> bool {
        self.pack
            .find_resource(&format!("https://identifiers.org/{id}"))
            .iter()
            .any(|(t, _)| matches!(t, Target::Entity(e) if self.pack.entity(*e).flags.contains(EntityFlags::CURRENCY)))
    }

    /// Accepts an entry found in another database. If the pack knows it,
    /// this is the same as accepting the pack's entry (replacing any other
    /// identification); otherwise its URI and SBO term are added. Returns
    /// a message for the user.
    pub fn accept_online(&mut self, el: Element, hit: &annot_online::Hit) -> String {
        let name = self.display_name(el);
        // For the model itself, a GO process is what it is a version of; no
        // SBO term (the model's is its framework).
        if el == Element::Model {
            let what = format!("{} ({} {})", hit.label, hit.db.label(), hit.id);
            let msg = match self.ws.add_terms(el, "isVersionOf", std::slice::from_ref(&hit.uri)) {
                Ok(()) => {
                    self.online_labels.insert(hit.uri.clone(), what.clone());
                    self.online_names.insert(hit.uri.clone(), hit.label.clone());
                    format!("The model is now a version of {what}.")
                }
                Err(e) => format!("Could not annotate the model: {e}."),
            };
            self.refresh();
            return msg;
        }
        let in_pack = self
            .pack
            .find_resource(&hit.uri)
            .into_iter()
            .filter(|(_, scope)| *scope == XrefScope::Exact)
            .map(|(t, _)| t)
            .find(|t| matches!((t, el), (Target::Entity(_), Element::Species(_)) | (Target::Reaction(_), Element::Reaction(_))));
        if let Some(target) = in_pack {
            return self.accept(el, target);
        }
        let what = format!("{} ({} {})", hit.label, hit.db.label(), hit.id);
        let msg = match self.ws.accept_uri(el, &hit.uri, Some(hit.db.sbo())) {
            Ok(()) => {
                self.online_labels.insert(hit.uri.clone(), what.clone());
                self.online_names.insert(hit.uri.clone(), hit.label.clone());
                format!("{name} annotated as {what}.")
            }
            Err(e) => format!("Could not annotate {name}: {e}."),
        };
        self.refresh();
        msg
    }

    /// Sets or clears an element's SBO term. Returns a message for the user.
    pub fn set_sbo(&mut self, el: Element, sbo: Option<u32>) -> Result<String, String> {
        let name = self.display_name(el);
        self.ws.set_sbo(el, sbo).map_err(|e| format!("Could not change {name}: {e}."))?;
        self.refresh();
        Ok(match sbo {
            Some(n) => format!("{name} now has SBO term SBO:{n:07}."),
            None => format!("{name} no longer has an SBO term."),
        })
    }

    /// Moves an annotation resource to another qualifier (both as
    /// [`qualifier_key`]s, so model and biology qualifiers alike).
    pub fn change_qualifier(&mut self, el: Element, uri: &str, from: &str, to: &str) -> Result<String, String> {
        let name = self.display_name(el);
        self.ws
            .change_qualifier_as(el, uri, split_qualifier(from), split_qualifier(to))
            .map_err(|e| format!("Could not change {name}: {e}."))?;
        self.refresh();
        Ok(format!("{name}: {} {uri}.", qualifier_words(to)))
    }

    /// Removes one annotation term (`key` as [`qualifier_key`]).
    pub fn remove_term(&mut self, el: Element, key: &str, uri: &str) -> Result<String, String> {
        let name = self.display_name(el);
        let (model, q) = split_qualifier(key);
        self.ws.remove_term_as(el, model, q, uri).map_err(|e| format!("Could not change {name}: {e}."))?;
        self.refresh();
        Ok(format!("Removed {uri} from {name}."))
    }

    /// Adds an annotation term from what the user typed (a compact
    /// identifier or a URL; [`annot_model::uri_from_text`]; for a paper,
    /// also a PubMed id or DOI: [`annot_model::reference_uri`]).
    pub fn add_term(&mut self, el: Element, key: &str, text: &str) -> Result<String, String> {
        let name = self.display_name(el);
        let (model, q) = split_qualifier(key);
        let uri = if q == "isDescribedBy" { annot_model::reference_uri(text)? } else { annot_model::uri_from_text(text)? };
        if self.ws.terms(el).iter().any(|t| t.model == model && t.qualifier == q && t.uri == uri) {
            return Err(format!("{name} already has that."));
        }
        self.ws
            .add_terms_as(el, model, q, std::slice::from_ref(&uri))
            .map_err(|e| format!("Could not change {name}: {e}."))?;
        self.refresh();
        Ok(format!("{name}: {} {uri}.", qualifier_words(key)))
    }

    /// The papers describing the model: its `isDescribedBy` resources
    /// (model qualifier, or the biology one some models use).
    pub fn references(&self) -> Vec<String> {
        self.ws
            .terms(Element::Model)
            .into_iter()
            .filter(|t| t.qualifier == "isDescribedBy")
            .map(|t| t.uri)
            .collect()
    }

    /// Adds a paper describing the model, from a PubMed id, DOI, PMC id or
    /// URL ([`annot_model::reference_uri`]).
    pub fn add_reference(&mut self, text: &str) -> Result<String, String> {
        self.add_term(Element::Model, &qualifier_key(true, "isDescribedBy"), text)
    }

    /// Sets (or clears) the organism the model is about.
    pub fn set_organism(&mut self, taxon: Option<u32>) -> Result<String, String> {
        self.ws.set_model_taxon(taxon).map_err(|e| format!("Could not change the model: {e}."))?;
        self.refresh();
        Ok(match taxon {
            Some(t) => {
                let name = annot_online::ORGANISMS.iter().find(|(id, _)| *id == t).map(|(_, n)| n.to_string());
                format!("The model is now about {} (taxon {t}).", name.unwrap_or_else(|| "this organism".into()))
            }
            None => "The model no longer names an organism.".into(),
        })
    }

    /// A paper looked up for a reference, if it has been.
    pub fn paper(&self, uri: &str) -> Option<&annot_online::Paper> {
        self.papers.get(uri)
    }

    /// Keeps a paper looked up for a reference, for this session.
    pub fn remember_paper(&mut self, uri: String, paper: annot_online::Paper) {
        self.papers.insert(uri, paper);
    }

    /// The file name for the annotated model: "model.xml" -> "model-annotated.xml".
    pub fn export_name(&self) -> String {
        let stem = self
            .file_name
            .rsplit_once('.')
            .map_or(self.file_name.as_str(), |(s, _)| s);
        format!("{stem}-annotated.xml")
    }
}
