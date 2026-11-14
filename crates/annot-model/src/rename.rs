//! Renaming an element's SBML id, and every reference to it.
//!
//! An id is referred to from many places: reaction participants, MathML in
//! kinetic laws, rules, initial assignments, events and constraints, rule
//! and assignment targets, compartment references, conversion factors, and
//! the Layout package's glyphs. [`Workspace::rename`] changes all of them
//! on a copy of the document, then writes the copy out and checks that the
//! old id is no longer referred to anywhere. If it still is (in MathML or
//! another package's data that `sbml-io` keeps as raw XML), the rename is
//! refused and the model is left as it was: a half-done rename would break
//! the model.

use std::fmt;

use sbml_io::layout::{AdditionalGlyph, GeneralGlyph, Layout, ReactionGlyph};
use sbml_io::{Math, Model};

use crate::{Change, Element, Workspace};

/// Why a rename was refused. The model is unchanged.
#[derive(Clone, Debug, PartialEq)]
pub enum RenameError {
    NoSuchElement,
    /// Only species, reactions and compartments can be renamed.
    NotRenamable,
    /// The new id is the old one.
    Unchanged,
    /// Not a valid SBML id: letters, digits and "_", not starting with a
    /// digit.
    InvalidId(String),
    /// Something in the model already has this id.
    Taken(String),
    /// A reaction's rate law has a local parameter with the old or the new
    /// id, so a reference there would be ambiguous.
    LocalParameter { reaction: String, id: String },
    /// The old id is still referred to after renaming, in places this tool
    /// cannot update (raw MathML, another package's data). Up to three
    /// examples of where.
    Unreachable(Vec<String>),
}

impl fmt::Display for RenameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RenameError::NoSuchElement => write!(f, "no such element"),
            RenameError::NotRenamable => write!(f, "only species, reactions and compartments can be renamed"),
            RenameError::Unchanged => write!(f, "that is already its id"),
            RenameError::InvalidId(id) => write!(
                f,
                "\"{id}\" is not a valid SBML id: use letters, digits and \"_\", and do not start with a digit"
            ),
            RenameError::Taken(id) => write!(f, "\"{id}\" is already used in this model"),
            RenameError::LocalParameter { reaction, id } => write!(
                f,
                "reaction {reaction} has a local parameter called \"{id}\" in its rate law, \
                 so the rename would be ambiguous there"
            ),
            RenameError::Unreachable(places) => write!(
                f,
                "the model also refers to it in places this tool cannot update ({}), so renaming \
                 would break the model",
                places.join("; ")
            ),
        }
    }
}

impl std::error::Error for RenameError {}

