#![allow(dead_code)]

use serde::Deserialize;

use crate::net::client::Api;
use crate::support::consts::POW_CHALLENGE;
use crate::support::error::{AppError, AppResult};

#[derive(Debug, Clone, Deserialize)]
pub struct Challenge {
    pub challenge_id: String,
    pub salt: String,
    #[serde(default)]
    pub alg: String,
    #[serde(default = "default_mem")]
    pub mem_kib: u32,
    #[serde(default = "default_iter")]
    pub iter: u32,
    #[serde(default = "default_parallel")]
    pub parallelism: u32,
    #[serde(default = "default_hash")]
    pub hash_len: usize,
    #[serde(default)]
    pub expires_in: u64,
}

fn default_mem() -> u32 {
    19_456
}

fn default_iter() -> u32 {
    2
}

fn default_parallel() -> u32 {
    1
}

fn default_hash() -> usize {
    32
}

impl Challenge {
    pub fn parse(value: &serde_json::Value) -> AppResult<Challenge> {
        serde_json::from_value(value.clone())
            .map_err(|err| AppError::proof(format!("the challenge could not be read: {}", err)))
    }
}

pub async fn request(
    api: &Api,
    token: &str,
    user_uuid: &str,
    action: &str,
) -> AppResult<Challenge> {
    let body = serde_json::json!({
        "user_uuid": user_uuid,
        "action": action,
    });
    let reply = api.post(POW_CHALLENGE, token, body).await?;
    Challenge::parse(&reply)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BODY: &str = r#"{
      "challenge_id": "KB2qhgwIMV325Bw2tARCDw",
      "salt": "L-JNabBSXBq0020-RG4s8HuTw3WbM537",
      "alg": "argon2id",
      "mem_kib": 19456,
      "iter": 2,
      "parallelism": 1,
      "hash_len": 32,
      "expires_in": 60
    }"#;

    #[test]
    fn the_challenge_reads_every_field() {
        let value: serde_json::Value = serde_json::from_str(BODY).unwrap();
        let challenge = Challenge::parse(&value).unwrap();
        assert_eq!(challenge.challenge_id, "KB2qhgwIMV325Bw2tARCDw");
        assert_eq!(challenge.salt, "L-JNabBSXBq0020-RG4s8HuTw3WbM537");
        assert_eq!(challenge.alg, "argon2id");
        assert_eq!(challenge.mem_kib, 19_456);
        assert_eq!(challenge.iter, 2);
        assert_eq!(challenge.parallelism, 1);
        assert_eq!(challenge.hash_len, 32);
        assert_eq!(challenge.expires_in, 60);
    }

    #[test]
    fn the_argon2_defaults_hold_when_fields_are_missing() {
        let value: serde_json::Value =
            serde_json::from_str(r#"{"challenge_id":"a","salt":"b"}"#).unwrap();
        let challenge = Challenge::parse(&value).unwrap();
        assert_eq!(challenge.mem_kib, 19_456);
        assert_eq!(challenge.iter, 2);
        assert_eq!(challenge.parallelism, 1);
        assert_eq!(challenge.hash_len, 32);
    }

    #[test]
    fn a_reply_without_a_challenge_is_refused() {
        let value: serde_json::Value = serde_json::from_str(r#"{"salt":"b"}"#).unwrap();
        assert!(Challenge::parse(&value).is_err());
    }
}
