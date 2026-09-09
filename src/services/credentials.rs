//! CalDAV credential storage via the system keyring.
//!
//! Passwords are never persisted to disk in plaintext. They live in the OS
//! secret store (GNOME Keyring / KWallet via the Secret Service D-Bus API).
//! The keyring entry is keyed by `service = "xcalendar"` and
//! `user = "<username>@<host>"` so one user on multiple servers does not
//! collide.
//!
//! Security note (repo CLAUDE.md): never log the password or a full URL that
//! embeds credentials. Log only the host.

use keyring::Entry;
use url::Url;

/// Errors from credential storage operations.
#[derive(Debug)]
pub enum CredentialError {
    /// No system keyring is reachable (headless / no Secret Service).
    KeyringUnavailable,
    /// The requested credential was not found.
    NotFound,
    /// Some other failure (message is safe to log — no secrets).
    Other(String),
}

impl std::fmt::Display for CredentialError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CredentialError::KeyringUnavailable => {
                write!(f, "Credential storage unavailable — CalDAV requires a system keyring")
            }
            CredentialError::NotFound => write!(f, "No stored credential for this account"),
            CredentialError::Other(msg) => write!(f, "Credential error: {}", msg),
        }
    }
}

impl std::error::Error for CredentialError {}

/// Map a keyring error onto our error type.
fn map_keyring_error(e: keyring::Error) -> CredentialError {
    match e {
        keyring::Error::NoEntry => CredentialError::NotFound,
        keyring::Error::NoDefaultStore => CredentialError::KeyringUnavailable,
        other => CredentialError::Other(other.to_string()),
    }
}

/// Stored CalDAV credentials.
///
/// Currently used as a namespace for the associated `store`/`load`/`delete`
/// functions; the struct itself is not yet instantiated as a value.
#[derive(Debug, Clone)]
#[allow(dead_code)] // Reserved for future credential display / account management
pub struct CalDavCredentials {
    pub username: String,
    pub password: String,
}

/// Extract the host from a server URL, falling back to the raw string.
pub fn host_of(server_url: &str) -> String {
    Url::parse(server_url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_string()))
        .unwrap_or_else(|| server_url.to_string())
}

/// The keyring `user` string: `<username>@<host>`.
fn entry_user(server_url: &str, username: &str) -> String {
    format!("{}@{}", username, host_of(server_url))
}

impl CalDavCredentials {
    /// Store a password in the keyring.
    pub fn store(server_url: &str, username: &str, password: &str) -> Result<(), CredentialError> {
        let entry = Self::entry(server_url, username)?;
        entry.set_password(password).map_err(map_keyring_error)
    }

    /// Load a password from the keyring.
    pub fn load(server_url: &str, username: &str) -> Result<String, CredentialError> {
        let entry = Self::entry(server_url, username)?;
        entry.get_password().map_err(map_keyring_error)
    }

    /// Delete a stored password from the keyring. A missing entry is not an
    /// error.
    pub fn delete(server_url: &str, username: &str) -> Result<(), CredentialError> {
        let entry = Self::entry(server_url, username)?;
        match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(map_keyring_error(e)),
        }
    }

    /// Whether a usable system keyring is available. Uses the store's own
    /// readiness check — no network, no specific credential touched.
    #[allow(dead_code)] // Reserved for future keyring-availability UI
    pub fn is_available() -> bool {
        Entry::store_status().is_ok()
    }

    fn entry(server_url: &str, username: &str) -> Result<Entry, CredentialError> {
        Entry::new("xcalendar", &entry_user(server_url, username)).map_err(map_keyring_error)
    }
}
