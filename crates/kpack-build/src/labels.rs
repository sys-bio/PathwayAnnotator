//! Labels people read: what the app shows for each molecule and reaction.
//!
//! Source labels are sometimes written for programs rather than people:
//! SBML-escaped ids ("R_glucose_transport__uniport_"), bare BiGG ids
//! ("FUMt3"), charge states ("L-leucine zwitterion", "Exchange of
//! cis-aconitate(3-)."). The builder tidies them once all sources are in
//! ([`crate::PackBuilder::finish`]). Only the label changes: the names the
//! matcher looks up were recorded as the sources gave them.

use kpack::{Entity, EntityFlags, EntityId, Reaction, ReactionFlags};

/// Suffixes naming a charge state, which biologists leave out:
/// "L-leucine zwitterion" is L-leucine.
const CHARGE_STATE_WORDS: &[&str] = &[
    "zwitterion",
    "dizwitterion",
    "anion",
    "dianion",
    "trianion",
    "tetraanion",
    "cation",
    "dication",
    "trication",
];

/// A molecule label without a charge state or charge:
/// "L-leucine zwitterion" -> "L-leucine", "ATP(4-)" -> "ATP".
pub fn tidy_entity_label(label: &str) -> String {
    let mut s = label.trim();
    if let Some((head, last)) = s.rsplit_once(' ')
        && CHARGE_STATE_WORDS.contains(&last.to_ascii_lowercase().as_str())
        && !head.trim().is_empty()
    {
        s = head.trim_end();
    }
    kpack::strip_charge_suffix(s).to_string()
}

/// A reaction label made readable, where it can be from the text alone:
/// SBML escapes undone ("R_glucose_transport__uniport_" -> "glucose
/// transport (uniport)"), a stray leading underscore and final full stop
/// dropped, a trailing charge removed ("Exchange of cis-aconitate(3-)." ->
/// "Exchange of cis-aconitate").
pub fn tidy_reaction_label(label: &str) -> String {
    let s = label.trim();
    let s = s.strip_suffix('.').unwrap_or(s).trim_end();
    let s = match s.strip_prefix("R_") {
        Some(rest) if !rest.is_empty() && !rest.contains(' ') => unescape_sbml_id(rest),
        _ => s.to_string(),
    };
    let s = match s.strip_prefix('_') {
        Some(rest) if rest.starts_with(|c: char| c.is_ascii_alphanumeric()) => rest.to_string(),
        _ => s,
    };
    kpack::strip_charge_suffix(&s).to_string()
}

/// Undoes the way BiGG wrote names as SBML ids: double underscores
/// bracket a part ("transport__uniport_" -> "transport (uniport)"); a
/// single underscore is a hyphen inside a chemical name ("3_Methyl",
/// "L_lysine", "14_alpha") and a space elsewhere.
fn unescape_sbml_id(s: &str) -> String {
    let segments: Vec<&str> = s.split("__").collect();
    let mut out = String::new();
    for (i, seg) in segments.iter().enumerate() {
        let seg = seg.trim_matches('_');
        if seg.is_empty() {
            continue;
        }
        let words = underscores_to_words(seg);
        if !out.is_empty() {
            out.push(' ');
        }
        // "x__a__b_": the final underscore closes a bracket too.
        let last = i + 1 == segments.len();
        if i % 2 == 1 || (i > 0 && last && s.ends_with('_')) {
            out.push('(');
            out.push_str(&words);
            out.push(')');
        } else {
            out.push_str(&words);
        }
    }
    out
}

fn underscores_to_words(s: &str) -> String {
    let parts: Vec<&str> = s.split('_').filter(|p| !p.is_empty()).collect();
    let mut out = String::new();
    for (i, p) in parts.iter().enumerate() {
        if i > 0 {
            let prev = parts[i - 1];
            // A locant or stereo prefix joins the next word with a hyphen.
            let digits = |w: &str| w.chars().all(|c| c.is_ascii_digit());
            // Locants: "2_4_cyclohexadiene" is 2,4-cyclohexadiene.
            if digits(prev) && digits(p) {
                out.push(',');
                out.push_str(p);
                continue;
            }
            let joins = prev.chars().all(|c| c.is_ascii_digit())
                || p.chars().all(|c| c.is_ascii_digit())
                || matches!(prev, "D" | "L" | "N" | "O" | "S" | "R" | "alpha" | "beta" | "cis" | "trans");
            out.push(if joins { '-' } else { ' ' });
        }
        out.push_str(p);
    }
    out
}

