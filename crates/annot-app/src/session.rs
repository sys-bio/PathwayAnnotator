//! The state of one annotation session: the model, the pack, the engine's
//! suggestions, and what each element's status is. No UI code here.

use std::collections::BTreeMap;
use std::rc::Rc;

use annot_engine::{Candidate, Confidence, Model, Suggestions, annotate, confidence};
use annot_model::{Element, Resolution, Workspace};
use kpack::{Pack, Target};

/// Where an element stands.
#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    /// Already annotated with something in the pack.
    Identified { label: String },
    /// Annotated, but with nothing in our database (often a protein).
    AnnotatedElsewhere,
    /// Not annotated; the engine has a suggestion.
    Suggested { label: String, level: Confidence },
    /// Not annotated, no suggestion.
    Unknown,
}

impl Status {
    /// A CSS class for the status.
    pub fn class(&self) -> &'static str {
        match self {
            Status::Identified { .. } | Status::AnnotatedElsewhere => "done",
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
    /// Suggestions the user rejected, per element. Kept for the session
    /// only: they are not written into the model.
    rejected: BTreeMap<Element, Vec<Target>>,
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
            rejected: BTreeMap::new(),
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
            Resolution::OutsidePack => Status::AnnotatedElsewhere,
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
            _ => String::new(),
        }
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
                Status::Identified { .. } | Status::AnnotatedElsewhere => c.done += 1,
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

    /// Accepts every high-confidence suggestion. Returns how many.
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

    pub fn accept_all_high(&mut self) -> usize {
        let mut n = 0;
        for el in self.species().into_iter().chain(self.reactions()) {
            if let Status::Suggested {
                level: Confidence::High,
                ..
            } = self.status(el)
            {
                let target = self.candidates(el)[0].target;
                if self.ws.accept(&self.pack, el, target).is_ok() {
                    n += 1;
                }
            }
        }
        self.refresh();
        n
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
