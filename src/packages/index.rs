use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct PackageRelease {
    pub id: String,
    pub version: String,
    pub platform: String,
    pub architecture: String,
    pub file_name: String,
    pub sha256: String,
    pub license: String,
    pub binary_sha256: String,
    pub license_sha256: String,
    pub source_url: String,
    pub source_sha256: String,
    pub commands: BTreeMap<String, String>,
}

impl PackageRelease {
    pub fn supports_current_host(&self) -> bool {
        self.platform == std::env::consts::OS && self.architecture == std::env::consts::ARCH
    }

    pub fn bundled_archive(&self) -> Option<PathBuf> {
        let exe = std::env::current_exe().ok()?;
        let bin = exe.parent()?;
        let resources = if cfg!(target_os = "macos") && bin.ends_with("Contents/MacOS") {
            bin.with_file_name("Resources")
        } else {
            bin.to_path_buf()
        };
        let path = resources.join("unix-packages").join(&self.file_name);
        path.is_file().then_some(path)
    }
}

#[derive(Debug, Deserialize)]
pub struct PackageIndex {
    schema: u32,
    releases: Vec<PackageRelease>,
}

impl PackageIndex {
    pub fn pinned() -> &'static Self {
        static INDEX: std::sync::OnceLock<PackageIndex> = std::sync::OnceLock::new();
        INDEX.get_or_init(|| {
            let index: Self =
                serde_json::from_str(include_str!("../../packaging/unix-runtime/manifest.json"))
                    .expect("compiled-in Unix package index");
            assert_eq!(index.schema, 1, "compiled-in Unix package index version");
            index
        })
    }

    pub fn available(&self) -> impl DoubleEndedIterator<Item = &PackageRelease> {
        self.releases
            .iter()
            .filter(|release| release.supports_current_host())
    }

    pub fn release(&self, package: &str, version: &str) -> Option<&PackageRelease> {
        self.available()
            .find(|release| release.id == package && release.version == version)
    }

    pub(super) fn authenticate(&self, release: &PackageRelease) -> bool {
        self.release(&release.id, &release.version) == Some(release)
    }
}
