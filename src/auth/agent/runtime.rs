//! In-memory unlock session state for the vault agent.

use super::error::AgentError;
use crate::auth::ipc::{UnlockPolicy, VaultStatus};
use crate::auth::secret::{ExposeSecret, SensitiveString};
use crate::auth::vault::{UnlockedVault, VaultPaths};
use crate::log_debug;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use getrandom::fill as random_fill;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use zeroize::Zeroize;

pub(crate) const AGENT_IDLE_SHUTDOWN_POLL_INTERVAL_MIN: Duration = Duration::from_millis(5);
pub(crate) const AGENT_IDLE_SHUTDOWN_POLL_INTERVAL_MAX: Duration = Duration::from_millis(250);
const ASKPASS_TOKEN_TTL: Duration = Duration::from_secs(60);
const ASKPASS_TOKEN_BYTES: usize = 32;

#[derive(Debug)]
pub(super) struct AskpassLease {
    token: SensitiveString,
    entry_name: String,
    expires_at: Instant,
}

#[derive(Debug)]
pub(crate) struct AgentRuntime {
    pub(super) data_key: Option<[u8; 32]>,
    pub(super) unlocked_at: Option<Instant>,
    pub(super) last_activity_at: Option<Instant>,
    pub(super) absolute_timeout_at: Option<SystemTime>,
    pub(super) policy: Option<UnlockPolicy>,
    pub(super) askpass_leases: Vec<AskpassLease>,
}

impl AgentRuntime {
    /// Create locked runtime state with no active key material.
    pub(crate) fn new() -> Self {
        Self {
            data_key: None,
            unlocked_at: None,
            last_activity_at: None,
            absolute_timeout_at: None,
            policy: None,
            askpass_leases: Vec::new(),
        }
    }

    /// Returns `true` when session expiration caused a lock transition.
    pub(crate) fn expire_if_needed(&mut self) -> bool {
        let Some(policy) = &self.policy else {
            return false;
        };
        let Some(unlocked_at) = self.unlocked_at else {
            self.lock();
            return false;
        };
        let Some(last_activity_at) = self.last_activity_at else {
            self.lock();
            return false;
        };

        let idle_expired = last_activity_at.elapsed() >= Duration::from_secs(policy.idle_timeout_seconds);
        let absolute_expired = unlocked_at.elapsed() >= Duration::from_secs(policy.session_timeout_seconds);
        if idle_expired || absolute_expired {
            log_debug!(
                "Password vault session expired (idle_expired={}, absolute_expired={})",
                idle_expired,
                absolute_expired
            );
            self.lock();
            return true;
        }

        false
    }

    /// Build current vault status snapshot.
    pub(crate) fn status(&self, paths: &VaultPaths) -> VaultStatus {
        let vault_exists = paths.metadata_path().is_file();
        let Some(policy) = &self.policy else {
            return VaultStatus::locked(vault_exists);
        };
        let Some(unlocked_at) = self.unlocked_at else {
            return VaultStatus::locked(vault_exists);
        };
        let Some(last_activity_at) = self.last_activity_at else {
            return VaultStatus::locked(vault_exists);
        };
        let idle_remaining = Duration::from_secs(policy.idle_timeout_seconds).saturating_sub(last_activity_at.elapsed());
        let absolute_remaining = Duration::from_secs(policy.session_timeout_seconds).saturating_sub(unlocked_at.elapsed());
        let expires_in_seconds = idle_remaining.min(absolute_remaining).as_secs();
        let absolute_timeout_at_epoch_seconds = self
            .absolute_timeout_at
            .and_then(|absolute_timeout_at| absolute_timeout_at.duration_since(UNIX_EPOCH).ok())
            .map(|absolute_timeout_at| absolute_timeout_at.as_secs());

        VaultStatus {
            vault_exists,
            unlocked: self.data_key.is_some(),
            unlock_expires_in_seconds: self.data_key.map(|_| expires_in_seconds),
            idle_timeout_seconds: Some(policy.idle_timeout_seconds),
            absolute_timeout_seconds: Some(policy.session_timeout_seconds),
            absolute_timeout_at_epoch_seconds: self.data_key.and(absolute_timeout_at_epoch_seconds),
        }
    }

