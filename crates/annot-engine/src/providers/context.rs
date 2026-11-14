//! Reaction context: what a species or reaction is, from its neighbors.
//!
//! - **Anchors.** A species whose best candidate is strong and well ahead
//!   of the next ([`ANCHOR_MIN`], [`ANCHOR_MARGIN`]) is taken to be that
//!   entity.
//! - **Matching.** A model reaction is compared with the pack reactions
//!   that contain its anchored, non-currency species. Direction is ignored
//!   (both orientations are tried) and so are compartments. Currency
//!   metabolites (ATP, H+, water, NAD, ...) are optional on both sides,
//!   since kinetic models often leave them out. A pack reaction fits if
//!   every anchored non-currency species is in it on the right side (one
//!   may be missing when three or more match: models lump steps).
//! - **Reactions.** Pack reactions that fit become candidates for the model
//!   reaction, weighted by how many non-currency species match.
//! - **Species.** For a species that is not an anchor, the pack reaction's
//!   participants left over on its side are candidates: one leftover is
//!   strong evidence ("Glc + ATP -> S3 + ADP" read as hexokinase leaves
//!   S3 = glucose 6-phosphate), two are weaker. Evidence from different
//!   reactions adds up; within one reaction the best fit counts.
//! - **Transport.** A pack transport reaction is considered only for a
//!   model reaction whose species span more than one compartment.
//! - **Two rounds.** Species found in the first round anchor the second, so
//!   identifications spread along a pathway.

use std::collections::{BTreeMap, BTreeSet};

use kpack::{EntityFlags, EntityId, Pack, ReactionFlags, ReactionId, Target};

use crate::{Evidence, Model, Provider, Role, Suggestions};

pub struct Context;

/// A species' best candidate must score at least this to be an anchor.
pub const ANCHOR_MIN: f32 = 1.4;
/// In the second round, a species found by context alone may anchor with a
/// lower score: one clean leftover is enough.
pub const ANCHOR_MIN_CONTEXT: f32 = 0.9;
/// ...and the second best at most this fraction of it.
pub const ANCHOR_MARGIN: f32 = 0.75;
/// Evidence for a species from one leftover.
pub const LEFTOVER_ONE: f32 = 1.2;
/// Evidence for each of two leftovers.
pub const LEFTOVER_TWO: f32 = 0.6;
/// Evidence for a currency leftover, when no other is left.
pub const LEFTOVER_CURRENCY: f32 = 0.5;
/// The most context evidence one species can get for one entity.
pub const SPECIES_CAP: f32 = 3.0;
/// Pack reactions kept per model reaction.
const MAX_FITS: usize = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Side {
    Left,
    Right,
}

/// How a pack reaction fits a model reaction.
#[derive(Clone, Debug)]
struct Fit {
    reaction: ReactionId,
    matched: usize,
    /// Anchored currency metabolites that matched too.
    currency_matched: usize,
    /// Currency metabolites in the pack reaction the model does not show.
    currency_left: usize,
    missing: usize,
    /// Pack participants nothing matched: (entity, side as the model reads it).
    leftover: Vec<(EntityId, Side)>,
    score: f32,
}

