//! Reading and changing an element's annotation terms.

use std::fmt;

use kpack::{Pack, Target};
use sbml_io::miriam::{self, CvTerm};

use crate::resolve::{Resolution, resolve};
use crate::uris::{sbo_for, uris_for};
use crate::{Change, Element, Workspace};

/// One annotation term: a qualifier and a resource.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Term {
    /// `is`, `isVersionOf`, `hasPart`, ...
    pub qualifier: String,
    /// A model qualifier (`bqmodel:`) rather than a biology one (`bqbiol:`).
    pub model: bool,
    pub uri: String,
}

/// Why an edit was refused. The model is unchanged.
#[derive(Clone, Debug, PartialEq)]
pub enum EditError {
    NoSuchElement,
    /// The element's RDF is in a form this library cannot take apart, so
    /// rewriting it could lose information.
    UnreadableRdf,
    UnknownQualifier(String),
    /// SBML Level 2 Version 1 has no sboTerm.
    SboNotSupported,
}

impl fmt::Display for EditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EditError::NoSuchElement => write!(f, "no such element"),
            EditError::UnreadableRdf => {
                write!(f, "its annotation is in a form that cannot be edited safely")
            }
            EditError::UnknownQualifier(q) => write!(f, "unknown qualifier {q:?}"),
            EditError::SboNotSupported => write!(f, "SBML Level 2 Version 1 has no SBO terms"),
        }
    }
}

impl std::error::Error for EditError {}

/// The biology qualifiers (`bqbiol:`) of the MIRIAM specification.
pub const BIOLOGY_QUALIFIERS: &[&str] = &[
    "is", "hasPart", "isPartOf", "isVersionOf", "hasVersion", "isHomologTo", "isDescribedBy",
    "isEncodedBy", "encodes", "occursIn", "hasProperty", "isPropertyOf", "hasTaxon",
];

impl Workspace {
    fn read_annotation(&self, el: Element) -> miriam::Annotation {
        self.meta(el)
            .annotation
            .as_deref()
            .map(|x| miriam::read_annotation(x, &self.doc.namespaces))
            .unwrap_or_default()
    }

    /// Every MIRIAM term on an element.
    pub fn terms(&self, el: Element) -> Vec<Term> {
        if !self.contains(el) {
            return Vec::new();
        }
        self.read_annotation(el)
            .terms
            .into_iter()
            .flat_map(|t| {
                let (q, model) = (t.qualifier, t.model);
                t.resources.into_iter().map(move |uri| Term {
                    qualifier: q.clone(),
                    model,
                    uri,
                })
            })
            .collect()
    }

    /// The resources of an element's `bqbiol:is` and `isVersionOf` terms:
    /// what it is said to be.
    pub fn identity_uris(&self, el: Element) -> Vec<String> {
        self.terms(el)
            .into_iter()
            .filter(|t| !t.model && (t.qualifier == "is" || t.qualifier == "isVersionOf"))
            .map(|t| t.uri)
            .collect()
    }

    /// What an element is already annotated as, in the pack.
    pub fn resolution(&self, pack: &Pack, el: Element) -> Resolution {
        resolve(pack, &self.identity_uris(el), matches!(el, Element::Reaction(_)))
    }

    /// Adds biology-qualifier terms (`bqbiol:<qualifier>`). Resources already
    /// there are not added twice.
    pub fn add_terms(&mut self, el: Element, qualifier: &str, uris: &[String]) -> Result<(), EditError> {
        let meta = self.with_terms(el, |terms| add(terms, qualifier, uris))?;
        self.apply(el, meta);
        Ok(())
    }

    /// Removes one biology-qualifier term. Removing a term that is not there
    /// changes nothing.
    pub fn remove_term(&mut self, el: Element, qualifier: &str, uri: &str) -> Result<(), EditError> {
        check_qualifier(qualifier)?;
        let meta = self.with_terms(el, |terms| {
            for t in terms.iter_mut().filter(|t| !t.model && t.qualifier == qualifier) {
                t.resources.retain(|r| r != uri);
            }
            Ok(())
        })?;
        self.apply(el, meta);
        Ok(())
    }