    /// Install decrypted data-key material and unlock policy.
    pub(crate) fn unlock(&mut self, data_key: [u8; 32], policy: UnlockPolicy) {
        let _ = self.lock();
        log_debug!(
            "Password vault runtime unlocked with idle={}s absolute={}s",
            policy.idle_timeout_seconds,
            policy.session_timeout_seconds
        );
        self.data_key = Some(data_key);
        self.unlocked_at = Some(Instant::now());
        self.last_activity_at = self.unlocked_at;
        self.absolute_timeout_at = SystemTime::now().checked_add(Duration::from_secs(policy.session_timeout_seconds));
        self.policy = Some(policy);
    }

    /// Refresh idle activity timestamp.
    pub(crate) fn touch(&mut self) {
        self.last_activity_at = Some(Instant::now());
    }

    /// Lock runtime and zeroize sensitive state. Returns previous unlock state.
    pub(crate) fn lock(&mut self) -> bool {
        let was_unlocked = self.data_key.is_some();
        if let Some(mut data_key) = self.data_key.take() {
            data_key.zeroize();
        }
        let lease_count = self.askpass_leases.len();
        self.askpass_leases.clear();
        self.unlocked_at = None;
        self.last_activity_at = None;
        self.absolute_timeout_at = None;
        self.policy = None;
        if was_unlocked {
            log_debug!("Password vault runtime key material zeroized");
        }
        if lease_count > 0 {
            log_debug!("Cleared {} outstanding askpass token(s)", lease_count);
        }
        was_unlocked
    }

    /// Build an unlocked vault handle from in-memory key material.
    pub(crate) fn unlocked_vault(&self, paths: &VaultPaths) -> Option<UnlockedVault> {
        self.data_key.map(|data_key| UnlockedVault::from_data_key(paths.clone(), data_key))
    }

    /// Issue a short-lived, single-use askpass token for one entry name.
    pub(crate) fn issue_askpass_token(&mut self, entry_name: &str) -> Result<SensitiveString, AgentError> {
        self.prune_expired_askpass_leases();
        let mut token_bytes = [0u8; ASKPASS_TOKEN_BYTES];
        random_fill(&mut token_bytes).map_err(|err| AgentError::Protocol(format!("failed to generate askpass token: {err}")))?;
        let token = SensitiveString::from_owned_string(URL_SAFE_NO_PAD.encode(token_bytes));
        token_bytes.zeroize();

        self.askpass_leases.push(AskpassLease {
            token: token.clone(),
            entry_name: entry_name.to_string(),
            expires_at: Instant::now() + ASKPASS_TOKEN_TTL,
        });
        log_debug!("Issued askpass token for entry '{}'", entry_name);
        Ok(token)
    }

    /// Consume token and return bound entry name.
    pub(crate) fn take_askpass_entry(&mut self, token: &str) -> Option<String> {
        self.prune_expired_askpass_leases();
        let index = self.askpass_leases.iter().position(|lease| lease.token.expose_secret() == token)?;
        let lease = self.askpass_leases.swap_remove(index);
        log_debug!("Consumed askpass token for entry '{}'", lease.entry_name);
        Some(lease.entry_name)
    }

    fn prune_expired_askpass_leases(&mut self) {
        let before = self.askpass_leases.len();
        let now = Instant::now();
        self.askpass_leases.retain(|lease| lease.expires_at > now);
        let removed = before.saturating_sub(self.askpass_leases.len());
        if removed > 0 {
            log_debug!("Expired {} askpass token(s)", removed);
        }
    }
}

/// Exponential backoff helper for idle-loop polling.
pub(crate) fn next_idle_shutdown_poll_interval(current: Duration) -> Duration {
    current.saturating_mul(2).min(AGENT_IDLE_SHUTDOWN_POLL_INTERVAL_MAX)
}

#[cfg(test)]
#[path = "../../test/auth/agent/runtime.rs"]
mod tests;