impl Provider for Context {
    fn name(&self) -> &'static str {
        "context"
    }

    fn run(&self, pack: &Pack, model: &Model, out: &mut Suggestions) {
        // Species the user confirmed (or the model already annotated) are
        // anchors whatever the other providers say.
        let confirmed: BTreeMap<usize, EntityId> = model
            .species
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.known.map(|e| (i, e)))
            .collect();
        let mut anchors1 = anchors(out, ANCHOR_MIN);
        anchors1.extend(confirmed.iter().map(|(i, e)| (*i, *e)));
        let learners: BTreeSet<usize> = (0..model.species.len()).filter(|i| !anchors1.contains_key(i)).collect();

        let rejected = |i: usize, e: EntityId| model.species[i].rejected.contains(&e);

        // Round 1: what context says about the species that are not anchors,
        // and what confirmed reactions say about their unconfirmed species.
        let first = species_evidence(pack, model, &anchors1, &learners);
        let mut trial = out.clone();
        for (i, map) in &first {
            for (e, (w, note)) in map {
                if !rejected(*i, *e) {
                    Suggestions::add(&mut trial.species[*i], Target::Entity(*e), ev(*w, note.clone(), false));
                }
            }
        }
        for (i, map) in pinned_by_reactions(pack, model, &anchors1, &confirmed) {
            for (e, (w, note, complete)) in map {
                if !rejected(i, e) {
                    Suggestions::add(&mut trial.species[i], Target::Entity(e), ev(w, note, complete));
                }
            }
        }
        let mut anchors2 = anchors1.clone();
        for (i, e) in anchors(&trial, ANCHOR_MIN_CONTEXT) {
            anchors2.entry(i).or_insert(e);
        }

        // Round 2, with the species round 1 found as anchors too.
        let second = species_evidence(pack, model, &anchors2, &learners);
        for (i, map) in second {
            for (e, (w, note)) in map {
                if !rejected(i, e) {
                    Suggestions::add(&mut out.species[i], Target::Entity(e), ev(w, note, false));
                }
            }
        }
        for (i, map) in pinned_by_reactions(pack, model, &anchors2, &confirmed) {
            for (e, (w, note, complete)) in map {
                if !rejected(i, e) {
                    Suggestions::add(&mut out.species[i], Target::Entity(e), ev(w, note, complete));
                }
            }
        }
        for (ri, r) in model.reactions.iter().enumerate() {
            let known = known_participants(model, ri, &anchors2, None);
            let fits = fits(pack, &known, spans_compartments(model, ri));
            // A complete match: every reactant and product is confirmed (not
            // just guessed: one wrong guess makes a "complete" match
            // complete for the wrong molecules), and exactly one database
            // reaction accounts for all of them, on the right sides, with
            // nothing else but cofactors left.
            let all_known = r
                .participants
                .iter()
                .filter(|p| p.role != Role::Modifier)
                .all(|p| confirmed.contains_key(&p.species));
            let complete: Vec<ReactionId> = if all_known {
                fits.iter()
                    .filter(|f| {
                        f.missing == 0
                            && f.matched + f.currency_matched == known.len()
                            && leftover_core(pack, f) == 0
                            && f.currency_left <= MAX_UNSHOWN_COFACTORS
                    })
                    .map(|f| f.reaction)
                    .collect()
            } else {
                Vec::new()
            };
            let the_complete = (complete.len() == 1).then(|| complete[0]);
            for f in fits {
                let unknown = r
                    .participants
                    .iter()
                    .filter(|p| p.role != Role::Modifier && !anchors2.contains_key(&p.species))
                    .count();
                let extra = leftover_core(pack, &f).saturating_sub(unknown);
                // Matched cofactors count a little: "F6P + ATP -> FBP + ADP"
                // is the kinase, not the phosphatase run backwards.
                let is_complete = the_complete == Some(f.reaction);
                let w = 0.8 + 0.6 * f.matched as f32 + 0.15 * f.currency_matched as f32
                    - 0.6 * f.missing as f32
                    - 0.2 * extra as f32
                    - 0.1 * f.currency_left as f32
                    + if is_complete { COMPLETE_BONUS } else { 0.0 };
                if w <= 0.0 {
                    continue;
                }
                let names: Vec<&str> = known
                    .iter()
                    .filter(|(e, _)| !is_currency(pack, *e))
                    .map(|(e, _)| pack.str(pack.entity(*e).label))
                    .collect();
                let note = if is_complete {
                    format!(
                        "all {} of its molecules are identified and match {} exactly, and no other reaction",
                        known.len(),
                        pack.str(pack.reaction(f.reaction).label)
                    )
                } else {
                    format!(
                        "its species {} fit {} ({} matched{})",
                        names.join(", "),
                        pack.str(pack.reaction(f.reaction).label),
                        f.matched,
                        if f.missing > 0 { format!(", {} missing", f.missing) } else { String::new() }
                    )
                };
                Suggestions::add(&mut out.reactions[ri], Target::Reaction(f.reaction), ev(w, note, is_complete));
            }
        }
    }
}

fn ev(weight: f32, note: String, complete: bool) -> Evidence {
    Evidence {
        provider: "context",
        weight,
        note,
        complete,
    }
}

/// Evidence for a species from a confirmed reaction with one molecule left
/// on its side.
pub const PINNED_ONE: f32 = 2.5;
/// Evidence for each of two molecules left on its side.
pub const PINNED_TWO: f32 = 0.8;
/// Evidence for a lone cofactor left on its side.
pub const PINNED_CURRENCY: f32 = 1.0;