    /// Sets or clears an element's SBO term.
    pub fn set_sbo(&mut self, el: Element, sbo: Option<u32>) -> Result<(), EditError> {
        if !self.contains(el) {
            return Err(EditError::NoSuchElement);
        }
        if self.doc.level == 2 && self.doc.version == 1 {
            return Err(EditError::SboNotSupported);
        }
        let mut meta = self.meta(el).clone();
        meta.sbo_term = sbo.map(|n| format!("SBO:{n:07}"));
        self.apply(el, meta);
        Ok(())
    }

    /// Accepts a suggestion: adds `bqbiol:is` terms for `target`
    /// ([`uris_for`]) and, if the element has no SBO term and the document
    /// can carry one, the matching SBO term ([`sbo_for`]). One undoable
    /// edit. Returns the URIs added.
    pub fn accept(&mut self, pack: &Pack, el: Element, target: Target) -> Result<Vec<String>, EditError> {
        let uris = uris_for(pack, target);
        let mut meta = self.with_terms(el, |terms| add(terms, "is", &uris))?;
        let sbo_allowed = !(self.doc.level == 2 && self.doc.version == 1);
        if meta.sbo_term.is_none() && sbo_allowed {
            meta.sbo_term = Some(format!("SBO:{:07}", sbo_for(pack, target)));
        }
        self.apply(el, meta);
        Ok(uris)
    }

    /// The element's metadata with its terms changed by `f`, everything
    /// else in its annotation kept.
    fn with_terms(
        &self,
        el: Element,
        f: impl FnOnce(&mut Vec<CvTerm>) -> Result<(), EditError>,
    ) -> Result<sbml_io::Meta, EditError> {
        if !self.contains(el) {
            return Err(EditError::NoSuchElement);
        }
        let before = self.meta(el).clone();
        let ann = self.read_annotation(el);
        if ann.other.iter().any(|o| o.is_rdf()) {
            return Err(EditError::UnreadableRdf);
        }
        let mut terms = ann.terms.clone();
        f(&mut terms)?;
        terms.retain(|t| !t.resources.is_empty());
        if terms == ann.terms {
            return Ok(before);
        }
        let metaid = match &before.metaid {
            Some(m) => m.clone(),
            None => self.new_metaid(self.element_id(el)),
        };
        let rdf = miriam::write_annotation(&metaid, &terms, &ann.history, &ann.descriptions);
        let mut parts: Vec<String> = ann.other.iter().map(|o| o.xml.clone()).collect();
        parts.extend(rdf);
        let mut meta = before;
        meta.annotation = (!parts.is_empty()).then(|| parts.join("\n"));
        if meta.annotation.is_some() {
            meta.metaid = Some(metaid);
        }
        Ok(meta)
    }

    /// Records and makes a change, unless it changes nothing.
    fn apply(&mut self, el: Element, after: sbml_io::Meta) {
        let before = self.meta(el).clone();
        if before == after {
            return;
        }
        *self.meta_mut(el) = after.clone();
        self.undo.push(Change {
            element: el,
            before,
            after,
        });
        self.redo.clear();
    }

    /// A metaid no element in the document uses yet: `meta_<id>`, with a
    /// number added if needed.
    fn new_metaid(&self, id: &str) -> String {
        let text = self.to_sbml();
        let taken = |m: &str| text.contains(&format!("metaid=\"{m}\""));
        let base = if id.is_empty() { "meta".to_string() } else { format!("meta_{id}") };
        if !taken(&base) {
            return base;
        }
        (2..).map(|n| format!("{base}_{n}")).find(|m| !taken(m)).expect("some number is free")
    }
}

fn check_qualifier(q: &str) -> Result<(), EditError> {
    if BIOLOGY_QUALIFIERS.contains(&q) {
        Ok(())
    } else {
        Err(EditError::UnknownQualifier(q.to_string()))
    }
}

/// Adds `uris` to the biology term with `qualifier`, creating it if needed.
fn add(terms: &mut Vec<CvTerm>, qualifier: &str, uris: &[String]) -> Result<(), EditError> {
    check_qualifier(qualifier)?;
    let i = match terms.iter().position(|t| !t.model && t.qualifier == qualifier) {
        Some(i) => i,
        None => {
            terms.push(CvTerm {
                model: false,
                qualifier: qualifier.to_string(),
                resources: Vec::new(),
            });
            terms.len() - 1
        }
    };
    for u in uris {
        if !terms[i].resources.contains(u) {
            terms[i].resources.push(u.clone());
        }
    }
    Ok(())
}
