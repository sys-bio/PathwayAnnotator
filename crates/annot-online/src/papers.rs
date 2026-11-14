//! Papers: what a model's references are, from Europe PMC.

use serde::Deserialize;

use crate::text::{percent_encode, strip_html};

/// A paper, as a reference list shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct Paper {
    pub title: String,
    /// "King ZA, Lu J, ...", as Europe PMC writes it.
    pub authors: String,
    pub journal: Option<String>,
    pub year: Option<String>,
}

impl Paper {
    /// "King ZA, Lu J, ... (2016) Nucleic Acids Res."
    pub fn citation(&self) -> String {
        let mut s = self.authors.trim_end_matches('.').to_string();
        if let Some(y) = &self.year {
            s.push_str(&format!(" ({y})"));
        }
        if let Some(j) = &self.journal {
            s.push_str(&format!(" {j}."));
        }
        s
    }
}

/// The Europe PMC search for the paper a reference URI names: a PubMed
/// id, a DOI or a PubMed Central id, in identifiers.org form
/// ("https://identifiers.org/pubmed:26476456") or the older
/// "urn:miriam:pubmed:26476456". `None` for anything else.
pub fn paper_url(uri: &str) -> Option<String> {
    let (_, rest) = uri.rsplit_once("identifiers.org/").or_else(|| uri.split_once("urn:miriam:"))?;
    let (prefix, id) = rest.split_once([':', '/'])?;
    let id = id.replace("%3A", ":").replace("%2F", "/");
    let query = match prefix.to_ascii_lowercase().as_str() {
        "pubmed" if id.chars().all(|c| c.is_ascii_digit()) => format!("EXT_ID:{id} AND SRC:MED"),
        "doi" => format!("DOI:\"{}\"", id.replace('"', "")),
        "pmc" => format!("PMCID:{}", id.to_ascii_uppercase()),
        _ => return None,
    };
    Some(format!(
        "https://www.ebi.ac.uk/europepmc/webservices/rest/search?query={}&format=json&resultType=lite&pageSize=1",
        percent_encode(&query)
    ))
}

#[derive(Deserialize)]
struct Answer {
    #[serde(rename = "resultList")]
    result_list: Option<ResultList>,
}

#[derive(Deserialize)]
struct ResultList {
    #[serde(default)]
    result: Vec<Record>,
}

#[derive(Deserialize)]
struct Record {
    title: Option<String>,
    #[serde(rename = "authorString")]
    author_string: Option<String>,
    #[serde(rename = "journalTitle")]
    journal_title: Option<String>,
    #[serde(rename = "pubYear")]
    pub_year: Option<String>,
}

/// The paper in a Europe PMC answer ([`paper_url`]); `Ok(None)` when it
/// found nothing.
pub fn parse_paper(json: &str) -> Result<Option<Paper>, String> {
    let answer: Answer = serde_json::from_str(json).map_err(|e| format!("Europe PMC answer: {e}"))?;
    Ok(answer.result_list.and_then(|l| l.result.into_iter().next()).and_then(|r| {
        Some(Paper {
            title: strip_html(r.title.as_deref()?).trim().to_string(),
            authors: r.author_string.unwrap_or_default(),
            journal: r.journal_title,
            year: r.pub_year,
        })
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls() {
        let u = paper_url("https://identifiers.org/pubmed:26476456").unwrap();
        assert!(u.contains("EXT_ID%3A26476456%20AND%20SRC%3AMED"), "{u}");
        assert!(paper_url("urn:miriam:pubmed:26476456").unwrap().contains("EXT_ID%3A26476456"));
        assert!(paper_url("http://identifiers.org/pubmed/26476456").unwrap().contains("EXT_ID%3A26476456"));
        assert!(paper_url("https://identifiers.org/doi:10.1093/nar/gkv1049").unwrap().contains("DOI%3A%2210.1093%2Fnar%2Fgkv1049%22"));
        assert!(paper_url("https://identifiers.org/pmc:PMC4702785").unwrap().contains("PMCID%3APMC4702785"));
        assert_eq!(paper_url("https://identifiers.org/CHEBI:15422"), None);
        assert_eq!(paper_url("https://example.org/x"), None);
    }

    #[test]
    fn nothing_found() {
        assert_eq!(parse_paper(r#"{"hitCount":0,"resultList":{"result":[]}}"#).unwrap(), None);
        assert!(parse_paper("<html>").is_err());
    }
}