/// What confirmed reactions say about their unconfirmed species: species ->
/// entity -> (weight, note, complete).
///
/// The database reaction is lined up with the model's (both directions are
/// tried; the one matching more known molecules wins), and what is left on
/// an unconfirmed species' side is evidence for it. One molecule left is
/// strong evidence; it is complete when every other reactant and product of
/// the reaction is confirmed, since the line-up then rests on nothing
/// guessed ("BPG + ADP -> s9 + ATP", confirmed as phosphoglycerate kinase,
/// makes s9 3-phosphoglycerate).
fn pinned_by_reactions(
    pack: &Pack,
    model: &Model,
    anchors: &BTreeMap<usize, EntityId>,
    confirmed: &BTreeMap<usize, EntityId>,
) -> BTreeMap<usize, BTreeMap<EntityId, (f32, String, bool)>> {
    let mut out: BTreeMap<usize, BTreeMap<EntityId, (f32, String, bool)>> = BTreeMap::new();
    for (ri, r) in model.reactions.iter().enumerate() {
        let Some(rid) = r.known else { continue };
        for p in &r.participants {
            if confirmed.contains_key(&p.species) {
                continue;
            }
            let side = match p.role {
                Role::Reactant => Side::Left,
                Role::Product => Side::Right,
                Role::Modifier => continue,
            };
            let known = known_participants(model, ri, anchors, Some(p.species));
            let Some(f) = [false, true]
                .into_iter()
                .filter_map(|flip| fit(pack, &known, rid, flip))
                .max_by(|a, b| a.score.total_cmp(&b.score))
            else {
                continue;
            };
            let on_side: Vec<EntityId> = f.leftover.iter().filter(|(_, s)| *s == side).map(|(e, _)| *e).collect();
            let core: Vec<EntityId> = on_side.iter().copied().filter(|e| !is_currency(pack, *e)).collect();
            let others_confirmed = r
                .participants
                .iter()
                .filter(|q| q.role != Role::Modifier && q.species != p.species)
                .all(|q| confirmed.contains_key(&q.species));
            let (picked, weight, complete) = match core.len() {
                1 => (core, PINNED_ONE, others_confirmed && f.missing == 0),
                2 => (core, PINNED_TWO, false),
                0 if on_side.len() == 1 => (on_side, PINNED_CURRENCY, false),
                _ => continue,
            };
            let role = if side == Side::Left { "substrate" } else { "product" };
            let note = format!(
                "in reaction \"{}\", which is identified as {}, it is the remaining {role}",
                r.name.as_deref().unwrap_or(&r.id),
                pack.str(pack.reaction(rid).label)
            );
            let mine = out.entry(p.species).or_default();
            for e in picked {
                let slot = mine.entry(e).or_insert((0.0, String::new(), false));
                if weight > slot.0 {
                    *slot = (weight, note.clone(), complete || slot.2);
                } else {
                    slot.2 |= complete;
                }
            }
        }
    }
    out
}

/// A complete match may leave this many of the database reaction's
/// cofactors unshown in the model (H+ and water, typically).
pub const MAX_UNSHOWN_COFACTORS: usize = 2;
/// Extra weight for a complete match.
pub const COMPLETE_BONUS: f32 = 0.5;

fn is_currency(pack: &Pack, e: EntityId) -> bool {
    pack.entity(e).flags.contains(EntityFlags::CURRENCY)
}

/// Species whose best candidate is clear, scoring at least `min`: species
/// index -> entity.
fn anchors(s: &Suggestions, min: f32) -> BTreeMap<usize, EntityId> {
    let mut out = BTreeMap::new();
    for (i, list) in s.species.iter().enumerate() {
        let mut best: Option<(f32, Target)> = None;
        let mut second = 0.0f32;
        for c in list {
            match best {
                Some((b, _)) if c.score <= b => second = second.max(c.score),
                Some((b, _)) => {
                    second = second.max(b);
                    best = Some((c.score, c.target));
                }
                None => best = Some((c.score, c.target)),
            }
        }
        if let Some((b, Target::Entity(e))) = best
            && b >= min
            && second <= ANCHOR_MARGIN * b
        {
            out.insert(i, e);
        }
    }
    out
}