/// Words naming where in the cell a reaction happens.
const COMPARTMENT_WORDS: &[&str] = &[
    "cytosol", "cytosolic", "cytoplasm", "cytoplasmic", "mitochondria", "mitochondrial", "mitochondrion",
    "periplasm", "periplasmic", "extracellular", "chloroplast", "plastid", "plastidic", "apicoplast",
    "endoplasmic reticulum", "er", "golgi", "golgi apparatus", "peroxisome", "peroxisomal", "nucleus",
    "nuclear", "lysosome", "lysosomal", "vacuole", "vacuolar", "glyoxysome", "glycosome", "thylakoid",
];

/// A reaction name without a final compartment ("glucose-6-phosphate
/// dehydrogenase, endoplasmic reticulum" -> "glucose-6-phosphate
/// dehydrogenase", "fumarase (mitochondrial)" -> "fumarase"), for using
/// one compartment's name for the same chemistry in another.
pub fn without_compartment(name: &str) -> &str {
    let is_compartment = |w: &str| COMPARTMENT_WORDS.contains(&w.trim().to_ascii_lowercase().as_str());
    let name = name.trim();
    if let Some(body) = name.strip_suffix(')')
        && let Some(open) = body.rfind('(')
        && is_compartment(&body[open + 1..])
        && !body[..open].trim().is_empty()
    {
        return body[..open].trim_end();
    }
    if let Some((head, last)) = name.rsplit_once(',')
        && is_compartment(last)
        && !head.trim().is_empty()
    {
        return head.trim_end();
    }
    name
}

/// Whether a label looks like a database id rather than a name: one word
/// of letters, digits and underscores with digits, underscores or capitals
/// after the first letter ("FUMt3", "MPTAT", "ATPSu_1"; not "hexokinase",
/// not "3-deoxy-manno-octulosonate-8-phosphatase"), or a MetaCyc reaction
/// id ("TRANS-RXNBWI-115637.ce.maizeexp.GLT_GLT").
pub fn looks_like_id(label: &str) -> bool {
    if label.is_empty() || label.contains([' ', ',']) {
        return false;
    }
    let one_word = label.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    let coded = label
        .chars()
        .skip(1)
        .any(|c| c.is_ascii_digit() || c == '_' || c.is_ascii_uppercase());
    (one_word && coded) || label.contains("RXN")
}

