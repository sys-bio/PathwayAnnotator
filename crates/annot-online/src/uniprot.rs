//! UniProt: reviewed (Swiss-Prot) proteins, for species that are enzymes,
//! transporters or other proteins.

use crate::text::percent_encode;
use crate::{Db, Hit, names_match};

/// How many entries to ask for.
const SIZE: usize = 10;

/// The UniProt search for reviewed proteins named `text` (protein or gene
/// name), optionally in one organism (an NCBI taxon id: 9606 is human).
pub fn uniprot_url(text: &str, taxon: Option<u32>) -> String {
    let t = text.trim().replace('"', "");
    let mut q = format!("(protein_name:\"{t}\" OR gene:\"{t}\") AND reviewed:true");
    if let Some(taxon) = taxon {
        q.push_str(&format!(" AND organism_id:{taxon}"));
    }
    format!(
        "https://rest.uniprot.org/uniprotkb/search?query={}&fields=accession,protein_name,organism_name,gene_primary,ec&size={SIZE}&format=tsv",
        percent_encode(&q)
    )
}

/// "ATP-dependent 6-phosphofructokinase, muscle type (ATP-PFK) (PFK-M) (EC
/// 2.7.1.11)" -> the name and its alternatives in brackets ("EC" ones
/// left out).
fn split_names(s: &str) -> (String, Vec<String>) {
    let mut depth = 0;
    let mut first_end = s.len();
    let mut others = Vec::new();
    let mut start = 0;
    for (i, c) in s.char_indices() {
        match c {
            '(' => {
                if depth == 0 {
                    if first_end == s.len() {
                        first_end = i;
                    }
                    start = i + 1;
                }
                depth += 1;
            }
            ')' if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    let alt = s[start..i].trim();
                    if !alt.is_empty() && !alt.starts_with("EC ") {
                        others.push(alt.to_string());
                    }
                }
            }
            _ => {}
        }
    }
    (s[..first_end].trim().to_string(), others)
}

/// The proteins in a UniProt TSV answer (columns as [`uniprot_url`] asks),
/// in UniProt's order, exact name or gene matches marked.
pub fn parse_uniprot(tsv: &str, query: &str) -> Result<Vec<Hit>, String> {
    let mut lines = tsv.lines();
    let header = lines.next().unwrap_or_default();
    if !header.starts_with("Entry") {
        return Err("UniProt search answer: not the expected table".into());
    }
    let mut hits = Vec::new();
    for (rank, line) in lines.filter(|l| !l.trim().is_empty()).enumerate() {
        let cols: Vec<&str> = line.split('\t').collect();
        let get = |i: usize| cols.get(i).map(|s| s.trim()).filter(|s| !s.is_empty());
        let Some(acc) = get(0) else { continue };
        let (label, mut synonyms) = split_names(get(1).unwrap_or(acc));
        let gene = get(3);
        if let Some(g) = gene {
            synonyms.insert(0, g.to_string());
        }
        let mut detail: Vec<String> = Vec::new();
        if let Some(o) = get(2) {
            detail.push(o.to_string());
        }
        if let Some(g) = gene {
            detail.push(format!("gene {g}"));
        }
        if let Some(ec) = get(4) {
            detail.push(format!("EC {ec}"));
        }
        let exact = names_match(query, &label, &synonyms);
        hits.push(Hit {
            db: Db::Uniprot,
            id: acc.to_string(),
            uri: format!("https://identifiers.org/uniprot:{acc}"),
            label,
            detail: (!detail.is_empty()).then(|| detail.join("; ")),
            synonyms,
            exact,
            score: -(rank as f32),
            why: exact.then(|| "Its name or gene is the search text".to_string()),
        });
    }
    Ok(hits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        let (n, alt) = split_names("ATP-dependent 6-phosphofructokinase, muscle type (ATP-PFK) (PFK-M) (EC 2.7.1.11)");
        assert_eq!(n, "ATP-dependent 6-phosphofructokinase, muscle type");
        assert_eq!(alt, ["ATP-PFK", "PFK-M"]);
        let (n, alt) = split_names("Hexokinase-1");
        assert_eq!((n.as_str(), alt.len()), ("Hexokinase-1", 0));
    }

    #[test]
    fn url() {
        let u = uniprot_url("PFK1", Some(9606));
        assert!(u.contains("organism_id%3A9606"), "{u}");
        assert!(u.contains("reviewed%3Atrue"));
    }
}
