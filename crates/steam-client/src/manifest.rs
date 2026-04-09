use std::io::Read;
use std::path::{Path, PathBuf};

use steam::depot::manifest::DepotManifest;
use steam::depot::{DepotId, ManifestId};
use steam::error::ManifestError;
use steam::util::checksum::Sha1Hash;

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

/// Cache directory for storing downloaded manifests.
pub struct ManifestCache {
    cache_dir: PathBuf,
}

impl ManifestCache {
    /// Create a new manifest cache at the given directory.
    pub fn new(cache_dir: PathBuf) -> Self {
        Self { cache_dir }
    }

    /// Default cache location: `.depotdownloader/manifests/`
    pub fn default_for(install_dir: &Path) -> Self {
        Self::new(install_dir.join(".depotdownloader").join("manifests"))
    }

    fn manifest_path(&self, depot_id: DepotId, manifest_id: ManifestId) -> PathBuf {
        self.cache_dir
            .join(format!("{}_{}.manifest", depot_id.0, manifest_id.0))
    }

    fn sha_path(&self, depot_id: DepotId, manifest_id: ManifestId) -> PathBuf {
        self.cache_dir
            .join(format!("{}_{}.sha", depot_id.0, manifest_id.0))
    }

    /// Try to load a cached manifest. Returns None if not cached or checksum mismatch.
    pub fn load(
        &self,
        depot_id: DepotId,
        manifest_id: ManifestId,
    ) -> Option<DepotManifest> {
        let manifest_path = self.manifest_path(depot_id, manifest_id);
        let sha_path = self.sha_path(depot_id, manifest_id);

        let data = std::fs::read(&manifest_path).ok()?;
        let stored_sha = std::fs::read_to_string(&sha_path).ok()?;
        let stored_sha = stored_sha.trim();

        let actual_sha = Sha1Hash::compute(&data);
        if actual_sha.to_string() != stored_sha {
            tracing::warn!(
                "Manifest cache checksum mismatch for {depot_id}_{manifest_id}, re-downloading"
            );
            return None;
        }

        match DepotManifest::parse(&data) {
            Ok(manifest) => {
                tracing::debug!("Loaded cached manifest {depot_id}_{manifest_id}");
                Some(manifest)
            }
            Err(e) => {
                tracing::warn!("Failed to parse cached manifest: {e}");
                None
            }
        }
    }

    /// Save a manifest to the cache.
    pub fn save(
        &self,
        depot_id: DepotId,
        manifest_id: ManifestId,
        raw_data: &[u8],
    ) -> Result<(), std::io::Error> {
        std::fs::create_dir_all(&self.cache_dir)?;

        let manifest_path = self.manifest_path(depot_id, manifest_id);
        let sha_path = self.sha_path(depot_id, manifest_id);

        // Extract from ZIP first (we cache the raw manifest, not the ZIP)
        let manifest_bytes = extract_zip(raw_data)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;

        let sha = Sha1Hash::compute(&manifest_bytes);
        std::fs::write(&manifest_path, &manifest_bytes)?;
        std::fs::write(&sha_path, sha.to_string())?;

        tracing::debug!("Cached manifest {depot_id}_{manifest_id} ({} bytes)", manifest_bytes.len());
        Ok(())
    }
}
