use super::{PluginManifest, Result, invalid};
use crate::packages::{
    cancelled,
    install::{ArchiveSpec, Inventory, stage_archive},
    open_regular,
};
use cap_std::{ambient_authority, fs::Dir};
use std::{
    collections::BTreeSet,
    io::Read,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

pub(super) const LICENSE: &str = "notices/LICENSE";
const MANIFEST_LIMIT: u64 = 16 * 1024;

#[derive(Clone, Debug)]
pub struct ReviewedPlugin {
    manifest: PluginManifest,
    release_digest: String,
    archive: PathBuf,
}

impl ReviewedPlugin {
    pub fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    pub fn release_digest(&self) -> &str {
        &self.release_digest
    }

    pub(super) fn read(path: &Path, cancel: &AtomicBool) -> Result<Self> {
        cancelled(cancel)?;
        let path = std::env::current_dir()?.join(path);
        let parent = path
            .parent()
            .ok_or_else(|| invalid("Missing plugin bundle directory"))?;
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| invalid("Invalid plugin manifest filename"))?;
        if !name.ends_with(".termcraft-plugin.json") {
            return Err(invalid("Select a .termcraft-plugin.json manifest"));
        }
        let dir = Dir::open_ambient_dir(parent, ambient_authority())?;
        let mut bytes = Vec::new();
        open_regular(&dir, name)?
            .take(MANIFEST_LIMIT + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MANIFEST_LIMIT {
            return Err(invalid("Plugin manifest exceeds the size limit"));
        }
        let manifest: PluginManifest = serde_json::from_slice(&bytes)
            .map_err(|_| invalid("Invalid TermCraft plugin manifest"))?;
        manifest.validate()?;
        let archive = parent.join(&manifest.archive);
        let release_digest = manifest.digest()?;
        Ok(Self {
            manifest,
            release_digest,
            archive,
        })
    }

    pub(super) fn stage(&self, target: &Path, cancel: &AtomicBool) -> Result<Inventory> {
        stage_archive(
            ArchiveSpec {
                sha256: &self.manifest.archive_sha256,
                binary_sha256: &self.manifest.binary_sha256,
                platform: &self.manifest.platform,
                architecture: &self.manifest.architecture,
                commands: BTreeSet::from([self.manifest.executable_path()]),
            },
            &self.archive,
            target,
            cancel,
            None,
        )
    }
}
