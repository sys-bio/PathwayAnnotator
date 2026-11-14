//! ChEBI, searched through EBI's Ontology Lookup Service (OLS4).

use serde::Deserialize;

use crate::text::{percent_encode, strip_html};
use crate::{Db, Hit, names_match};

/// How many entries to ask for.
const ROWS: usize = 10;

/// The OLS search for `text` among ChEBI's names and synonyms.
pub fn chebi_url(text: &str) -> String {
    format!(
        "https://www.ebi.ac.uk/ols4/api/search?q={}&ontology=chebi&rows={ROWS}\
         &fieldList=obo_id,label,exact_synonyms,related_synonyms,description&queryFields=label,synonym",
        percent_encode(text.trim())
    )
}

#[derive(Deserialize)]
struct Answer {
    response: Response,
}

#[derive(Deserialize)]
struct Response {
    #[serde(default)]
    docs: Vec<Doc>,
}

#[derive(Deserialize)]
struct Doc {
    obo_id: Option<String>,
    label: Option<String>,
    #[serde(default)]
    exact_synonyms: Vec<String>,
    #[serde(default)]
    related_synonyms: Vec<String>,
    #[serde(default)]
    description: Vec<String>,
}

/// The ChEBI entries in an OLS answer, in OLS's order of relevance, with
/// exact name matches marked. Entries without a ChEBI id are skipped.
pub fn parse_chebi(json: &str, query: &str) -> Result<Vec<Hit>, String> {
    let answer: Answer = serde_json::from_str(json).map_err(|e| format!("ChEBI search answer: {e}"))?;
    let mut hits = Vec::new();
    for (rank, d) in answer.response.docs.into_iter().enumerate() {
        let Some(id) = d.obo_id.filter(|i| i.starts_with("CHEBI:")) else {
            continue;
        };
        let label = strip_html(d.label.as_deref().unwrap_or(&id));
        let mut synonyms: Vec<String> = Vec::new();
        for s in d.exact_synonyms.iter().chain(&d.related_synonyms).map(|s| strip_html(s)) {
            if s != label && !synonyms.contains(&s) {
                synonyms.push(s);
            }
        }
        let exact = names_match(query, &label, &synonyms);
        hits.push(Hit {
            db: Db::Chebi,
            uri: format!("https://identifiers.org/{id}"),
            id,
            label,
            detail: d.description.first().map(|s| strip_html(s)),
            synonyms,
            exact,
            score: -(rank as f32),
            why: exact.then(|| "Its name or a synonym is the search text".to_string()),
        });
    }
    Ok(hits)
}
