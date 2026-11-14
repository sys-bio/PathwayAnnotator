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

/// The model qualifiers (`bqmodel:`) of the MIRIAM specification: what a
/// model is, the papers describing it, what it was derived from.
pub const MODEL_QUALIFIERS: &[&str] = &["is", "isDescribedBy", "isDerivedFrom", "isInstanceOf", "hasInstance"];

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

    /// The organism the model is about: the NCBI taxonomy id of its
    /// `bqbiol:hasTaxon` term ("https://identifiers.org/taxonomy:9606",
    /// "urn:miriam:taxonomy:9606", ".../taxonomy/9606", ".../NCBITaxon_9606"),
    /// if it has one.
    pub fn model_taxon(&self) -> Option<u32> {
        self.terms(Element::Model)
            .into_iter()
            .filter(|t| !t.model && t.qualifier == "hasTaxon")
            .find_map(|t| {
                let lower = t.uri.to_ascii_lowercase();
                let at = ["taxonomy", "ncbitaxon"].iter().find_map(|w| lower.find(w).map(|i| i + w.len()))?;
                let digits: String = t.uri[at..]
                    .trim_start_matches([':', '_', '/'])
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect();
                digits.parse().ok()
            })
    }

    /// What an element is already annotated as, in the pack.
    pub fn resolution(&self, pack: &Pack, el: Element) -> Resolution {
        resolve(pack, &self.identity_uris(el), matches!(el, Element::Reaction(_)))
    }

    /// Adds biology-qualifier terms (`bqbiol:<qualifier>`). Resources already
    /// there are not added twice.
    pub fn add_terms(&mut self, el: Element, qualifier: &str, uris: &[String]) -> Result<(), EditError> {
        self.add_terms_as(el, false, qualifier, uris)
    }

    /// Adds terms with a model qualifier (`bqmodel:`, `model` true) or a
    /// biology one. Resources already there are not added twice.
    pub fn add_terms_as(&mut self, el: Element, model: bool, qualifier: &str, uris: &[String]) -> Result<(), EditError> {
        let meta = self.with_terms(el, |terms| add(terms, model, qualifier, uris))?;
        self.apply(el, meta);
        Ok(())
    }

    /// Removes one biology-qualifier term. Removing a term that is not there
    /// changes nothing.
    pub fn remove_term(&mut self, el: Element, qualifier: &str, uri: &str) -> Result<(), EditError> {
        self.remove_term_as(el, false, qualifier, uri)
    }

    /// Removes one term with a model (`model` true) or biology qualifier.
    pub fn remove_term_as(&mut self, el: Element, model: bool, qualifier: &str, uri: &str) -> Result<(), EditError> {
        check_qualifier(model, qualifier)?;
        let meta = self.with_terms(el, |terms| {
            for t in terms.iter_mut().filter(|t| t.model == model && t.qualifier == qualifier) {
                t.resources.retain(|r| r != uri);
            }
            Ok(())
        })?;
        self.apply(el, meta);
        Ok(())
    }

    /// An element's SBO term, as a number (`SBO:0000247` -> 247).
    pub fn sbo(&self, el: Element) -> Option<u32> {
        if !self.contains(el) {
            return None;
        }
        let t = self.meta(el).sbo_term.as_deref()?;
        t.trim().trim_start_matches("SBO:").parse().ok()
    }

    /// Moves one resource from one biology qualifier to another ("is" to
    /// "isVersionOf"), keeping its place in the annotation otherwise. One
    /// undoable edit; nothing changes if the resource is not under `from`.
    pub fn change_qualifier(&mut self, el: Element, uri: &str, from: &str, to: &str) -> Result<(), EditError> {
        self.change_qualifier_as(el, uri, (false, from), (false, to))
    }

    /// As [`Workspace::change_qualifier`], between any qualifiers: each is
    /// (model qualifier?, name), so a resource can move from
    /// `bqbiol:isVersionOf` to `bqmodel:isDescribedBy`.
    pub fn change_qualifier_as(
        &mut self,
        el: Element,
        uri: &str,
        from: (bool, &str),
        to: (bool, &str),
    ) -> Result<(), EditError> {
        check_qualifier(from.0, from.1)?;
        check_qualifier(to.0, to.1)?;
        if from == to {
            return Ok(());
        }
        let meta = self.with_terms(el, |terms| {
            let mut moved = false;
            for t in terms.iter_mut().filter(|t| t.model == from.0 && t.qualifier == from.1) {
                let before = t.resources.len();
                t.resources.retain(|r| r != uri);
                moved |= t.resources.len() != before;
            }
            if moved {
                add(terms, to.0, to.1, &[uri.to_string()])?;
            }
            Ok(())
        })?;
        self.apply(el, meta);
        Ok(())
    }

    /// Sets what organism the model is about: replaces its `bqbiol:hasTaxon`
    /// terms with the NCBI taxonomy id `taxon`, or removes them (`None`).
    /// One undoable edit.
    pub fn set_model_taxon(&mut self, taxon: Option<u32>) -> Result<(), EditError> {
        let meta = self.with_terms(Element::Model, |terms| {
            for t in terms.iter_mut().filter(|t| !t.model && t.qualifier == "hasTaxon") {
                t.resources.clear();
            }
            match taxon {
                Some(id) => add(terms, false, "hasTaxon", &[format!("https://identifiers.org/taxonomy:{id}")]),
                None => Ok(()),
            }
        })?;
        self.apply(Element::Model, meta);
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
        let mut meta = self.with_terms(el, |terms| add(terms, false, "is", &uris))?;
        let sbo_allowed = !(self.doc.level == 2 && self.doc.version == 1);
        if meta.sbo_term.is_none() && sbo_allowed {
            meta.sbo_term = Some(format!("SBO:{:07}", sbo_for(pack, target)));
        }
        self.apply(el, meta);
        Ok(uris)
    }

    /// Accepts an entry found outside the pack (a ChEBI, UniProt or Rhea
    /// URI): adds it as a `bqbiol:is` term and, if the element has no SBO
    /// term and the document can carry one, sets `sbo`. One undoable edit.
    pub fn accept_uri(&mut self, el: Element, uri: &str, sbo: Option<u32>) -> Result<(), EditError> {
        let mut meta = self.with_terms(el, |terms| add(terms, false, "is", &[uri.to_string()]))?;
        let sbo_allowed = !(self.doc.level == 2 && self.doc.version == 1);
        if let Some(n) = sbo
            && meta.sbo_term.is_none()
            && sbo_allowed
        {
            meta.sbo_term = Some(format!("SBO:{n:07}"));
        }
        self.apply(el, meta);
        Ok(())
    }

    /// Replaces what an element is identified as: removes its `bqbiol:is`
    /// and `isVersionOf` terms that lead to anything in the pack, then
    /// accepts `target` as [`Workspace::accept`] does. Terms the pack does
    /// not know (a protein, another database) are kept. One undoable edit.
    /// Returns the URIs added.
    pub fn replace_identity(&mut self, pack: &Pack, el: Element, target: Target) -> Result<Vec<String>, EditError> {
        let uris = uris_for(pack, target);
        let mut meta = self.with_terms(el, |terms| {
            for t in terms.iter_mut().filter(|t| !t.model && (t.qualifier == "is" || t.qualifier == "isVersionOf")) {
                t.resources.retain(|r| pack.find_resource(r).is_empty());
            }
            add(terms, false, "is", &uris)
        })?;
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
        self.with_annotation(el, |terms, _| f(terms))
    }

    /// The element's metadata with its terms and history changed by `f`,
    /// everything else in its annotation kept.
    pub(crate) fn with_annotation(
        &self,
        el: Element,
        f: impl FnOnce(&mut Vec<CvTerm>, &mut miriam::History) -> Result<(), EditError>,
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
        let mut history = ann.history.clone();
        f(&mut terms, &mut history)?;
        terms.retain(|t| !t.resources.is_empty());
        if terms == ann.terms && history == ann.history {
            return Ok(before);
        }
        let metaid = match &before.metaid {
            Some(m) => m.clone(),
            None => self.new_metaid(self.element_id(el)),
        };
        let rdf = miriam::write_annotation(&metaid, &terms, &history, &ann.descriptions);
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
    pub(crate) fn apply(&mut self, el: Element, after: sbml_io::Meta) {
        let before = self.meta(el).clone();
        if before == after {
            return;
        }
        *self.meta_mut(el) = after.clone();
        self.undo.push(Change::Meta {
            element: el,
            before: Box::new(before),
            after: Box::new(after),
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

fn check_qualifier(model: bool, q: &str) -> Result<(), EditError> {
    let known = if model { MODEL_QUALIFIERS } else { BIOLOGY_QUALIFIERS };
    if known.contains(&q) {
        Ok(())
    } else {
        Err(EditError::UnknownQualifier(q.to_string()))
    }
}

/// Adds `uris` to the term with `qualifier` (a model qualifier if `model`),
/// creating it if needed.
fn add(terms: &mut Vec<CvTerm>, model: bool, qualifier: &str, uris: &[String]) -> Result<(), EditError> {
    check_qualifier(model, qualifier)?;
    let i = match terms.iter().position(|t| t.model == model && t.qualifier == qualifier) {
        Some(i) => i,
        None => {
            terms.push(CvTerm {
                model,
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
