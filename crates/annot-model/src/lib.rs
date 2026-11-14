//! The model being annotated.
//!
//! A [`Workspace`] holds one SBML document (an Antimony model is converted to
//! SBML on loading) and everything the annotator does to it:
//!
//! - [`Workspace::engine_model`]: the engine's view (names, ids,
//!   compartments, reaction participants; no annotations).
//! - [`Workspace::terms`] and [`resolve`]: what an element is already
//!   annotated as, and what that is in the knowledge pack.
//! - [`Workspace::accept`], [`Workspace::add_terms`],
//!   [`Workspace::remove_term`], [`Workspace::set_sbo`]: edits, each
//!   undoable ([`Workspace::undo`], [`Workspace::redo`]).
//! - [`Workspace::to_sbml`]: the annotated model.
//!
//! Annotations other tools wrote are kept: an edit changes only the
//! element's MIRIAM terms and leaves its other annotation content, its
//! history and its notes as they were. An element whose RDF this library
//! cannot take apart is not edited at all ([`EditError::UnreadableRdf`]).
//!
//! No I/O; works in WebAssembly.

mod edit;
mod rename;
mod resolve;
mod uris;
mod view;

pub use edit::{EditError, Term};
pub use rename::{RenameError, is_valid_id, references_left};
pub use resolve::{Resolution, resolve};
pub use edit::BIOLOGY_QUALIFIERS;
pub use uris::{sbo_for, uri, uri_from_text, uris_for};

use std::fmt;

/// Where the model came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Sbml,
    /// Converted to SBML Level 3 Version 2 on loading.
    Antimony,
}

/// An element that can carry annotations. Indices are into the model's
/// lists, as in [`annot_engine::Model`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Element {
    Model,
    Compartment(usize),
    Species(usize),
    Reaction(usize),
}

/// Why a model could not be loaded.
#[derive(Debug, Clone, PartialEq)]
pub enum LoadError {
    /// The SBML reader's first complaint.
    Sbml(String),
    /// The Antimony reader's errors.
    Antimony(String),
    /// The document has no model.
    NoModel,
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::Sbml(e) => write!(f, "not readable as SBML: {e}"),
            LoadError::Antimony(e) => write!(f, "not readable as Antimony: {e}"),
            LoadError::NoModel => write!(f, "the document has no model"),
        }
    }
}

impl std::error::Error for LoadError {}

/// One change, kept for undo.
#[derive(Clone, Debug)]
enum Change {
    /// An element's annotation, SBO term or other metadata.
    Meta {
        element: Element,
        before: Box<sbml_io::Meta>,
        after: Box<sbml_io::Meta>,
    },
    /// A change reaching all over the document (a rename).
    Document {
        before: Box<sbml_io::Document>,
        after: Box<sbml_io::Document>,
    },
    /// Several changes undone and redone together ([`Workspace::batch`]).
    Group(Vec<Change>),
}

/// A model being annotated.
pub struct Workspace {
    doc: sbml_io::Document,
    format: Format,
    /// Things worth telling the user about the load (reader warnings).
    pub warnings: Vec<String>,
    undo: Vec<Change>,
    redo: Vec<Change>,
}

/// Loads SBML (text starting with `<`) or Antimony (anything else).
pub fn load(text: &str) -> Result<Workspace, LoadError> {
    if text.trim_start().starts_with('<') {
        load_sbml(text)
    } else {
        load_antimony(text)
    }
}

/// Loads SBML (Level 2 Versions 1-5, Level 3 Versions 1-2).
pub fn load_sbml(text: &str) -> Result<Workspace, LoadError> {
    let read = sbml_io::read(text).map_err(|d| {
        LoadError::Sbml(d.first().map(|d| format!("{d:?}")).unwrap_or_else(|| "unreadable".into()))
    })?;
    if read.document.model.is_none() {
        return Err(LoadError::NoModel);
    }
    Ok(Workspace {
        doc: read.document,
        format: Format::Sbml,
        warnings: read.warnings.iter().map(|w| format!("{w:?}")).collect(),
        undo: Vec::new(),
        redo: Vec::new(),
    })
}