/// The anchored participants of model reaction `ri`, with their sides,
/// leaving out species `except`.
fn known_participants(
    model: &Model,
    ri: usize,
    anchors: &BTreeMap<usize, EntityId>,
    except: Option<usize>,
) -> Vec<(EntityId, Side)> {
    model.reactions[ri]
        .participants
        .iter()
        .filter(|p| Some(p.species) != except)
        .filter_map(|p| {
            let side = match p.role {
                Role::Reactant => Side::Left,
                Role::Product => Side::Right,
                Role::Modifier => return None,
            };
            anchors.get(&p.species).map(|&e| (e, side))
        })
        .collect()
}

/// Whether model reaction `ri`'s reactants and products are in more than
/// one compartment: only then can it be a transport.
fn spans_compartments(model: &Model, ri: usize) -> bool {
    let comps: BTreeSet<Option<&str>> = model.reactions[ri]
        .participants
        .iter()
        .filter(|p| p.role != Role::Modifier)
        .map(|p| model.species[p.species].compartment.as_deref())
        .collect();
    comps.len() > 1
}

/// Pack reactions that fit the known participants, best first. Transport
/// reactions are considered only if `transport` (the model reaction spans
/// compartments): otherwise "IMP transport" fits any reaction with one
/// anchored IMP perfectly.
fn fits(pack: &Pack, known: &[(EntityId, Side)], transport: bool) -> Vec<Fit> {
    let core: Vec<EntityId> = known.iter().map(|(e, _)| *e).filter(|e| !is_currency(pack, *e)).collect();
    if core.is_empty() {
        return Vec::new();
    }
    let candidates: BTreeSet<ReactionId> = core.iter().flat_map(|e| pack.reactions_with(*e).iter().copied()).collect();
    let mut out: Vec<Fit> = candidates
        .into_iter()
        .filter(|&r| {
            let f = pack.reaction(r).flags;
            !f.contains(ReactionFlags::EXCHANGE)
                && !f.contains(ReactionFlags::PSEUDO)
                && (transport || !f.contains(ReactionFlags::TRANSPORT))
        })
        .filter_map(|r| {
            [false, true]
                .into_iter()
                .filter_map(|flip| fit(pack, known, r, flip))
                .max_by(|a, b| a.score.total_cmp(&b.score))
        })
        .collect();
    // Best first; among equal fits, the plainest BiGG id first ("PFK"
    // before its organism or compartment variant "PFKg").
    out.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| bigg_rank(pack, a.reaction).cmp(&bigg_rank(pack, b.reaction)))
    });
    // Keep one reaction per chemistry: BiGG has the same reaction under
    // several ids for different compartments and organisms.
    let mut seen = BTreeSet::new();
    out.retain(|f| seen.insert(chemistry(pack, f.reaction)));
    out.truncate(MAX_FITS);
    out
}

/// Sorts plain BiGG ids before variants: shorter first, then by text.
fn bigg_rank(pack: &Pack, r: ReactionId) -> (usize, String) {
    let id = pack
        .reaction(r)
        .xrefs
        .iter()
        .find(|x| pack.namespace_prefix(x.ns) == "bigg.reaction")
        .map(|x| pack.str(x.id).to_string())
        .unwrap_or_default();
    (if id.is_empty() { usize::MAX } else { id.len() }, id)
}

/// A reaction's participants and stoichiometry, ignoring compartments: the
/// same for a reaction's compartment variants.
fn chemistry(pack: &Pack, r: ReactionId) -> Vec<(EntityId, i64)> {
    let mut v: Vec<(EntityId, i64)> = pack
        .reaction(r)
        .participants
        .iter()
        .map(|p| (p.entity, (p.coef * 1000.0).round() as i64))
        .collect();
    v.sort_unstable();
    v
}

