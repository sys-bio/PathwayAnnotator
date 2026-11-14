//! The pack as bytes: a magic number, the format version, then the pack in
//! postcard encoding, compressed as one zstd frame.

use std::fmt;
use std::io::Read;

use crate::data::Pack;

/// Bumped whenever the layout of [`Pack`] changes. A pack of another
/// version is refused rather than misread.
pub const FORMAT_VERSION: u32 = 4;

const MAGIC: &[u8; 6] = b"KPACK\0";

/// Why bytes could not be read as a pack.
#[derive(Debug)]
pub enum PackError {
    /// Not a knowledge pack at all.
    NotAPack,
    /// A pack, but built for another version of this crate.
    WrongVersion { found: u32, expected: u32 },
    /// The compressed data is damaged.
    Decompress(String),
    /// The data decompressed but does not decode as a pack.
    Decode(String),
}

impl fmt::Display for PackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PackError::NotAPack => write!(f, "not a knowledge pack"),
            PackError::WrongVersion { found, expected } => write!(
                f,
                "knowledge pack format {found}, but this program reads format {expected}"
            ),
            PackError::Decompress(e) => write!(f, "knowledge pack is damaged: {e}"),
            PackError::Decode(e) => write!(f, "knowledge pack does not decode: {e}"),
        }
    }
}

impl std::error::Error for PackError {}

/// The pack as bytes, ready to save or serve.
pub fn encode(pack: &Pack) -> Vec<u8> {
    let raw = postcard::to_allocvec(pack).expect("a pack always serializes");
    let compressed =
        ruzstd::encoding::compress_to_vec(raw.as_slice(), ruzstd::encoding::CompressionLevel::Fastest);
    let mut out = Vec::with_capacity(MAGIC.len() + 4 + compressed.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    out.extend_from_slice(&compressed);
    out
}

/// A pack from bytes made by [`encode`].
pub fn decode(bytes: &[u8]) -> Result<Pack, PackError> {
    let rest = bytes.strip_prefix(MAGIC.as_slice()).ok_or(PackError::NotAPack)?;
    if rest.len() < 4 {
        return Err(PackError::NotAPack);
    }
    let (version, compressed) = rest.split_at(4);
    let found = u32::from_le_bytes(version.try_into().expect("four bytes"));
    if found != FORMAT_VERSION {
        return Err(PackError::WrongVersion {
            found,
            expected: FORMAT_VERSION,
        });
    }
    let mut raw = Vec::new();
    ruzstd::decoding::StreamingDecoder::new(compressed)
        .map_err(|e| PackError::Decompress(e.to_string()))?
        .read_to_end(&mut raw)
        .map_err(|e| PackError::Decompress(e.to_string()))?;
    postcard::from_bytes(&raw).map_err(|e| PackError::Decode(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Header;

    #[test]
    fn round_trip() {
        let pack = Pack {
            header: Header {
                built: "2026-11-13".into(),
                sources: Vec::new(),
            },
            ..Pack::default()
        };
        let bytes = encode(&pack);
        assert_eq!(decode(&bytes).unwrap(), pack);
    }

    #[test]
    fn refuses_what_is_not_a_pack() {
        assert!(matches!(decode(b"hello"), Err(PackError::NotAPack)));
        let mut bytes = encode(&Pack::default());
        bytes[6] = 99;
        assert!(matches!(
            decode(&bytes),
            Err(PackError::WrongVersion { found: 99, .. })
        ));
    }
}
