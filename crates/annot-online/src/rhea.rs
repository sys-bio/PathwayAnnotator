//! Rhea: reactions, found by their molecules.

use crate::text::percent_encode;
use crate::{Db, Hit};

/// How many reactions to ask for.
const LIMIT: usize = 50;

/// One molecule of the model's reaction, as far as it is known.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Participant {
    /// What the model calls it.
    pub name: String,
    /// What it is identified as ("keto-D-fructose 6-phosphate"), when it
    /// is: compared with Rhea's names when its ids are not Rhea's.
    pub label: Option<String>,
    /// Its ChEBI ids ("CHEBI:4167"), every charge form, when it is
    /// identified; empty when not.
    pub chebi: Vec<String>,
    /// Consumed (reactant) or produced (product).
    pub product: bool,
    /// Water, protons, ATP and the like: not required in a match.
    pub currency: bool,
}

/// What a Rhea search looks for.
#[derive(Clone, Debug, PartialEq)]
pub enum RheaQuery {
    /// Reactions with all these identified molecules (any of each one's
    /// ids): the strong search.
    ByIds(Vec<Vec<String>>),
    /// Reactions naming all these molecules, when none is identified.
    ByNames(Vec<String>),
}

impl RheaQuery {
    /// The query for a model reaction: its identified main molecules if it
    /// has any, else their names. `None` when it has no molecules to go by.
    pub fn for_reaction(participants: &[Participant]) -> Option<RheaQuery> {
        let main: Vec<&Participant> = participants.iter().filter(|p| !p.currency).collect();
        let main = if main.is_empty() { participants.iter().collect() } else { main };
        let ids: Vec<Vec<String>> = main.iter().filter(|p| !p.chebi.is_empty()).map(|p| p.chebi.clone()).collect();
        if !ids.is_empty() {
            return Some(RheaQuery::ByIds(ids));
        }
        let names: Vec<String> = main.iter().map(|p| p.name.trim().to_string()).filter(|n| !n.is_empty()).collect();
        (!names.is_empty()).then_some(RheaQuery::ByNames(names))
    }

    /// Looser searches for when this one finds nothing: every way of
    /// leaving out one molecule (one may be identified with a ChEBI entry
    /// Rhea does not use, such as an uncharged form). Empty when there is
    /// nothing to leave out.
    pub fn relaxed(&self) -> Vec<RheaQuery> {
        let leave_one_out = |n: usize| -> Vec<Vec<usize>> {
            if n < 2 {
                return Vec::new();
            }
            (0..n).map(|skip| (0..n).filter(|&i| i != skip).collect()).collect()
        };
        match self {
            RheaQuery::ByIds(g) => leave_one_out(g.len())
                .into_iter()
                .map(|keep| RheaQuery::ByIds(keep.into_iter().map(|i| g[i].clone()).collect()))
                .collect(),
            RheaQuery::ByNames(n) => leave_one_out(n.len())
                .into_iter()
                .map(|keep| RheaQuery::ByNames(keep.into_iter().map(|i| n[i].clone()).collect()))
                .collect(),
        }
    }

    /// The query in Rhea's syntax: "(chebi:4167 OR chebi:17634) AND ...".
    pub fn text(&self) -> String {
        match self {
            RheaQuery::ByIds(groups) => groups
                .iter()
                .map(|g| {
                    let ids: Vec<String> = g.iter().map(|i| i.to_ascii_lowercase()).collect();
                    if ids.len() == 1 { ids[0].clone() } else { format!("({})", ids.join(" OR ")) }
                })
                .collect::<Vec<_>>()
                .join(" AND "),
            RheaQuery::ByNames(names) => names
                .iter()
                .map(|n| format!("\"{}\"", n.replace('"', "")))
                .collect::<Vec<_>>()
                .join(" AND "),
        }
    }
}

/// The Rhea search URL for a query.
pub fn rhea_url(q: &RheaQuery) -> String {
    format!(
        "https://www.rhea-db.org/rhea/?query={}&columns=rhea-id,equation,chebi-id,ec&format=tsv&limit={LIMIT}",
        percent_encode(&q.text())
    )
}

/// A Rhea reaction as read from the answer, before ranking.
struct Row {
    id: String,
    equation: String,
    chebi: Vec<String>,
    ec: Vec<String>,
}

fn read_rows(tsv: &str) -> Result<Vec<Row>, String> {
    let mut lines = tsv.lines();
    let header = lines.next().unwrap_or_default();
    if !header.starts_with("Reaction identifier") {
        return Err("Rhea search answer: not the expected table".into());
    }
    let list = |s: Option<&str>| -> Vec<String> {
        s.map(|s| s.split(';').map(str::trim).filter(|x| !x.is_empty()).map(String::from).collect())
            .unwrap_or_default()
    };
    Ok(lines
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| {
            let cols: Vec<&str> = l.split('\t').collect();
            let id = cols.first()?.trim().to_string();
            Some(Row {
                id,
                equation: cols.get(1).map(|s| s.trim().to_string()).unwrap_or_default(),
                chebi: list(cols.get(2).copied()),
                ec: list(cols.get(3).copied()).into_iter().map(|e| e.trim_start_matches("EC:").to_string()).collect(),
            })
        })
        .collect())
}