/// Loads Antimony, converting it to SBML Level 3 Version 2.
pub fn load_antimony(text: &str) -> Result<Workspace, LoadError> {
    let loaded = antimony::parse(text);
    if loaded.has_errors() {
        return Err(LoadError::Antimony(loaded.diagnostics.to_string()));
    }
    let doc = antimony::sbml::to_sbml(&antimony::flatten::flatten(&loaded.document));
    if doc.model.is_none() {
        return Err(LoadError::NoModel);
    }
    Ok(Workspace {
        doc,
        format: Format::Antimony,
        warnings: loaded.diagnostics.iter().map(|d| d.message.clone()).collect(),
        undo: Vec::new(),
        redo: Vec::new(),
    })
}

impl Workspace {
    pub fn format(&self) -> Format {
        self.format
    }

    /// The SBML document as it now stands.
    pub fn document(&self) -> &sbml_io::Document {
        &self.doc
    }

    fn model(&self) -> &sbml_io::Model {
        self.doc.model.as_ref().expect("a workspace always has a model")
    }

    /// The annotated model as SBML text, in the level and version it was
    /// read in (Level 3 Version 2 for Antimony).
    pub fn to_sbml(&self) -> String {
        sbml_io::write(&self.doc)
    }

    /// Whether there is an edit to undo or redo.
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Undoes the last edit. False if there was none.
    pub fn undo(&mut self) -> bool {
        let Some(c) = self.undo.pop() else {
            return false;
        };
        self.revert(&c);
        self.redo.push(c);
        true
    }

    /// Redoes the last undone edit. False if there was none.
    pub fn redo(&mut self) -> bool {
        let Some(c) = self.redo.pop() else {
            return false;
        };
        self.reapply(&c);
        self.undo.push(c);
        true
    }

    /// Makes every edit `f` makes one undoable step: Undo reverts them all
    /// at once. Edits that change nothing are not recorded, and a batch
    /// with no edits adds no step.
    pub fn batch<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        let start = self.undo.len();
        let r = f(self);
        if self.undo.len() > start + 1 {
            let group = self.undo.split_off(start);
            self.undo.push(Change::Group(group));
        }
        r
    }

    fn revert(&mut self, c: &Change) {
        match c {
            Change::Meta { element, before, .. } => *self.meta_mut(*element) = (**before).clone(),
            Change::Document { before, .. } => self.doc = (**before).clone(),
            Change::Group(cs) => cs.iter().rev().for_each(|c| self.revert(c)),
        }
    }

    fn reapply(&mut self, c: &Change) {
        match c {
            Change::Meta { element, after, .. } => *self.meta_mut(*element) = (**after).clone(),
            Change::Document { after, .. } => self.doc = (**after).clone(),
            Change::Group(cs) => cs.iter().for_each(|c| self.reapply(c)),
        }
    }

    fn meta(&self, el: Element) -> &sbml_io::Meta {
        let m = self.model();
        match el {
            Element::Model => &m.meta,
            Element::Compartment(i) => &m.compartments[i].meta,
            Element::Species(i) => &m.species[i].meta,
            Element::Reaction(i) => &m.reactions[i].meta,
        }
    }

    fn meta_mut(&mut self, el: Element) -> &mut sbml_io::Meta {
        let m = self.doc.model.as_mut().expect("a workspace always has a model");
        match el {
            Element::Model => &mut m.meta,
            Element::Compartment(i) => &mut m.compartments[i].meta,
            Element::Species(i) => &mut m.species[i].meta,
            Element::Reaction(i) => &mut m.reactions[i].meta,
        }
    }

    /// The SBML id of an element ("" for a model without one).
    pub fn element_id(&self, el: Element) -> &str {
        let m = self.model();
        match el {
            Element::Model => m.id.as_deref().unwrap_or(""),
            Element::Compartment(i) => &m.compartments[i].id,
            Element::Species(i) => &m.species[i].id,
            Element::Reaction(i) => &m.reactions[i].id,
        }
    }

    /// Whether `el` exists in this model.
    pub fn contains(&self, el: Element) -> bool {
        let m = self.model();
        match el {
            Element::Model => true,
            Element::Compartment(i) => i < m.compartments.len(),
            Element::Species(i) => i < m.species.len(),
            Element::Reaction(i) => i < m.reactions.len(),
        }
    }
}
