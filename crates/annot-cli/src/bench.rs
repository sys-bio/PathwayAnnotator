//! The accuracy benchmark: run the engine on curated models it has not
//! learned from, and grade its suggestions against the models' own
//! annotations.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use annot_engine::Candidate;
use kpack::{Pack, Target};

use crate::input::{Graded, Key};

/// Scores for one kind of element (species or reactions).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Score {
    /// Elements with a `bqbiol:is` / `isVersionOf` annotation.
    pub annotated: usize,
    /// Of those, annotated with nothing the pack knows.
    pub outside_pack: usize,
    /// Annotated with something in the pack: these are graded.
    pub gradable: usize,
    /// Gradable elements whose annotations tie between several targets
    /// (any of them is accepted).
    pub ties: usize,
    /// Gradable elements that got at least one suggestion.
    pub suggested: usize,
    pub top1: usize,
    pub top3: usize,
    pub top5: usize,
    /// Sum of 1/rank of the right answer (0 when absent).
    pub reciprocal_rank: f64,
    /// Gradable elements with exactly one candidate, and how many of those
    /// were right: how far a lone suggestion can be trusted.
    pub single: usize,
    pub single_right: usize,
    /// Right first if an equivalent target counts: for reactions, the same
    /// chemistry under another BiGG id (another compartment, a duplicate).
    pub top1_equivalent: usize,
}

/// Why an element was not answered right first.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum MissKind {
    /// No candidate at all.
    Nothing,
    /// Candidates, but the right one is not among them.
    Wrong,
    /// The right one is there, but not first (rank, from 1).
    Lower(usize),
}

/// One element not answered right first, for the report.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Miss {
    pub kind: MissKind,
    pub model: String,
    pub element: String,
    pub name: Option<String>,
    pub truth: String,
    pub first: Option<String>,
}

impl Score {
    /// Grades one element. `same(a, b)` says whether `b` is as good an
    /// answer as `a`, for [`Score::top1_equivalent`].
    fn record(&mut self, key: &Key, cands: &[Candidate], same: &dyn Fn(Target, Target) -> bool) -> Option<MissKind> {
        let accept = match key {
            Key::Unannotated => return None,
            Key::OutsidePack => {
                self.annotated += 1;
                self.outside_pack += 1;
                return None;
            }
            Key::Known { accept, tie } => {
                self.annotated += 1;
                self.gradable += 1;
                if *tie {
                    self.ties += 1;
                }
                accept
            }
        };
        if !cands.is_empty() {
            self.suggested += 1;
        }
        let rank = cands.iter().position(|c| accept.contains(&c.target));
        if let Some(first) = cands.first()
            && accept.iter().any(|a| *a == first.target || same(*a, first.target))
        {
            self.top1_equivalent += 1;
        }
        if let Some(r) = rank {
            self.top1 += usize::from(r == 0);
            self.top3 += usize::from(r < 3);
            self.top5 += usize::from(r < 5);
            self.reciprocal_rank += 1.0 / (r as f64 + 1.0);
        }
        if cands.len() == 1 {
            self.single += 1;
            self.single_right += usize::from(rank == Some(0));
        }
        match rank {
            Some(0) => None,
            Some(r) => Some(MissKind::Lower(r + 1)),
            None if cands.is_empty() => Some(MissKind::Nothing),
            None => Some(MissKind::Wrong),
        }
    }
}

/// Everything the benchmark found.
#[derive(Debug, Default)]
pub struct Results {
    pub models: usize,
    pub unreadable: Vec<(String, String)>,
    pub species: Score,
    pub reactions: Score,
    pub species_misses: Vec<Miss>,
    pub reaction_misses: Vec<Miss>,
    /// The same scores over "novel" models only: those whose species names
    /// overlap no training model much (see [`NOVEL_OVERLAP`]). BioModels has
    /// families of near-identical models; a test model whose sibling is in
    /// the training set flatters the scores.
    pub novel_models: usize,
    pub novel_species: Score,
    pub novel_reactions: Score,
}

/// A test model is novel if the Jaccard overlap of its species names with
/// every training model's is below this.
pub const NOVEL_OVERLAP: f64 = 0.5;

/// The normalized species names (or ids, when unnamed) of a model.
pub fn name_set(model: &annot_engine::Model) -> BTreeSet<String> {
    model
        .species
        .iter()
        .map(|s| kpack::normalize_name(s.name.as_deref().unwrap_or(&s.id)))
        .filter(|n| !n.is_empty())
        .collect()
}