/// The reactions in a Rhea answer, unranked (Rhea's order).
pub fn parse_rhea(tsv: &str) -> Result<Vec<Hit>, String> {
    Ok(read_rows(tsv)?.into_iter().enumerate().map(|(rank, r)| hit(r, -(rank as f32), None)).collect())
}

fn hit(r: Row, score: f32, why: Option<String>) -> Hit {
    let number = r.id.trim_start_matches("RHEA:").to_string();
    Hit {
        db: Db::Rhea,
        uri: format!("https://identifiers.org/rhea:{number}"),
        id: r.id,
        label: r.equation,
        detail: (!r.ec.is_empty()).then(|| format!("EC {}", r.ec.join(", "))),
        synonyms: Vec::new(),
        exact: false,
        score,
        why,
    }
}

/// How alike two molecule names must be ([`word_likeness`]) to count as
/// probably the same molecule in another form.
const ALIKE: f32 = 0.6;

/// The words of a molecule name that say what it is: "keto-D-fructose
/// 6-phosphate" -> fructose, 6, phosphate. Stereo and form prefixes (D,
/// alpha, keto, ...), charges and coefficients are left out.
fn name_words(s: &str) -> Vec<String> {
    const FORM: &[&str] = &["d", "l", "dl", "alpha", "beta", "keto", "aldehydo", "cis", "trans", "r", "s", "e", "z", "n"];
    let mut w: Vec<String> = s
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty() && !FORM.contains(w))
        .map(String::from)
        .collect();
    w.sort();
    w.dedup();
    w
}

/// How alike two molecule names are, 0 to 1: the share of their
/// meaningful words they have in common.
fn word_likeness(a: &str, b: &str) -> f32 {
    let (a, b) = (name_words(a), name_words(b));
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let common = a.iter().filter(|w| b.contains(w)).count();
    common as f32 / (a.len() + b.len() - common) as f32
}

/// One side of an equation, term by term, names normalized: "2 H(+)" ->
/// "h(+)", "D-glucose(out)" -> "d-glucose".
fn terms(side: &str) -> Vec<String> {
    side.split(" + ")
        .map(|t| {
            let t = t.trim();
            let t = match t.split_once(' ') {
                Some((n, rest)) if n.chars().all(|c| c.is_ascii_digit() || c == 'n') => rest,
                _ => t,
            };
            let t = t.strip_suffix("(in)").or_else(|| t.strip_suffix("(out)")).unwrap_or(t);
            kpack::normalize_name(t)
        })
        .collect()
}

