//! Decoder: for species and reactions the dictionary found nothing for,
//! looks up other readings of their names and ids ([`crate::decode`]), and
//! database ids embedded in their SBML ids ("M_C03506_cytop" holds KEGG
//! C03506).
//!
//! A hit on a reading weighs what the same hit on the name as written would
//! ([`super::names::weight`]), times the reading's factor. An embedded
//! database id that names exactly one thing in the pack weighs
//! [`EMBEDDED_ID_WEIGHT`]. Where several readings lead to the same target,
//! the strongest counts.
//!
//! It only runs where [`super::names`] found nothing, so it never weakens a
//! direct match.

use std::collections::BTreeMap;

use kpack::{Pack, Target};

use crate::decode::{Vocab, embedded_ids, readings};
use crate::providers::names;
use crate::{Candidate, Evidence, Model, Provider, Suggestions};

pub struct Decoder;

/// The weight of a database id found inside an SBML id: as strong as a
/// preferred name, since it is an identifier, not a guess.
pub const EMBEDDED_ID_WEIGHT: f32 = 2.0;

impl Provider for Decoder {
    fn name(&self) -> &'static str {
        "decoder"
    }

    fn run(&self, pack: &Pack, model: &Model, out: &mut Suggestions) {
        let vocab = Vocab::new(pack, model);
        for (s, list) in model.species.iter().zip(out.species.iter_mut()) {
            if list.is_empty() {
                decode(pack, &vocab, false, s.name.as_deref(), &s.id, list);
            }
        }
        for (r, list) in model.reactions.iter().zip(out.reactions.iter_mut()) {
            if list.is_empty() {
                decode(pack, &vocab, true, r.name.as_deref(), &r.id, list);
            }
        }
    }
}

fn decode(pack: &Pack, vocab: &Vocab, reaction: bool, name: Option<&str>, id: &str, out: &mut Vec<Candidate>) {
    let mut best: BTreeMap<Target, (f32, String)> = BTreeMap::new();
    let mut offer = |t: Target, w: f32, note: &dyn Fn() -> String| {
        if best.get(&t).is_none_or(|(bw, _)| w > *bw) {
            best.insert(t, (w, note()));
        }
    };

    for (field, text, on_id) in names::fields(name, id) {
        if on_id {
            for (prefix, xid) in embedded_ids(text, reaction) {
                let targets = pack.find_xref(prefix, &xid);
                if let [t] = targets.as_slice() {
                    let label = pack
                        .namespace_id(prefix)
                        .map(|ns| pack.str(pack.namespaces[ns as usize].label))
                        .unwrap_or(prefix);
                    offer(*t, EMBEDDED_ID_WEIGHT, &|| format!("{field} \"{text}\" contains the {label} id {xid}"));
                }
            }
        }
        for r in readings(text, on_id, vocab) {
            let hits = if reaction {
                pack.reactions_named(&r.text)
            } else {
                pack.entities_named(&r.text)
            };
            for hit in hits {
                let t = if reaction {
                    Target::Reaction(hit.target)
                } else {
                    Target::Entity(hit.target)
                };
                let w = names::weight(hit, on_id) * r.factor;
                offer(t, w, &|| {
                    format!(
                        "{field} \"{text}\" read as \"{}\" ({}): {}",
                        r.text,
                        r.describe(),
                        names::describe_hit(hit)
                    )
                });
            }
        }
    }
    for (t, (weight, note)) in best {
        Suggestions::add(
            out,
            t,
            Evidence {
                provider: "decoder",
                weight,
                note,
                complete: false,
            },
        );
    }
}