/// The largest Jaccard overlap of `names` with any of `others`.
pub fn best_overlap(names: &BTreeSet<String>, others: &[BTreeSet<String>]) -> f64 {
    others
        .iter()
        .map(|o| {
            let union = names.union(o).count();
            if union == 0 {
                0.0
            } else {
                names.intersection(o).count() as f64 / union as f64
            }
        })
        .fold(0.0, f64::max)
}

fn label(pack: &Pack, t: Target) -> String {
    let (label, bigg) = match t {
        Target::Entity(e) => (
            pack.str(pack.entity(e).label),
            pack.entity_xrefs(e, "bigg.metabolite").next().map(|x| pack.str(x.id)),
        ),
        Target::Reaction(r) => (
            pack.str(pack.reaction(r).label),
            pack.reaction(r)
                .xrefs
                .iter()
                .find(|x| pack.namespace_prefix(x.ns) == "bigg.reaction")
                .map(|x| pack.str(x.id)),
        ),
    };
    match bigg {
        Some(b) => format!("{label} ({b})"),
        None => label.to_string(),
    }
}

/// Grades one model and adds it to `results`.
pub fn grade(pack: &Pack, g: &Graded, novel: bool, results: &mut Results) {
    let s = annot_engine::annotate(pack, &g.model);
    let strict = |_: Target, _: Target| false;
    let same_reaction = |a: Target, b: Target| equivalent_reactions(pack, a, b);
    results.models += 1;
    if novel {
        results.novel_models += 1;
        for (i, key) in g.species.iter().enumerate() {
            results.novel_species.record(key, &s.species[i], &strict);
        }
        for (i, key) in g.reactions.iter().enumerate() {
            results.novel_reactions.record(key, &s.reactions[i], &same_reaction);
        }
    }
    for (i, key) in g.species.iter().enumerate() {
        if let Some(kind) = results.species.record(key, &s.species[i], &strict) {
            let sp = &g.model.species[i];
            results.species_misses.push(miss(pack, kind, g, &sp.id, &sp.name, key, &s.species[i]));
        }
    }
    for (i, key) in g.reactions.iter().enumerate() {
        if let Some(kind) = results.reactions.record(key, &s.reactions[i], &same_reaction) {
            let r = &g.model.reactions[i];
            results.reaction_misses.push(miss(pack, kind, g, &r.id, &r.name, key, &s.reactions[i]));
        }
    }
}

/// Whether two pack reactions are the same chemistry: the same
/// participants and stoichiometry ignoring compartments, or a shared Rhea or
/// MetaNetX id.
pub fn equivalent_reactions(pack: &Pack, a: Target, b: Target) -> bool {
    let (Target::Reaction(a), Target::Reaction(b)) = (a, b) else {
        return false;
    };
    let chemistry = |r: u32| {
        let mut v: Vec<(u32, i64)> = pack
            .reaction(r)
            .participants
            .iter()
            .map(|p| (p.entity, (p.coef * 1000.0).round() as i64))
            .collect();
        v.sort_unstable();
        v
    };
    if chemistry(a) == chemistry(b) {
        return true;
    }
    let ids = |r: u32| -> BTreeSet<(u16, u32)> {
        pack.reaction(r)
            .xrefs
            .iter()
            .filter(|x| matches!(pack.namespace_prefix(x.ns), "rhea" | "metanetx.reaction"))
            .map(|x| (x.ns, x.id))
            .collect()
    };
    !ids(a).is_disjoint(&ids(b))
}

fn miss(
    pack: &Pack,
    kind: MissKind,
    g: &Graded,
    element: &str,
    name: &Option<String>,
    key: &Key,
    cands: &[Candidate],
) -> Miss {
    let truth = match key {
        Key::Known { accept, .. } => accept
            .iter()
            .map(|t| label(pack, *t))
            .collect::<Vec<_>>()
            .join(" / "),
        _ => String::new(),
    };
    Miss {
        kind,
        model: g.id.clone(),
        element: element.to_string(),
        name: name.clone(),
        truth,
        first: cands.first().map(|c| label(pack, c.target)),
    }
}

fn pct(n: usize, d: usize) -> String {
    if d == 0 {
        "-".into()
    } else {
        format!("{:.1}%", 100.0 * n as f64 / d as f64)
    }
}

