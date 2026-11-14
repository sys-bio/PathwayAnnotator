//! ChEBI and GO, searched through EBI's Ontology Lookup Service (OLS4).

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

/// The OLS search for `text` among GO's cellular components (the terms
/// under GO:0005575, "cellular_component").
pub fn go_url(text: &str) -> String {
    format!(
        "https://www.ebi.ac.uk/ols4/api/search?q={}&ontology=go&rows={ROWS}\
         &fieldList=obo_id,label,exact_synonyms,related_synonyms,description&queryFields=label,synonym\
         &allChildrenOf=http%3A%2F%2Fpurl.obolibrary.org%2Fobo%2FGO_0005575",
        percent_encode(text.trim())
    )
}

/// The OLS search for `text` among GO's biological processes (the terms
/// under GO:0008150): what a model as a whole is about ("glycolytic
/// process"). Read the answer with [`parse_go`].
pub fn go_process_url(text: &str) -> String {
    format!(
        "https://www.ebi.ac.uk/ols4/api/search?q={}&ontology=go&rows={ROWS}         &fieldList=obo_id,label,exact_synonyms,related_synonyms,description&queryFields=label,synonym         &allChildrenOf=http%3A%2F%2Fpurl.obolibrary.org%2Fobo%2FGO_0008150",
        percent_encode(text.trim())
    )
}

/// The ChEBI entries in an OLS answer, in OLS's order of relevance, with
/// exact name matches marked. Entries without a ChEBI id are skipped.
pub fn parse_chebi(json: &str, query: &str) -> Result<Vec<Hit>, String> {
    parse_ols(json, query, Db::Chebi, "CHEBI:")
}

/// The GO terms in an OLS answer ([`go_url`]), as [`parse_chebi`] does.
pub fn parse_go(json: &str, query: &str) -> Result<Vec<Hit>, String> {
    parse_ols(json, query, Db::Go, "GO:")
}

fn parse_ols(json: &str, query: &str, db: Db, prefix: &str) -> Result<Vec<Hit>, String> {
    let answer: Answer =
        serde_json::from_str(json).map_err(|e| format!("{} search answer: {e}", db.label()))?;
    let mut hits = Vec::new();
    for (rank, d) in answer.response.docs.into_iter().enumerate() {
        let Some(id) = d.obo_id.filter(|i| i.starts_with(prefix)) else {
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
            db,
            uri: format!("https://identifiers.org/{id}"),
            id,
            label,
            detail: d.description.first().map(|s| strip_html(s)),
            synonyms,
            exact,
            score: -(rank as f32),
            why: exact.then(|| "Its name or a synonym is the search text".to_string()),
            ec: Vec::new(),
        });
    }
    Ok(hits)
}
