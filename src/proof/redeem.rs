use serde_json::Value;

use crate::net::client::Api;
use crate::support::consts::POW_REDEEM;
use crate::support::error::{AppError, AppResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Granted {
    pub session_token: String,
    pub quota: u32,
}

pub async fn redeem(
    api: &Api,
    token: &str,
    challenge_id: &str,
    proof: &str,
    solve_ms: u64,
) -> AppResult<Granted> {
    let body = serde_json::json!({
        "challenge_id": challenge_id,
        "proof": proof,
        "solve_ms": solve_ms,
    });
    let reply = api.post(POW_REDEEM, token, body).await?;
    Granted::parse(&reply)
}

impl Granted {
    pub fn parse(value: &Value) -> AppResult<Granted> {
        let session_token = pick(value, &["session_token", "sessionToken", "token"])
            .ok_or_else(|| AppError::proof("the redeem reply carried no session token"))?;
        let quota = pick_u64(value, &["quota", "remaining"])
            .unwrap_or(5)
            .min(u32::MAX as u64) as u32;
        Ok(Granted {
            session_token,
            quota,
        })
    }
}

fn pick(value: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(found) = value.get(*key).and_then(|v| v.as_str()) {
            if !found.is_empty() {
                return Some(found.to_string());
            }
        }
    }
    if let Some(data) = value.get("data") {
        for key in keys {
            if let Some(found) = data.get(*key).and_then(|v| v.as_str()) {
                if !found.is_empty() {
                    return Some(found.to_string());
                }
            }
        }
    }
    None
}

fn pick_u64(value: &Value, keys: &[&str]) -> Option<u64> {
    for key in keys {
        if let Some(found) = value.get(*key).and_then(|v| v.as_u64()) {
            return Some(found);
        }
    }
    if let Some(data) = value.get("data") {
        for key in keys {
            if let Some(found) = data.get(*key).and_then(|v| v.as_u64()) {
                return Some(found);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_redeem_reads_the_token_and_the_quota() {
        let value: Value =
            serde_json::from_str(r#"{"status":"success","session_token":"abc","quota":5}"#)
                .unwrap();
        let granted = Granted::parse(&value).unwrap();
        assert_eq!(granted.session_token, "abc");
        assert_eq!(granted.quota, 5);
    }

    #[test]
    fn the_quota_defaults_to_five_when_it_is_missing() {
        let value: Value = serde_json::from_str(r#"{"session_token":"abc"}"#).unwrap();
        assert_eq!(Granted::parse(&value).unwrap().quota, 5);
    }

    #[test]
    fn a_wrapped_reply_is_still_read() {
        let value: Value =
            serde_json::from_str(r#"{"data":{"session_token":"abc","quota":4}}"#).unwrap();
        let granted = Granted::parse(&value).unwrap();
        assert_eq!(granted.session_token, "abc");
        assert_eq!(granted.quota, 4);
    }

    #[test]
    fn a_reply_without_a_session_is_refused() {
        let value: Value = serde_json::from_str(r#"{"ok":true,"proof":"x"}"#).unwrap();
        assert!(Granted::parse(&value).is_err());
    }
}