/// The report as Markdown.
pub fn report(r: &Results, pack: &Pack, set: &str, date: &str, providers: &[&str]) -> String {
    let mut o = String::new();
    let _ = writeln!(o, "# Annotation benchmark, {date}\n");
    let _ = writeln!(o, "- Models: {} ({set} set), {} unreadable", r.models, r.unreadable.len());
    let _ = writeln!(o, "- Engine providers: {}", providers.join(", "));
    let _ = writeln!(o, "- Pack built {}:", pack.header.built);
    for s in &pack.header.sources {
        let _ = writeln!(o, "  - {} {}", s.name, s.version);
    }
    if set != "test" {
        let _ = writeln!(
            o,
            "\n**In-sample:** these models' names are in the pack. Scores overstate accuracy."
        );
    }
    let _ = writeln!(o, "\n| | Species | Reactions |\n|---|---|---|");
    let (s, x) = (&r.species, &r.reactions);
    let row = |o: &mut String, label: &str, a: String, b: String| {
        let _ = writeln!(o, "| {label} | {a} | {b} |");
    };
    row(&mut o, "Annotated", s.annotated.to_string(), x.annotated.to_string());
    row(&mut o, "Annotated with something outside the pack", s.outside_pack.to_string(), x.outside_pack.to_string());
    row(&mut o, "**Graded**", format!("**{}**", s.gradable), format!("**{}**", x.gradable));
    row(&mut o, "Graded with tied annotations", s.ties.to_string(), x.ties.to_string());
    let both = |f: &dyn Fn(&Score) -> String| (f(s), f(x));
    let (a, b) = both(&|k| format!("{} ({})", k.suggested, pct(k.suggested, k.gradable)));
    row(&mut o, "Got a suggestion (coverage)", a, b);
    let (a, b) = both(&|k| format!("**{}**", pct(k.top1, k.gradable)));
    row(&mut o, "**Right first (top-1)**", a, b);
    let (a, b) = both(&|k| pct(k.top3, k.gradable));
    row(&mut o, "Right in top 3", a, b);
    let (a, b) = both(&|k| pct(k.top1_equivalent, k.gradable));
    row(&mut o, "Right first, counting the same chemistry under another id", a, b);
    let (a, b) = both(&|k| pct(k.top5, k.gradable));
    row(&mut o, "Right in top 5", a, b);
    let (a, b) = both(&|k| pct(k.top1, k.suggested));
    row(&mut o, "First suggestion right, when there is one (precision)", a, b);
    let (a, b) = both(&|k| {
        if k.gradable == 0 {
            "-".into()
        } else {
            format!("{:.3}", k.reciprocal_rank / k.gradable as f64)
        }
    });
    row(&mut o, "Mean reciprocal rank", a, b);
    let (a, b) = both(&|k| format!("{} of {} ({})", k.single_right, k.single, pct(k.single_right, k.single)));
    row(&mut o, "Lone suggestion right", a, b);

    if set == "test" {
        let (s, x) = (&r.novel_species, &r.novel_reactions);
        let _ = writeln!(
            o,
            "\n### Novel test models only\n\n{} of the {} test models share less than {:.0}% of their \
             species names with every training model. The rest have a near-twin in the training set, \
             which flatters the scores above; these are the fairer estimate for a model the tool has \
             never seen.\n",
            r.novel_models,
            r.models,
            100.0 * NOVEL_OVERLAP
        );
        let _ = writeln!(o, "| | Species | Reactions |\n|---|---|---|");
        row(&mut o, "Graded", s.gradable.to_string(), x.gradable.to_string());
        row(&mut o, "Got a suggestion (coverage)", pct(s.suggested, s.gradable), pct(x.suggested, x.gradable));
        row(
            &mut o,
            "**Right first (top-1)**",
            format!("**{}**", pct(s.top1, s.gradable)),
            format!("**{}**", pct(x.top1, x.gradable)),
        );
        row(&mut o, "Right in top 3", pct(s.top3, s.gradable), pct(x.top3, x.gradable));
        row(
            &mut o,
            "Right first, counting the same chemistry under another id",
            pct(s.top1_equivalent, s.gradable),
            pct(x.top1_equivalent, x.gradable),
        );
        row(
            &mut o,
            "First suggestion right, when there is one (precision)",
            pct(s.top1, s.suggested),
            pct(x.top1, x.suggested),
        );
    }

    misses_section(&mut o, "Species", &r.species_misses, 40);
    misses_section(&mut o, "Reactions", &r.reaction_misses, 20);

    if !r.unreadable.is_empty() {
        let _ = writeln!(o, "\n## Unreadable models\n");
        for (id, e) in &r.unreadable {
            let _ = writeln!(o, "- {id}: {e}");
        }
    }
    o
}

