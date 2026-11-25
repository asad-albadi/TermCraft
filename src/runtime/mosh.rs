//! Private launch images for the declared Linux Mosh runtime.
use super::{RuntimeCatalog, manifest::VerifiedArtifact};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

const PACKAGE_WRAPPER: &str = "runtime/linux-x86_64/mosh";

pub(crate) struct MoshLaunch {
    pub(crate) program: PathBuf,
    pub(crate) args: Vec<String>,
    pub(crate) libraries: PathBuf,
    pub(crate) terminfo: PathBuf,
    pub(crate) ssh_sk_helper: PathBuf,
    pub(crate) ssh_pkcs11_helper: PathBuf,
    _private: tempfile::TempDir,
}

pub(crate) fn prepare_mosh_launch(
    program: &str,
    args: &[String],
    cancelled: &AtomicBool,
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
    let wrapper = catalog.mosh_artifact(super::ToolId::Mosh)?;
    let client = catalog.mosh_artifact(super::ToolId::MoshClient)?;
    let client_arg = format!("--client={}", client.path.display());
    let mut args = args.to_vec();
    let selected = args
        .iter_mut()
        .find(|arg| **arg == client_arg)
        .ok_or("Packaged Mosh launch must select the verified client")?;

    let (_, private) = super::mosh_dependencies::prepare(&catalog.resources, cancelled)?;
    let program = private.path().join("mosh");
    let client_path = private.path().join("mosh-client");
    let perl = private.path().join("bin/perl");
    let native_modules = private.path().join("usr/lib/perl5/5.44/core_perl");
    let modules = private.path().join("usr/share/perl5/core_perl");
    // $0 is invoked recursively by OpenSSH ProxyCommand --fake-proxy. Its
    // shebang and @INC must describe this same private closure, not the host.
    let source = std::str::from_utf8(&wrapper.bytes).map_err(|_| "Invalid Mosh wrapper text")?;
    let (_, body) = source
        .split_once('\n')
        .ok_or("Missing Mosh wrapper shebang")?;
    let launch_wrapper = format!(
        "#!{}\nBEGIN {{ @INC = ('{}', '{}'); }}\n{}",
        perl.display(),
        native_modules.display(),
        modules.display(),
        body
    );
    private_bytes(&program, launch_wrapper.as_bytes())?;
    private_image(&client_path, &client)?;
    *selected = format!("--client={}", client_path.display());
    let installed_ssh = catalog
        .resources
        .join("runtime/linux-x86_64/mosh-deps/bin/ssh");
    let ssh_option = args
        .iter()
        .position(|arg| arg == "--ssh")
        .ok_or("Packaged Mosh launch must select the verified SSH client")?;
    let command = args
        .get_mut(ssh_option + 1)
        .ok_or("Missing packaged Mosh SSH command")?;
    let expected = format!(
        "'{}'",
        installed_ssh.to_string_lossy().replace('\'', "'\\''")
    );
    let tail = command
        .strip_prefix(&expected)
        .filter(|tail| tail.is_empty() || tail.starts_with(' '))
        .ok_or("Packaged Mosh launch must select the verified SSH client")?;
    *command = format!("'{}'{}", private.path().join("bin/ssh").display(), tail);
    Ok(Some(MoshLaunch {
        program,
        args,
        libraries: private.path().join("lib"),
        terminfo: private.path().join("terminfo"),
        ssh_sk_helper: private.path().join("bin/ssh-sk-helper"),
        ssh_pkcs11_helper: private.path().join("bin/ssh-pkcs11-helper"),
        _private: private,
    }))
}

fn private_image(path: &Path, artifact: &VerifiedArtifact) -> Result<(), String> {
    private_bytes(path, &artifact.bytes)
}

fn private_bytes(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file: File = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o500)
        .open(path)
        .map_err(failure)?;
    file.write_all(bytes).map_err(failure)
}

fn failure(error: std::io::Error) -> String {
    format!("Could not prepare private Mosh launch images: {error}")
}
