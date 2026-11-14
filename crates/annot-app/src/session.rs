//! The state of one annotation session: the model, the pack, the engine's
//! suggestions, and what each element's status is. No UI code here.

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
}

impl Session {
    pub fn new(pack: Rc<Pack>, ws: Workspace, file_name: String) -> Session {
        let model = ws.engine_model();
        let suggestions = annotate(&pack, &model);
        let mut s = Session {
            pack,
            ws,
            model,
            suggestions,
            file_name,
            species_status: Vec::new(),
            reaction_status: Vec::new(),
        };
        s.refresh();
        s
    }

    fn refresh(&mut self) {
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
                label: targets
                    .iter()
                    .map(|t| self.label(*t))
                    .collect::<Vec<_>>()
                    .join(" / "),
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

    /// Accepts a candidate. Returns a message for the user.
    pub fn accept(&mut self, el: Element, target: Target) -> String {
        let label = self.label(target);
        let msg = match self.ws.accept(&self.pack, el, target) {
            Ok(_) => format!("{} identified as {label}.", self.display_name(el)),
            Err(e) => format!("Could not annotate {}: {e}.", self.display_name(el)),
        };
        self.refresh();
        msg
    }

    /// Accepts every high-confidence suggestion. Returns how many.
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
