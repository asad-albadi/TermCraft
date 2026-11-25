//! Private launch images for the declared Linux Mosh runtime.
use super::{RuntimeCatalog, manifest::VerifiedArtifact};
use std::fs::{File, OpenOptions, Permissions};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

const PACKAGE_WRAPPER: &str = "runtime/linux-x86_64/mosh";

pub(crate) struct MoshLaunch {
    pub(crate) program: PathBuf,
    pub(crate) args: Vec<String>,
    _private: tempfile::TempDir,
}

pub(crate) fn prepare_mosh_launch(
    program: &str,
    args: &[String],
) -> Result<Option<MoshLaunch>, String> {
    if !Path::new(program).ends_with(PACKAGE_WRAPPER) {
        return Ok(None);
    }
    let catalog = RuntimeCatalog::for_current_executable()?;
    if catalog.resources.join(PACKAGE_WRAPPER) != Path::new(program) {
        return Ok(None);
    }

    // Hash the bytes that become the private images. Replacing either installed
    // file after the session was resolved cannot change the executed bytes.
    let (wrapper, client, _) = catalog.bundled_mosh()?;
    let client_arg = format!("--client={}", client.path.display());
    let mut args = args.to_vec();
    let selected = args
        .iter_mut()
        .find(|arg| **arg == client_arg)
        .ok_or("Packaged Mosh launch must select the verified client")?;

    let private = tempfile::Builder::new()
        .prefix("termcraft-mosh-")
        .permissions(Permissions::from_mode(0o700))
        .tempdir_in("/tmp")
        .map_err(failure)?;
    let program = private.path().join("mosh");
    let client_path = private.path().join("mosh-client");
    private_image(&program, &wrapper)?;
    private_image(&client_path, &client)?;
    *selected = format!("--client={}", client_path.display());
    Ok(Some(MoshLaunch {
        program,
        args,
        _private: private,
    }))
}

fn private_image(path: &Path, artifact: &VerifiedArtifact) -> Result<(), String> {
    let mut file: File = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o500)
        .open(path)
        .map_err(failure)?;
    file.write_all(&artifact.bytes).map_err(failure)
}

fn failure(error: std::io::Error) -> String {
    format!("Could not prepare private Mosh launch images: {error}")
}