/// A label for a reaction whose source gave only an id, from what it does:
/// "Exchange of fumarate", "fumarate transport", or its equation without
/// currency metabolites ("D-glucose 6-phosphate = D-fructose
/// 6-phosphate"). `None` when there is nothing to say (no participants).
pub fn reaction_label_from_participants(r: &Reaction, entities: &[Entity], label: impl Fn(EntityId) -> String) -> Option<String> {
    let mut ids: Vec<EntityId> = Vec::new();
    for p in &r.participants {
        if !ids.contains(&p.entity) {
            ids.push(p.entity);
        }
    }
    if ids.is_empty() {
        return None;
    }
    let currency = |e: EntityId| entities[e as usize].flags.contains(EntityFlags::CURRENCY);
    // The molecules that matter: currency ones only when there is nothing
    // else ("H+ transport").
    let main: Vec<EntityId> = ids.iter().copied().filter(|&e| !currency(e)).collect();
    let main = if main.is_empty() { ids.clone() } else { main };
    let names = |es: &[EntityId]| es.iter().map(|&e| label(e)).collect::<Vec<_>>().join(", ");

    if r.flags.contains(ReactionFlags::EXCHANGE) {
        return Some(format!("Exchange of {}", names(&main)));
    }
    if r.flags.contains(ReactionFlags::TRANSPORT) {
        // What crosses: on both sides of the reaction.
        let crossing: Vec<EntityId> = main
            .iter()
            .copied()
            .filter(|&e| {
                let coefs = r.participants.iter().filter(|p| p.entity == e).map(|p| p.coef);
                let (mut used, mut made) = (false, false);
                for c in coefs {
                    used |= c < 0.0;
                    made |= c > 0.0;
                }
                used && made
            })
            .collect();
        let what = if crossing.is_empty() { &main } else { &crossing };
        return Some(format!("{} transport", names(what)));
    }
    // Each side's molecules: the main ones, or every one when a side has
    // only currency molecules ("Ap4A + H2O = ADP + ADP").
    let side = |sign: f32| {
        let all: Vec<EntityId> = r.participants.iter().filter(|p| p.coef * sign > 0.0).map(|p| p.entity).collect();
        let picked: Vec<EntityId> = all.iter().copied().filter(|e| main.contains(e)).collect();
        if picked.is_empty() { all } else { picked }
    };
    let (left, right) = (side(-1.0), side(1.0));
    let mut l_sorted = left.clone();
    let mut r_sorted = right.clone();
    l_sorted.sort_unstable();
    r_sorted.sort_unstable();
    if !left.is_empty() && l_sorted == r_sorted {
        // The same molecules on both sides: it moves between compartments.
        l_sorted.dedup();
        return Some(format!("{} transport", names(&l_sorted)));
    }
    let join = |es: &[EntityId]| es.iter().map(|&e| label(e)).collect::<Vec<_>>().join(" + ");
    match (left.is_empty(), right.is_empty()) {
        (true, true) => None,
        (false, true) => Some(format!("{} consumed", join(&left))),
        (true, false) => Some(format!("{} produced", join(&right))),
        // Rhea's way of writing a reaction: matching ignores direction.
        (false, false) => Some(format!("{} = {}", join(&left), join(&right))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kpack::Participant;

    #[test]
    fn molecule_labels() {
        assert_eq!(tidy_entity_label("L-leucine zwitterion"), "L-leucine");
        assert_eq!(
            tidy_entity_label("(2S,6S)-2,6-diaminopimelic acid dizwitterion"),
            "(2S,6S)-2,6-diaminopimelic acid"
        );
        assert_eq!(tidy_entity_label("ATP(4-)"), "ATP");
        assert_eq!(tidy_entity_label("trimethylammonium"), "trimethylammonium");
        // A charge-state word alone is left alone.
        assert_eq!(tidy_entity_label("anion"), "anion");
    }

    #[test]
    fn reaction_labels() {
        assert_eq!(tidy_reaction_label("R_glucose_transport__uniport_"), "glucose transport (uniport)");
        assert_eq!(
            tidy_reaction_label("R_formate_tetrahydrofolate_ligase__mitochondrial_irreversible"),
            "formate tetrahydrofolate ligase (mitochondrial irreversible)"
        );
        assert_eq!(
            tidy_reaction_label("R_3_Methyl_2_oxopentanoate_transport__diffusion__mitochondrial"),
            "3-Methyl-2-oxopentanoate transport (diffusion) mitochondrial"
        );
        assert_eq!(
            tidy_reaction_label("R_L_lysine_transport_in_via_proton_antiport__vacuolar_"),
            "L-lysine transport in via proton antiport (vacuolar)"
        );
        assert_eq!(tidy_reaction_label("R_fumarylacetoacetase"), "fumarylacetoacetase");
        assert_eq!(
            tidy_reaction_label("R_Cretinine_transport_via_proton_symport__reversible__periplasm_"),
            "Cretinine transport via proton symport (reversible) (periplasm)"
        );
        assert_eq!(
            tidy_reaction_label("R_L_leucylleucine__leuleu__transport_via_ABC_system__periplasm_"),
            "L-leucylleucine (leuleu) transport via ABC system (periplasm)"
        );
        assert_eq!(
            tidy_reaction_label("R_2_succinyl_6_hydroxy_2_4_cyclohexadiene_1_carboxylate_synthase"),
            "2-succinyl-6-hydroxy-2,4-cyclohexadiene-1-carboxylate synthase"
        );
        assert_eq!(
            tidy_reaction_label("_3-hydroxyacyl-CoA dehydrogenase (acetoacetyl-CoA) (mitochondria)"),
            "3-hydroxyacyl-CoA dehydrogenase (acetoacetyl-CoA) (mitochondria)"
        );
        assert_eq!(tidy_reaction_label("Exchange of cis-aconitate(3-)."), "Exchange of cis-aconitate");
        assert_eq!(tidy_reaction_label("hexokinase (D-glucose:ATP)"), "hexokinase (D-glucose:ATP)");
    }

    #[test]
    fn compartments_come_off_borrowed_names() {
        assert_eq!(
            without_compartment("glucose-6-phosphate dehydrogenase, endoplasmic reticulum"),
            "glucose-6-phosphate dehydrogenase"
        );
        assert_eq!(without_compartment("fumarase (mitochondrial)"), "fumarase");
        assert_eq!(without_compartment("hexokinase (D-glucose:ATP)"), "hexokinase (D-glucose:ATP)");
        assert_eq!(without_compartment("(mitochondrial)"), "(mitochondrial)");
    }

    #[test]
    fn ids() {
        for id in ["FUMt3", "MPTAT", "ATPSu_1", "NADPHts", "TRANS-RXNBWI-115637.ce.maizeexp.GLT_GLT"] {
            assert!(looks_like_id(id), "{id}");
        }
        for name in [
            "hexokinase",
            "Hexokinase",
            "glucose transport",
            "PE(12:0/0:0) synthase",
            "3-deoxy-manno-octulosonate-8-phosphatase",
            "dephospho-CoAkinase,apicoplast",
        ] {
            assert!(!looks_like_id(name), "{name}");
        }
    }

    #[test]
    fn labels_from_what_a_reaction_does() {
        let entity = |currency: bool| Entity {
            flags: if currency { EntityFlags::CURRENCY } else { EntityFlags::default() },
            ..Entity::default()
        };
        // 0 g6p, 1 f6p, 2 h, 3 atp.
        let entities = vec![entity(false), entity(false), entity(true), entity(true)];
        let names = ["D-glucose 6-phosphate", "D-fructose 6-phosphate", "H+", "ATP"];
        let label = |e: EntityId| names[e as usize].to_string();
        let p = |entity, coef| Participant {
            entity,
            coef,
            compartment: None,
        };
        let mut r = Reaction {
            participants: vec![p(0, -1.0), p(3, -1.0), p(1, 1.0), p(2, 1.0)],
            ..Reaction::default()
        };
        assert_eq!(
            reaction_label_from_participants(&r, &entities, label).unwrap(),
            "D-glucose 6-phosphate = D-fructose 6-phosphate"
        );
        // Only currency molecules on one side: they are shown there.
        r.participants = vec![p(0, -1.0), p(2, 1.0), p(3, 1.0)];
        assert_eq!(
            reaction_label_from_participants(&r, &entities, label).unwrap(),
            "D-glucose 6-phosphate = H+ + ATP"
        );
        // The same molecule on both sides, not flagged as transport.
        r.participants = vec![p(0, -1.0), p(0, 1.0)];
        assert_eq!(reaction_label_from_participants(&r, &entities, label).unwrap(), "D-glucose 6-phosphate transport");
        r.flags = ReactionFlags::TRANSPORT;
        r.participants = vec![p(0, -1.0), p(2, -1.0), p(0, 1.0), p(2, 1.0)];
        assert_eq!(reaction_label_from_participants(&r, &entities, label).unwrap(), "D-glucose 6-phosphate transport");
        r.participants = vec![p(2, -1.0), p(2, 1.0)];
        assert_eq!(reaction_label_from_participants(&r, &entities, label).unwrap(), "H+ transport");
        r.flags = ReactionFlags::EXCHANGE;
        r.participants = vec![p(1, -1.0)];
        assert_eq!(reaction_label_from_participants(&r, &entities, label).unwrap(), "Exchange of D-fructose 6-phosphate");
        r.participants.clear();
        assert_eq!(reaction_label_from_participants(&r, &entities, label), None);
    }
}