/// Whether `id` is a valid SBML SId.
pub fn is_valid_id(id: &str) -> bool {
    let mut chars = id.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

impl Workspace {
    /// Renames an element's id to `new_id` and updates every reference to
    /// it. One undoable step. Returns how many references were updated
    /// (besides the id itself).
    pub fn rename(&mut self, el: Element, new_id: &str) -> Result<usize, RenameError> {
        if !self.contains(el) {
            return Err(RenameError::NoSuchElement);
        }
        if !matches!(el, Element::Species(_) | Element::Reaction(_) | Element::Compartment(_)) {
            return Err(RenameError::NotRenamable);
        }
        let new_id = new_id.trim();
        let old_id = self.element_id(el).to_string();
        if new_id == old_id {
            return Err(RenameError::Unchanged);
        }
        if !is_valid_id(new_id) {
            return Err(RenameError::InvalidId(new_id.to_string()));
        }
        let before_text = self.to_sbml();
        if before_text.contains(&format!(" id=\"{new_id}\"")) {
            return Err(RenameError::Taken(new_id.to_string()));
        }
        let model = self.model();
        for r in &model.reactions {
            if let Some(law) = &r.kinetic_law {
                for p in &law.parameters {
                    if p.id == old_id || p.id == new_id {
                        return Err(RenameError::LocalParameter {
                            reaction: r.id.clone(),
                            id: p.id.clone(),
                        });
                    }
                }
            }
        }

        let mut doc = self.doc.clone();
        let m = doc.model.as_mut().expect("a workspace always has a model");
        let changed = rename_in_model(m, &old_id, new_id);

        let after_text = sbml_io::write(&doc);
        let left = references_left(&after_text, &old_id);
        if !left.is_empty() {
            return Err(RenameError::Unreachable(left.into_iter().take(3).collect()));
        }

        let before = std::mem::replace(&mut self.doc, doc);
        self.undo.push(Change::Document {
            before: Box::new(before),
            after: Box::new(self.doc.clone()),
        });
        self.redo.clear();
        Ok(changed)
    }
}

/// Replaces `old` by `new` in one id field. Counts it.
fn swap(field: &mut String, old: &str, new: &str, n: &mut usize) {
    if field == old {
        *field = new.to_string();
        *n += 1;
    }
}

fn swap_opt(field: &mut Option<String>, old: &str, new: &str, n: &mut usize) {
    if let Some(f) = field {
        swap(f, old, new, n);
    }
}

/// Every id field and reference in the model, including the element's own
/// id (ids are unique, so any field equal to the old id means it). Returns
/// how many fields changed, the element's own id included.
fn rename_in_model(m: &mut Model, old: &str, new: &str) -> usize {
    let mut n = 0;
    swap_opt(&mut m.conversion_factor, old, new, &mut n);
    for c in &mut m.compartments {
        swap(&mut c.id, old, new, &mut n);
        swap_opt(&mut c.outside, old, new, &mut n);
    }
    for s in &mut m.species {
        swap(&mut s.id, old, new, &mut n);
        swap(&mut s.compartment, old, new, &mut n);
        swap_opt(&mut s.conversion_factor, old, new, &mut n);
    }
    for p in &mut m.parameters {
        swap(&mut p.id, old, new, &mut n);
    }
    for a in &mut m.initial_assignments {
        swap(&mut a.symbol, old, new, &mut n);
        rename_math_opt(&mut a.math, old, new, &mut n);
    }
    for r in &mut m.rules {
        match r {
            sbml_io::Rule::Assignment { variable, math, .. } | sbml_io::Rule::Rate { variable, math, .. } => {
                swap(variable, old, new, &mut n);
                rename_math_opt(math, old, new, &mut n);
            }
            sbml_io::Rule::Algebraic { math, .. } => rename_math_opt(math, old, new, &mut n),
        }
    }
    for c in &mut m.constraints {
        rename_math_opt(&mut c.math, old, new, &mut n);
    }
    for r in &mut m.reactions {
        swap(&mut r.id, old, new, &mut n);
        swap_opt(&mut r.compartment, old, new, &mut n);
        for sr in r.reactants.iter_mut().chain(r.products.iter_mut()) {
            swap(&mut sr.species, old, new, &mut n);
            rename_math_opt(&mut sr.stoichiometry_math, old, new, &mut n);
        }
        for mr in &mut r.modifiers {
            swap(&mut mr.species, old, new, &mut n);
        }
        if let Some(law) = &mut r.kinetic_law {
            rename_math_opt(&mut law.math, old, new, &mut n);
        }
    }
    for e in &mut m.events {
        swap_opt(&mut e.id, old, new, &mut n);
        if let Some(t) = &mut e.trigger {
            rename_math_opt(&mut t.math, old, new, &mut n);
        }
        if let Some(d) = &mut e.delay {
            rename_math_opt(&mut d.math, old, new, &mut n);
        }
        if let Some(p) = &mut e.priority {
            rename_math_opt(&mut p.math, old, new, &mut n);
        }
        for a in &mut e.assignments {
            swap(&mut a.variable, old, new, &mut n);
            rename_math_opt(&mut a.math, old, new, &mut n);
        }
    }
    for l in &mut m.layouts {
        rename_in_layout(l, old, new, &mut n);
    }
    n
}

fn rename_math_opt(math: &mut Option<Math>, old: &str, new: &str, n: &mut usize) {
    if let Some(m) = math {
        rename_math(m, old, new, n);
    }
}

/// `<ci>old</ci>` -> `<ci>new</ci>` throughout an expression. Inside a
/// lambda whose arguments include the old id, the argument shadows it.
fn rename_math(m: &mut Math, old: &str, new: &str, n: &mut usize) {
    match m {
        Math::Ident(id) => swap(id, old, new, n),
        Math::Apply(_, args) | Math::CallSymbol(_, args) => {
            for a in args {
                rename_math(a, old, new, n);
            }
        }
        Math::Call(name, args) => {
            swap(name, old, new, n);
            for a in args {
                rename_math(a, old, new, n);
            }
        }
        Math::Root { degree, arg } | Math::Log { base: degree, arg } => {
            if let Some(d) = degree {
                rename_math(d, old, new, n);
            }
            rename_math(arg, old, new, n);
        }
        Math::Piecewise { pieces, otherwise } => {
            for (v, c) in pieces {
                rename_math(v, old, new, n);
                rename_math(c, old, new, n);
            }
            if let Some(o) = otherwise {
                rename_math(o, old, new, n);
            }
        }
        Math::Lambda { args, body } => {
            if !args.iter().any(|a| a == old) {
                rename_math(body, old, new, n);
            }
        }
        Math::Semantics { math, .. } => rename_math(math, old, new, n),
        Math::Number { .. } | Math::Symbol(_) | Math::Constant(_) | Math::Raw(_) => {}
    }
}

fn rename_in_layout(l: &mut Layout, old: &str, new: &str, n: &mut usize) {
    for g in &mut l.compartment_glyphs {
        swap_opt(&mut g.compartment, old, new, n);
    }
    for g in &mut l.species_glyphs {
        swap_opt(&mut g.species, old, new, n);
    }
    for g in &mut l.reaction_glyphs {
        rename_in_reaction_glyph(g, old, new, n);
    }
    for g in &mut l.text_glyphs {
        swap_opt(&mut g.origin_of_text, old, new, n);
    }
    for g in &mut l.additional {
        rename_in_additional(g, old, new, n);
    }
}

fn rename_in_reaction_glyph(g: &mut ReactionGlyph, old: &str, new: &str, n: &mut usize) {
    swap_opt(&mut g.reaction, old, new, n);
}

fn rename_in_general(g: &mut GeneralGlyph, old: &str, new: &str, n: &mut usize) {
    swap_opt(&mut g.reference, old, new, n);
    for r in &mut g.reference_glyphs {
        swap_opt(&mut r.reference, old, new, n);
    }
    for s in &mut g.sub_glyphs {
        rename_in_additional(s, old, new, n);
    }
}

fn rename_in_additional(g: &mut AdditionalGlyph, old: &str, new: &str, n: &mut usize) {
    match g {
        AdditionalGlyph::General(g) => rename_in_general(g, old, new, n),
        AdditionalGlyph::Compartment(g) => swap_opt(&mut g.compartment, old, new, n),
        AdditionalGlyph::Species(g) => swap_opt(&mut g.species, old, new, n),
        AdditionalGlyph::Reaction(g) => rename_in_reaction_glyph(g, old, new, n),
        AdditionalGlyph::Text(g) => swap_opt(&mut g.origin_of_text, old, new, n),
        AdditionalGlyph::Graphical(_) => {}
    }
}

/// Where `old` is still referred to in SBML text: as an attribute value
/// (other than a `name`, which is free text, or a `metaid`, a separate
/// namespace) or as the content of a MathML `<ci>`. Each place is
/// described for people: `fbc:reaction="R_PFK"`, `<ci> s9 </ci>`.
///
/// Function definitions are skipped: SBML does not let them refer to the
/// model's ids, only to their own arguments, and models often name the
/// arguments after species (`lambda(S1, ...)`), which is not a reference.
pub fn references_left(text: &str, old: &str) -> Vec<String> {
    let text = &without_function_definitions(text);
    let mut out = Vec::new();
    let quoted = format!("=\"{old}\"");
    let mut from = 0;
    while let Some(i) = text[from..].find(&quoted) {
        let at = from + i;
        let attr_start = text[..at]
            .rfind(|c: char| c.is_whitespace())
            .map_or(0, |p| p + 1);
        let attr = &text[attr_start..at];
        if attr != "name" && attr != "metaid" {
            out.push(format!("{attr}=\"{old}\""));
        }
        from = at + quoted.len();
    }
    let mut from = 0;
    while let Some(i) = text[from..].find("<ci") {
        let start = from + i;
        let Some(gt) = text[start..].find('>') else { break };
        let content_start = start + gt + 1;
        let Some(end) = text[content_start..].find("</") else { break };
        if text[content_start..content_start + end].trim() == old {
            out.push(format!("<ci> {old} </ci>"));
        }
        from = content_start + end;
    }
    out
}

/// The text with every `<functionDefinition>...</functionDefinition>` (with
/// any namespace prefix) removed.
fn without_function_definitions(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    loop {
        let Some(start) = find_tag(rest, "functionDefinition", false) else {
            out.push_str(rest);
            return out;
        };
        out.push_str(&rest[..start]);
        let after = &rest[start..];
        // A self-closing definition, or one with content.
        let open_end = after.find('>').map_or(after.len(), |i| i + 1);
        if after[..open_end].ends_with("/>") {
            rest = &after[open_end..];
            continue;
        }
        match find_tag(after, "functionDefinition", true) {
            Some(close) => {
                let close_end = after[close..].find('>').map_or(after.len(), |i| close + i + 1);
                rest = &after[close_end..];
            }
            None => return out,
        }
    }
}

/// Where `<name` (or `</name` if `closing`), possibly with a namespace
/// prefix, starts in `text`.
fn find_tag(text: &str, name: &str, closing: bool) -> Option<usize> {
    let mut from = 0;
    while let Some(i) = text[from..].find(name) {
        let at = from + i;
        let before = &text[..at];
        let lt = before.rfind('<')?;
        let between = &before[lt + 1..];
        let prefix_ok = if closing {
            between.strip_prefix('/').is_some_and(|p| p.is_empty() || (p.ends_with(':') && !p.contains(char::is_whitespace)))
        } else {
            between.is_empty() || (between.ends_with(':') && !between.contains(|c: char| c.is_whitespace() || c == '/'))
        };
        let after_ok = text[at + name.len()..].starts_with(|c: char| c.is_whitespace() || c == '>' || c == '/');
        if prefix_ok && after_ok {
            return Some(lt);
        }
        from = at + name.len();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use sbml_io::layout::{SpeciesGlyph, TextGlyph};

    #[test]
    fn function_definitions_are_not_references() {
        let text = r#"<listOfFunctionDefinitions><functionDefinition id="f"><math><lambda><bvar><ci> S1 </ci></bvar><ci> S1 </ci></lambda></math></functionDefinition></listOfFunctionDefinitions><kineticLaw><math><ci> S1 </ci></math></kineticLaw>"#;
        assert_eq!(references_left(text, "S1"), ["<ci> S1 </ci>"], "only the kinetic law's");
        let prefixed = r#"<sbml:functionDefinition id="f"><ci> S1 </ci></sbml:functionDefinition>"#;
        assert!(references_left(prefixed, "S1").is_empty());
    }

    #[test]
    fn valid_ids() {
        assert!(is_valid_id("s9"));
        assert!(is_valid_id("_3pg"));
        assert!(is_valid_id("PG3"));
        assert!(!is_valid_id("3pg"));
        assert!(!is_valid_id("3-PG"));
        assert!(!is_valid_id(""));
        assert!(!is_valid_id("a b"));
    }

    #[test]
    fn layouts_follow() {
        let mut m = Model {
            species: vec![sbml_io::Species {
                id: "s9".into(),
                compartment: "cell".into(),
                ..sbml_io::Species::default()
            }],
            layouts: vec![Layout {
                id: "l".into(),
                species_glyphs: vec![SpeciesGlyph {
                    id: "g1".into(),
                    species: Some("s9".into()),
                    ..SpeciesGlyph::default()
                }],
                text_glyphs: vec![TextGlyph {
                    id: "t1".into(),
                    origin_of_text: Some("s9".into()),
                    graphical_object: Some("g1".into()),
                    ..TextGlyph::default()
                }],
                ..Layout::default()
            }],
            ..Model::default()
        };
        assert_eq!(rename_in_model(&mut m, "s9", "PG3"), 3);
        assert_eq!(m.species[0].id, "PG3");
        assert_eq!(m.layouts[0].species_glyphs[0].species.as_deref(), Some("PG3"));
        assert_eq!(m.layouts[0].text_glyphs[0].origin_of_text.as_deref(), Some("PG3"));
        // Glyph ids are a different thing and stay.
        assert_eq!(m.layouts[0].text_glyphs[0].graphical_object.as_deref(), Some("g1"));
    }

    #[test]
    fn lambda_arguments_shadow() {
        let mut f = Math::Lambda {
            args: vec!["s9".into()],
            body: Box::new(Math::Ident("s9".into())),
        };
        let mut n = 0;
        rename_math(&mut f, "s9", "PG3", &mut n);
        assert_eq!(n, 0);
    }

    #[test]
    fn leftover_references_are_found() {
        let text = r#"<species id="PG3" name="s9" metaid="s9"/><fbc:fluxObjective fbc:reaction="s9"/><ci> s9 </ci>"#;
        let left = references_left(text, "s9");
        assert_eq!(left, ["fbc:reaction=\"s9\"", "<ci> s9 </ci>"]);
    }
}
