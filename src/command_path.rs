use once_cell::sync::OnceCell;
use std::{
    fs, io,
    path::{Path, PathBuf},
};

#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, PermissionsExt};

#[cfg(unix)]
const EXECUTE_BITS: u32 = 0o111;
#[cfg(unix)]
const WORLD_WRITABLE_BIT: u32 = 0o002;

#[derive(Debug, Clone)]
struct CachedPathError {
    kind: io::ErrorKind,
    message: String,
}

impl CachedPathError {
    fn from_io(err: io::Error) -> Self {
        Self {
            kind: err.kind(),
            message: err.to_string(),
        }
    }
}

static SSH_PATH: OnceCell<Result<PathBuf, CachedPathError>> = OnceCell::new();
static SSHPASS_PATH: OnceCell<Result<PathBuf, CachedPathError>> = OnceCell::new();
static GPG_PATH: OnceCell<Result<PathBuf, CachedPathError>> = OnceCell::new();
static COSSH_PATH: OnceCell<Result<PathBuf, CachedPathError>> = OnceCell::new();

fn resolve_cached(
    cell: &OnceCell<Result<PathBuf, CachedPathError>>,
    label: &'static str,
    resolver: impl FnOnce() -> io::Result<PathBuf>,
) -> io::Result<PathBuf> {
    let cached = cell.get_or_init(|| resolver().map_err(CachedPathError::from_io));
    match cached {
        Ok(path) => Ok(path.clone()),
        Err(err) => Err(io::Error::new(err.kind, format!("{label}: {}", err.message))),
    }
}

pub(crate) fn ssh_path() -> io::Result<PathBuf> {
    resolve_cached(&SSH_PATH, "ssh", || resolve_path_from_env("ssh"))
}

pub(crate) fn sshpass_path() -> io::Result<PathBuf> {
    resolve_cached(&SSHPASS_PATH, "sshpass", || resolve_path_from_env("sshpass"))
}

pub(crate) fn gpg_path() -> io::Result<PathBuf> {
    resolve_cached(&GPG_PATH, "gpg", || resolve_path_from_env("gpg"))
}

pub(crate) fn cossh_path() -> io::Result<PathBuf> {
    resolve_cached(&COSSH_PATH, "cossh", resolve_current_exe_path)
}

pub(crate) fn resolve_known_command_path(command: &str) -> io::Result<PathBuf> {
    match command {
        "ssh" => ssh_path(),
        "sshpass" => sshpass_path(),
        "gpg" => gpg_path(),
        "cossh" => cossh_path(),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unsupported command path lookup: {command}"),
        )),
    }
}

fn resolve_path_from_env(binary: &str) -> io::Result<PathBuf> {
    let located = which::which(binary).map_err(|err| io::Error::new(io::ErrorKind::NotFound, format!("{binary} not found in PATH: {err}")))?;
    validate_executable_path(&located, binary)
}

fn resolve_current_exe_path() -> io::Result<PathBuf> {
    let current =
        std::env::current_exe().map_err(|err| io::Error::new(io::ErrorKind::NotFound, format!("unable to resolve current executable path: {err}")))?;
    validate_executable_path(&current, "cossh")
}

fn validate_executable_path(path: &Path, label: &str) -> io::Result<PathBuf> {
    let canonical = fs::canonicalize(path).map_err(|err| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("unable to canonicalize {label} path '{}': {err}", path.display()),
        )
    })?;

    let metadata = fs::metadata(&canonical).map_err(|err| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("unable to inspect {label} path '{}': {err}", canonical.display()),
        )
    })?;

    if !metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{label} path '{}' is not a regular file", canonical.display()),
        ));
    }

    #[cfg(unix)]
    {
        validate_unix_executable_security(&canonical, &metadata, label)?;
    }

    Ok(canonical)
}

#[cfg(unix)]
fn validate_unix_executable_security(path: &Path, metadata: &fs::Metadata, label: &str) -> io::Result<()> {
    let mode = metadata.permissions().mode();
    if mode & WORLD_WRITABLE_BIT != 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("{label} path '{}' is world-writable", path.display()),
        ));
    }

    if mode & EXECUTE_BITS == 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("{label} path '{}' is not executable", path.display()),
        ));
    }

    let owner_uid = metadata.uid();
    let effective_uid = nix::unistd::Uid::effective().as_raw();
    if owner_uid != 0 && owner_uid != effective_uid {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("{label} path '{}' must be owned by root or the current user", path.display()),
        ));
    }

    Ok(())
}
