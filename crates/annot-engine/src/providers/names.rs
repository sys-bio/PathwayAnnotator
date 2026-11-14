//! Names: looks each species' and reaction's name and id up in the pack's
//! dictionary, exactly (after normalization: case, punctuation and charge
//! suffixes do not matter).
//!
//! This is the baseline provider. It does not decode names ("M_g6p_c",
//! "ERKpp"), and it does not look at reactions; later providers do.
//!
//! Weights:
//! - by how the name relates to what it names: a preferred name counts most,
//!   an id seen in models least;
//! - a match on the element's name counts more than one on its id;
//! - plus `0.5 * ln(1 + n)` for a name used with this meaning in `n` curated
//!   models, so "G6P" (17 models) beats a name seen once.

use kpack::{NameHit, NameKind, Pack, Target};

use crate::{Evidence, Model, Provider, Suggestions};

impl Provider for Names {
    fn name(&self) -> &'static str {
        "names"
    }

    fn run(&self, pack: &Pack, model: &Model, out: &mut Suggestions) {
        for (s, list) in model.species.iter().zip(out.species.iter_mut()) {
            look_up(
                |t| pack.entities_named(t).to_vec(),
                Target::Entity,
                s.name.as_deref(),
                &s.id,
                list,
            );
        }
        for (r, list) in model.reactions.iter().zip(out.reactions.iter_mut()) {
            look_up(
                |t| pack.reactions_named(t).to_vec(),
                Target::Reaction,
                r.name.as_deref(),
                &r.id,
                list,
            );
        }
    }
}

pub struct Names;

/// How strongly a name's kind says it names the target.
pub fn kind_weight(kind: NameKind) -> f32 {
    match kind {
        NameKind::Preferred => 2.0,
        NameKind::Synonym | NameKind::Abbreviation | NameKind::BiggId => 1.5,
        NameKind::ModelId => 1.0,
    }
}

/// A hit on an element's id counts this much of a hit on its name.
pub const ID_FACTOR: f32 = 0.8;

/// The weight of one dictionary hit.
pub fn weight(hit: &NameHit, on_id: bool) -> f32 {
    let field = if on_id { ID_FACTOR } else { 1.0 };
    kind_weight(hit.kind) * field + 0.5 * (1.0 + f32::from(hit.count)).ln()
}

/// For people: `its BiGG id (BiGG); used for it in 17 curated models`.
pub fn describe_hit(hit: &NameHit) -> String {
    let kind = match hit.kind {
        NameKind::Preferred => "its name",
        NameKind::Synonym => "a name for it",
        NameKind::Abbreviation => "an abbreviation for it",
        NameKind::BiggId => "its BiGG id",
        NameKind::ModelId => "an id curated models use for it",
    };
    let mut s = format!("{kind} ({})", hit.sources.names().join(", "));
    if hit.count > 0 {
        s.push_str(&format!("; used for it in {} curated models", hit.count));
    }
    s
}

fn note(field: &str, text: &str, hit: &NameHit) -> String {
    format!("{field} \"{text}\" is {}", describe_hit(hit))
}

/// The texts to look up for an element: `("name", text, false)` and
/// `("id", text, true)`. The id is left out when it is the name written
/// differently, and an empty name is left out.
pub fn fields<'a>(name: Option<&'a str>, id: &'a str) -> Vec<(&'static str, &'a str, bool)> {
    let mut fields = Vec::new();
    if let Some(n) = name.filter(|n| !n.trim().is_empty()) {
        fields.push(("name", n, false));
    }
    if name.is_none_or(|n| kpack::normalize_name(n) != kpack::normalize_name(id)) {
        fields.push(("id", id, true));
    }
    fields
}

/// Adds evidence from the name and the id of one element.
fn look_up(
    lookup: impl Fn(&str) -> Vec<NameHit>,
    target: impl Fn(u32) -> Target,
    name: Option<&str>,
    id: &str,
    out: &mut Vec<crate::Candidate>,
) {
    for (field, text, on_id) in fields(name, id) {
        for hit in lookup(text) {
            Suggestions::add(
                out,
                target(hit.target),
                Evidence {
                    provider: "names",
                    weight: weight(&hit, on_id),
                    note: note(field, text, &hit),
                },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kpack::Sources;

    #[test]
    fn weights_order_sensibly() {
        let hit = |kind, count| NameHit {
            target: 0,
            kind,
            sources: Sources::BIOMODELS,
            count,
        };
        // Used in many models beats used once.
        assert!(weight(&hit(NameKind::Synonym, 17), false) > weight(&hit(NameKind::Synonym, 1), false));
        // A name beats an id.
        assert!(weight(&hit(NameKind::Synonym, 0), false) > weight(&hit(NameKind::Synonym, 0), true));
        // A preferred name beats a model id.
        assert!(weight(&hit(NameKind::Preferred, 0), false) > weight(&hit(NameKind::ModelId, 0), false));
    }
}