/// How pack reaction `r`, read forwards or (if `flip`) backwards, fits.
fn fit(pack: &Pack, known: &[(EntityId, Side)], r: ReactionId, flip: bool) -> Option<Fit> {
    let parts: Vec<(EntityId, Side)> = pack
        .reaction(r)
        .participants
        .iter()
        .map(|p| {
            let left = (p.coef < 0.0) != flip;
            (p.entity, if left { Side::Left } else { Side::Right })
        })
        .collect();
    let mut used = vec![false; parts.len()];
    let (mut matched, mut missing, mut currency_matched) = (0, 0, 0);
    for &(e, side) in known {
        match parts.iter().enumerate().position(|(i, p)| !used[i] && *p == (e, side)) {
            Some(i) => {
                used[i] = true;
                if is_currency(pack, e) {
                    currency_matched += 1;
                } else {
                    matched += 1;
                }
            }
            None if is_currency(pack, e) => {}
            None => missing += 1,
        }
    }
    let allowed_missing = usize::from(matched >= 3);
    if matched == 0 || missing > allowed_missing {
        return None;
    }
    let leftover: Vec<(EntityId, Side)> = parts
        .iter()
        .zip(&used)
        .filter(|(_, u)| !**u)
        .map(|(p, _)| *p)
        .collect();
    // Cofactors the model does not show are allowed (kinetic models omit
    // them) but count a little against the fit: "G6P -> X" is the
    // isomerase, not hexokinase backwards with ATP, ADP and H+ unexplained.
    let currency_left = leftover.iter().filter(|(e, _)| is_currency(pack, *e)).count();
    let score = matched as f32 + 0.25 * currency_matched as f32 - missing as f32 - 0.1 * currency_left as f32;
    Some(Fit {
        reaction: r,
        matched,
        currency_matched,
        currency_left,
        missing,
        leftover,
        score,
    })
}

/// Non-currency leftovers of a fit.
fn leftover_core(pack: &Pack, f: &Fit) -> usize {
    f.leftover.iter().filter(|(e, _)| !is_currency(pack, *e)).count()
}

type SpeciesEvidence = BTreeMap<usize, BTreeMap<EntityId, (f32, String)>>;

/// Context evidence for each species in `learners`: species -> entity ->
/// (weight, note).
fn species_evidence(
    pack: &Pack,
    model: &Model,
    anchors: &BTreeMap<usize, EntityId>,
    learners: &BTreeSet<usize>,
) -> SpeciesEvidence {
    let mut out: SpeciesEvidence = BTreeMap::new();
    for (ri, r) in model.reactions.iter().enumerate() {
        for p in &r.participants {
            if !learners.contains(&p.species) {
                continue;
            }
            let side = match p.role {
                Role::Reactant => Side::Left,
                Role::Product => Side::Right,
                Role::Modifier => continue,
            };
            let known = known_participants(model, ri, anchors, Some(p.species));
            // Within this reaction, the best evidence per entity.
            let mut here: BTreeMap<EntityId, (f32, String)> = BTreeMap::new();
            for f in fits(pack, &known, spans_compartments(model, ri)) {
                // More matched participants, more trust; matched cofactors
                // (ATP -> ADP in a kinase) count a little.
                let quality = (0.5 + 0.25 * f.matched as f32 + 0.1 * f.currency_matched as f32).min(1.0)
                    * if f.missing > 0 { 0.7 } else { 1.0 }
                    / (1.0 + 0.1 * f.currency_left as f32);
                let on_side: Vec<EntityId> = f.leftover.iter().filter(|(_, s)| *s == side).map(|(e, _)| *e).collect();
                let core: Vec<EntityId> = on_side.iter().copied().filter(|e| !is_currency(pack, *e)).collect();
                let (picked, base) = match core.len() {
                    1 => (core, LEFTOVER_ONE),
                    2 => (core, LEFTOVER_TWO),
                    0 if on_side.len() == 1 => (on_side, LEFTOVER_CURRENCY),
                    _ => continue,
                };
                let w = base * quality;
                for e in picked {
                    if here.get(&e).is_none_or(|(bw, _)| w > *bw) {
                        let role = if side == Side::Left { "substrate" } else { "product" };
                        let note = format!(
                            "in reaction \"{}\", read as {}, it is the remaining {role}",
                            r.name.as_deref().unwrap_or(&r.id),
                            pack.str(pack.reaction(f.reaction).label)
                        );
                        here.insert(e, (w, note));
                    }
                }
            }
            let mine = out.entry(p.species).or_default();
            for (e, (w, note)) in here {
                let slot = mine.entry(e).or_insert((0.0, String::new()));
                slot.0 = (slot.0 + w).min(SPECIES_CAP);
                if slot.1.is_empty() {
                    slot.1 = note;
                } else if !slot.1.ends_with("...") {
                    slot.1.push_str("; and more reactions agree...");
                }
            }
        }
    }
    out
}
