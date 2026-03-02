use crate::command_path;
use std::io;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasswordTransportBackend {
    UnixSshpassDirect,
    UnixSshpassTui,
    UnsupportedWindows,
}

pub fn direct_backend() -> PasswordTransportBackend {
    #[cfg(unix)]
    {
        PasswordTransportBackend::UnixSshpassDirect
    }

    #[cfg(not(unix))]
    {
        PasswordTransportBackend::UnsupportedWindows
    }
}

pub fn tui_backend() -> PasswordTransportBackend {
    #[cfg(unix)]
    {
        PasswordTransportBackend::UnixSshpassTui
    }

    #[cfg(not(unix))]
    {
        PasswordTransportBackend::UnsupportedWindows
    }
}

pub fn ensure_backend_available(backend: PasswordTransportBackend) -> io::Result<()> {
    match backend {
        PasswordTransportBackend::UnixSshpassDirect | PasswordTransportBackend::UnixSshpassTui => command_path::sshpass_path().map(|_| ()),
        PasswordTransportBackend::UnsupportedWindows => Ok(()),
    }
}

pub fn unsupported_transport_notice() -> String {
    "Password auto-login is not supported on this platform yet; continuing with the standard SSH password prompt.".to_string()
}

pub fn missing_sshpass_notice() -> String {
    "Password auto-login is unavailable because sshpass is not installed; continuing with the standard SSH password prompt.".to_string()
}
