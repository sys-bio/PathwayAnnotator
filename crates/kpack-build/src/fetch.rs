//! Downloads source files into the local cache.
//!
//! Each source gets its own folder under the cache, with a `fetched.txt`
//! recording the download date (used as the source's version in the pack
//! header). Files are written to a temporary name first, so a failed
//! download never leaves a half-written file behind.

use std::path::Path;

use crate::date;
use crate::sources::{bigg, chebi};

/// The largest file we will download (BiGG's are about 15 MB).
const MAX_BYTES: u64 = 1 << 30;

/// Downloads `url` to `path`.
pub fn download(url: &str, path: &Path) -> Result<u64, Box<dyn std::error::Error>> {
    let mut response = ureq::get(url).call()?;
    let bytes = response
        .body_mut()
        .with_config()
        .limit(MAX_BYTES)
        .read_to_vec()?;
    let tmp = path.with_extension("part");
    std::fs::write(&tmp, &bytes)?;
    std::fs::rename(&tmp, path)?;
    Ok(bytes.len() as u64)
}

/// Downloads BiGG's bulk files into `<cache>/bigg`.
pub fn fetch_bigg(cache: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let dir = cache.join("bigg");
    std::fs::create_dir_all(&dir)?;
    for (file, url) in [
        (bigg::METABOLITES_FILE, bigg::METABOLITES_URL),
        (bigg::REACTIONS_FILE, bigg::REACTIONS_URL),
    ] {
        eprintln!("fetching {url}");
        let n = download(url, &dir.join(file))?;
        eprintln!("  {file}: {n} bytes");
    }
    std::fs::write(dir.join("fetched.txt"), date::today())?;
    Ok(())
}

/// Downloads ChEBI's flat files into `<cache>/chebi`.
pub fn fetch_chebi(cache: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let dir = cache.join("chebi");
    std::fs::create_dir_all(&dir)?;
    for file in chebi::FILES {
        let url = format!("{}{file}", chebi::BASE_URL);
        eprintln!("fetching {url}");
        let n = download(&url, &dir.join(file))?;
        eprintln!("  {file}: {n} bytes");
    }
    std::fs::write(dir.join("fetched.txt"), date::today())?;
    Ok(())
}

/// The date a source was fetched, or "unknown".
pub fn fetched_date(dir: &Path) -> String {
    std::fs::read_to_string(dir.join("fetched.txt"))
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "unknown".into())
}