/// Ranks a Rhea answer by how well each reaction fits the model's: the
/// share of the model's main molecules it has, whether they are on the
/// sides the model has them (either direction), and how many main
/// molecules it has that the model's reaction does not account for.
/// `currency` says whether a ChEBI id is a currency molecule (water,
/// protons, ...). Best first.
pub fn rank_rhea(tsv: &str, participants: &[Participant], currency: impl Fn(&str) -> bool) -> Result<Vec<Hit>, String> {
    let main: Vec<&Participant> = participants.iter().filter(|p| !p.currency).collect();
    let main = if main.is_empty() { participants.iter().collect() } else { main };
    let unidentified = main.iter().filter(|p| p.chebi.is_empty()).count();
    let mut hits = Vec::new();
    for (rank, row) in read_rows(tsv)?.into_iter().enumerate() {
        let (left, right) = row.equation.split_once(" = ").unwrap_or((row.equation.as_str(), ""));
        let raw: Vec<(String, bool)> = left
            .split(" + ")
            .map(|t| (t.trim().to_string(), false))
            .chain(right.split(" + ").filter(|t| !t.trim().is_empty()).map(|t| (t.trim().to_string(), true)))
            .collect();
        let (left, right) = (terms(left), terms(right));
        // The ids follow the equation's terms, unless a molecule appears
        // twice ("[phosphate](n) ... [phosphate](n-1)"): then sides are
        // only known for names.
        let aligned = row.chebi.len() == left.len() + right.len();
        let side_of_id = |id: &str| -> Option<bool> {
            let i = row.chebi.iter().position(|c| c.eq_ignore_ascii_case(id))?;
            aligned.then_some(i >= left.len())
        };
        let side_of_name = |name: &str| -> Option<bool> {
            let n = kpack::normalize_name(name);
            if n.is_empty() {
                return None;
            }
            if left.contains(&n) {
                Some(false)
            } else if right.contains(&n) {
                Some(true)
            } else {
                None
            }
        };
        // For each main molecule: found? on which side (if known)?
        let mut found = 0usize;
        let mut sides: Vec<(bool, bool)> = Vec::new(); // (model product, Rhea right side)
        // Identified molecules whose ids Rhea does not use, but whose name
        // is nearly one of Rhea's ("keto-D-fructose 6-phosphate", "beta-D-
        // fructose 6-phosphate"): (model's name for it, Rhea's).
        let mut alike: Vec<(String, String)> = Vec::new();
        for p in &main {
            let by_id = p.chebi.iter().find(|id| row.chebi.iter().any(|c| c.eq_ignore_ascii_case(id)));
            let (hit, side) = match by_id {
                Some(id) => (true, side_of_id(id)),
                None if p.chebi.is_empty() => match side_of_name(&p.name) {
                    Some(s) => (true, Some(s)),
                    None => (false, None),
                },
                None => (false, None),
            };
            if hit {
                found += 1;
                if let Some(s) = side {
                    sides.push((p.product, s));
                }
            } else if let Some(label) = &p.label {
                let best = raw
                    .iter()
                    .map(|(t, s)| (word_likeness(label, t), t, *s))
                    .max_by(|a, b| a.0.total_cmp(&b.0));
                if let Some((_, term, s)) = best.filter(|b| b.0 >= ALIKE) {
                    sides.push((p.product, s));
                    alike.push((label.clone(), term.clone()));
                }
            }
        }
        let same_sides = !sides.is_empty()
            && (sides.iter().all(|(m, r)| m == r) || sides.iter().all(|(m, r)| m != r))
            && sides.iter().any(|(m, _)| *m)
            && sides.iter().any(|(m, _)| !*m);
        let extra = row
            .chebi
            .iter()
            .filter(|c| !currency(c))
            .filter(|c| !main.iter().any(|p| p.chebi.iter().any(|id| id.eq_ignore_ascii_case(c))))
            .count()
            .saturating_sub(unidentified + alike.len());
        let total = main.len().max(1);
        let score = (found as f32 + 0.6 * alike.len() as f32) / total as f32 + if same_sides { 0.5 } else { 0.0 }
            - 0.25 * extra as f32
            - rank as f32 * 0.001;
        let mut why = format!("Has {found} of the {} molecules", main.len());
        for (model, rhea) in &alike {
            why.push_str(&format!(", and {rhea} (like {model})"));
        }
        if same_sides {
            why.push_str(", on the same sides");
        }
        if extra > 0 {
            why.push_str(&format!("; {extra} other molecule{} not in the model's reaction", if extra == 1 { "" } else { "s" }));
        }
        let complete = found == main.len() && extra == 0;
        let mut h = hit(row, score, Some(why));
        h.exact = complete && (same_sides || main.len() < 2);
        hits.push(h);
    }
    hits.sort_by(|a, b| b.score.total_cmp(&a.score));
    Ok(hits)
}

/// Several ranked answers as one list: each reaction once (its best
/// score), best first.
pub fn merge_ranked(lists: Vec<Vec<Hit>>) -> Vec<Hit> {
    let mut out: Vec<Hit> = Vec::new();
    for h in lists.into_iter().flatten() {
        match out.iter_mut().find(|o| o.id == h.id) {
            Some(o) if o.score < h.score => *o = h,
            Some(_) => {}
            None => out.push(h),
        }
    }
    out.sort_by(|a, b| b.score.total_cmp(&a.score));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_text() {
        let p = |name: &str, ids: &[&str], product, currency| Participant {
            name: name.into(),
            label: None,
            chebi: ids.iter().map(|s| s.to_string()).collect(),
            product,
            currency,
        };
        let parts = [
            p("GLC", &["CHEBI:4167", "CHEBI:17634"], false, false),
            p("ATP", &["CHEBI:30616"], false, true),
            p("G6P", &[], true, false),
        ];
        let q = RheaQuery::for_reaction(&parts).unwrap();
        assert_eq!(q.text(), "(chebi:4167 OR chebi:17634)");
        let parts = [p("glucose", &[], false, false), p("G6P", &[], true, false)];
        assert_eq!(RheaQuery::for_reaction(&parts).unwrap().text(), "\"glucose\" AND \"G6P\"");
        assert_eq!(RheaQuery::for_reaction(&[]), None);
        let two = RheaQuery::ByIds(vec![vec!["CHEBI:1".into()], vec!["CHEBI:2".into(), "CHEBI:3".into()]]);
        let texts: Vec<String> = two.relaxed().iter().map(|q| q.text()).collect();
        assert_eq!(texts, ["(chebi:2 OR chebi:3)", "chebi:1"]);
        assert!(RheaQuery::ByIds(vec![vec!["CHEBI:1".into()]]).relaxed().is_empty());
    }

    #[test]
    fn likeness() {
        assert_eq!(word_likeness("keto-D-fructose 6-phosphate", "beta-D-fructose 6-phosphate"), 1.0);
        assert!(word_likeness("keto-D-fructose 6-phosphate", "alpha-D-glucose 1-phosphate") < ALIKE);
        assert_eq!(word_likeness("", "x"), 0.0);
    }

    #[test]
    fn equation_terms() {
        assert_eq!(terms("2 H(+) + D-glucose(out)"), [kpack::normalize_name("H(+)"), kpack::normalize_name("D-glucose")]);
    }
}