fn misses_section(o: &mut String, title: &str, misses: &[Miss], per_kind: usize) {
    let count = |f: &dyn Fn(&MissKind) -> bool| misses.iter().filter(|m| f(&m.kind)).count();
    let _ = writeln!(
        o,
        "\n## {title} not right first\n\n{} with no suggestion, {} with only wrong ones, {} right but ranked lower.",
        count(&|k| *k == MissKind::Nothing),
        count(&|k| *k == MissKind::Wrong),
        count(&|k| matches!(k, MissKind::Lower(_)))
    );
    type Pick<'a> = &'a dyn Fn(&MissKind) -> bool;
    let groups: [(&str, Pick); 3] = [
        ("No suggestion", &|k| *k == MissKind::Nothing),
        ("Wrong suggestions only", &|k| *k == MissKind::Wrong),
        ("Right answer ranked lower", &|k| matches!(k, MissKind::Lower(_))),
    ];
    for (heading, pick) in groups {
        let picked: Vec<&Miss> = misses.iter().filter(|m| pick(&m.kind)).collect();
        if picked.is_empty() {
            continue;
        }
        let _ = writeln!(
            o,
            "\n### {heading} (first {} of {})\n\n| Model | Id | Name | Annotated as | First suggestion |\n|---|---|---|---|---|",
            per_kind.min(picked.len()),
            picked.len()
        );
        for m in picked.iter().take(per_kind) {
            let first = match (&m.first, &m.kind) {
                (Some(f), MissKind::Lower(r)) => format!("{f} (right one at {r})"),
                (Some(f), _) => f.clone(),
                (None, _) => "-".into(),
            };
            let _ = writeln!(
                o,
                "| {} | {} | {} | {} | {} |",
                m.model,
                cell(&m.element),
                cell(m.name.as_deref().unwrap_or("")),
                cell(&m.truth),
                cell(&first)
            );
        }
    }
}

/// Text safe in a Markdown table cell.
fn cell(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ")
}

/// One line for bench/history.tsv.
pub fn history_line(r: &Results, pack: &Pack, set: &str, date: &str, providers: &[&str]) -> String {
    let f = |k: &Score| {
        format!(
            "{}\t{}\t{}\t{}\t{}",
            k.gradable,
            pct(k.top1, k.gradable),
            pct(k.top3, k.gradable),
            pct(k.suggested, k.gradable),
            pct(k.top1, k.suggested)
        )
    };
    format!(
        "{date}\t{}\t{set}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
        pack.header.built,
        r.models,
        providers.join("+"),
        f(&r.species),
        f(&r.reactions),
        r.novel_models,
        pct(r.novel_species.top1, r.novel_species.gradable),
        pct(r.novel_reactions.top1, r.novel_reactions.gradable)
    )
}

pub const HISTORY_HEADER: &str = "date\tpack_built\tset\tmodels\tproviders\t\
species_graded\tspecies_top1\tspecies_top3\tspecies_coverage\tspecies_precision\t\
reactions_graded\treactions_top1\treactions_top3\treactions_coverage\treactions_precision\t\
novel_models\tnovel_species_top1\tnovel_reactions_top1";

#[cfg(test)]
mod tests {
    use super::*;
    use annot_engine::Evidence;

    fn cand(t: u32) -> Candidate {
        Candidate {
            target: Target::Entity(t),
            score: 1.0,
            evidence: vec![Evidence {
                provider: "t",
                weight: 1.0,
                note: String::new(),
            }],
        }
    }

    fn known(t: u32) -> Key {
        Key::Known {
            accept: vec![Target::Entity(t)],
            tie: false,
        }
    }

    #[test]
    fn scoring() {
        let mut s = Score::default();
        let no = |_: Target, _: Target| false;
        let two_is_one = |a: Target, b: Target| a == Target::Entity(1) && b == Target::Entity(2);
        assert_eq!(s.record(&Key::Unannotated, &[cand(1)], &no), None);
        assert_eq!(s.record(&Key::OutsidePack, &[], &no), None);
        assert_eq!(s.record(&known(1), &[cand(1)], &no), None);
        assert_eq!(s.record(&known(1), &[cand(2), cand(1)], &no), Some(MissKind::Lower(2)));
        assert_eq!(s.record(&known(1), &[cand(2)], &two_is_one), Some(MissKind::Wrong));
        assert_eq!(s.record(&known(1), &[], &no), Some(MissKind::Nothing));
        assert_eq!(s.annotated, 5);
        assert_eq!(s.outside_pack, 1);
        assert_eq!(s.gradable, 4);
        assert_eq!(s.suggested, 3);
        assert_eq!((s.top1, s.top3, s.top5), (1, 2, 2));
        assert!((s.reciprocal_rank - 1.5).abs() < 1e-9);
        assert_eq!((s.single, s.single_right), (2, 1));
        // The lone wrong answer (2) counts as right when 2 is equivalent to 1.
        assert_eq!(s.top1_equivalent, 2);
    }
}
