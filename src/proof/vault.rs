#![allow(dead_code)]

use std::sync::Mutex;

use crate::net::client::Api;
use crate::proof::{challenge, redeem, solve};
use crate::support::consts::POW_SESSION_SECS;
use crate::support::error::{AppError, AppResult};
use crate::thread::ledger::now_unix;

#[derive(Debug, Clone)]
struct Slot {
    token: String,
    quota: u32,
    expires_at: i64,
}

pub struct Vault {
    action: String,
    inner: Mutex<Option<Slot>>,
}

impl Vault {
    pub fn new(action: &str) -> Vault {
        Vault {
            action: action.to_string(),
            inner: Mutex::new(None),
        }
    }

    pub fn peek(&self) -> Option<String> {
        let guard = self.inner.lock().ok()?;
        let slot = guard.as_ref()?;
        if self.usable(slot) {
            Some(slot.token.clone())
        } else {
            None
        }
    }

    pub fn remaining(&self) -> u32 {
        match self.inner.lock() {
            Ok(guard) => match guard.as_ref() {
                Some(slot) if self.usable(slot) => slot.quota,
                _ => 0,
            },
            Err(_) => 0,
        }
    }

    pub fn spend(&self) {
        if let Ok(mut guard) = self.inner.lock() {
            if let Some(slot) = guard.as_mut() {
                slot.quota = slot.quota.saturating_sub(1);
            }
        }
    }

    pub fn invalidate(&self) {
        if let Ok(mut guard) = self.inner.lock() {
            *guard = None;
        }
    }

    pub async fn session(&self, api: &Api, access: &str, user_uuid: &str) -> AppResult<String> {
        if let Some(token) = self.peek() {
            return Ok(token);
        }
        self.renew(api, access, user_uuid).await
    }

    pub async fn renew(&self, api: &Api, access: &str, user_uuid: &str) -> AppResult<String> {
        let challenge = challenge::request(api, access, user_uuid, &self.action).await?;
        let uuid = user_uuid.to_string();
        let owned = challenge.clone();
        let solved = tokio::task::spawn_blocking(move || solve::solve(&owned, &uuid))
            .await
            .map_err(|err| AppError::proof(format!("the proof task failed: {}", err)))??;

        let granted = redeem::redeem(
            api,
            access,
            &challenge.challenge_id,
            &solved.proof,
            solved.ms,
        )
        .await?;

        let slot = Slot {
            token: granted.session_token.clone(),
            quota: granted.quota,
            expires_at: now_unix() + POW_SESSION_SECS as i64,
        };
        if let Ok(mut guard) = self.inner.lock() {
            *guard = Some(slot);
        }
        Ok(granted.session_token)
    }

    fn usable(&self, slot: &Slot) -> bool {
        slot.quota > 0 && slot.expires_at > now_unix()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_vault_has_nothing_to_offer() {
        let vault = Vault::new("chat");
        assert!(vault.peek().is_none());
        assert_eq!(vault.remaining(), 0);
    }

    #[test]
    fn a_live_slot_is_handed_out() {
        let vault = Vault::new("chat");
        *vault.inner.lock().unwrap() = Some(Slot {
            token: "abc".into(),
            quota: 5,
            expires_at: now_unix() + 600,
        });
        assert_eq!(vault.peek().as_deref(), Some("abc"));
        assert_eq!(vault.remaining(), 5);
    }

    #[test]
    fn spending_quota_counts_down() {
        let vault = Vault::new("chat");
        *vault.inner.lock().unwrap() = Some(Slot {
            token: "abc".into(),
            quota: 2,
            expires_at: now_unix() + 600,
        });
        vault.spend();
        assert_eq!(vault.remaining(), 1);
        vault.spend();
        assert_eq!(vault.remaining(), 0);
        assert!(vault.peek().is_none());
    }

    #[test]
    fn spending_never_underflows() {
        let vault = Vault::new("chat");
        *vault.inner.lock().unwrap() = Some(Slot {
            token: "abc".into(),
            quota: 0,
            expires_at: now_unix() + 600,
        });
        vault.spend();
        assert_eq!(vault.remaining(), 0);
    }

    #[test]
    fn an_expired_slot_is_not_handed_out() {
        let vault = Vault::new("chat");
        *vault.inner.lock().unwrap() = Some(Slot {
            token: "abc".into(),
            quota: 5,
            expires_at: now_unix() - 1,
        });
        assert!(vault.peek().is_none());
        assert_eq!(vault.remaining(), 0);
    }

    #[test]
    fn invalidation_clears_the_slot() {
        let vault = Vault::new("chat");
        *vault.inner.lock().unwrap() = Some(Slot {
            token: "abc".into(),
            quota: 5,
            expires_at: now_unix() + 600,
        });
        vault.invalidate();
        assert!(vault.peek().is_none());
    }
}
