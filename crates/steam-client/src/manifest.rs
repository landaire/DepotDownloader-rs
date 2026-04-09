use std::io::Read;

use steam::depot::manifest::DepotManifest;
use steam::error::ManifestError;

/// Extract a manifest from the CDN response (ZIP-compressed) and parse it.
pub fn extract_and_parse(data: &[u8]) -> Result<DepotManifest, ManifestError> {
    let raw = extract_zip(data)?;
    DepotManifest::parse(&raw)
}

/// Extract the first file from a ZIP archive.
fn extract_zip(data: &[u8]) -> Result<Vec<u8>, ManifestError> {
    let cursor = std::io::Cursor::new(data);
    let mut archive =
        zip::ZipArchive::new(cursor).map_err(|_| ManifestError::MissingSection("zip archive"))?;

    if archive.len() == 0 {
        return Err(ManifestError::MissingSection("empty zip archive"));
    }

    let mut file = archive
        .by_index(0)
        .map_err(|_| ManifestError::MissingSection("zip entry"))?;

    let mut buf = Vec::with_capacity(file.size() as usize);
    file.read_to_end(&mut buf)
        .map_err(|_| ManifestError::MissingSection("zip read failed"))?;
    Ok(buf)
}
