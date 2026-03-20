use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmbeddedAsset {
    pub id: &'static str,
    pub relative_path: &'static str,
    pub bytes: &'static [u8],
}

impl EmbeddedAsset {
    pub const fn new(id: &'static str, relative_path: &'static str, bytes: &'static [u8]) -> Self {
        Self {
            id,
            relative_path,
            bytes,
        }
    }
}

pub fn materialize(asset: &EmbeddedAsset) -> Result<PathBuf> {
    let path = asset_cache_root()?.join(asset.relative_path);
    ensure_materialized(&path, asset.bytes)
        .with_context(|| format!("failed to materialize embedded asset '{}'", asset.id))?;
    Ok(path)
}

fn asset_cache_root() -> Result<PathBuf> {
    if let Ok(root) = std::env::var("OPENRIG_ASSET_CACHE_DIR") {
        return Ok(PathBuf::from(root));
    }

    dirs::data_local_dir()
        .map(|path| path.join("OpenRig").join("embedded-assets"))
        .ok_or_else(|| anyhow::anyhow!("failed to resolve application data directory"))
}

fn ensure_materialized(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Ok(existing) = fs::read(path) {
        if existing == bytes {
            return Ok(());
        }
    }

    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("invalid asset cache path '{}'", path.display()))?;
    fs::create_dir_all(parent)?;

    let temp_path = path.with_extension("tmp");
    fs::write(&temp_path, bytes)?;
    fs::rename(&temp_path, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{materialize, EmbeddedAsset};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn materialize_writes_embedded_bytes_to_cache() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should move forward")
            .as_nanos();
        let cache_root = std::env::temp_dir().join(format!("openrig_embedded_assets_{}", unique));
        std::env::set_var("OPENRIG_ASSET_CACHE_DIR", &cache_root);

        let asset = EmbeddedAsset::new("test.asset", "test/path/sample.bin", b"openrig");
        let path = materialize(&asset).expect("asset should materialize");

        assert_eq!(
            std::fs::read(&path).expect("asset bytes should exist"),
            b"openrig"
        );

        let _ = std::fs::remove_dir_all(&cache_root);
        std::env::remove_var("OPENRIG_ASSET_CACHE_DIR");
    }
}
