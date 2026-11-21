use serde_json::{json, Value};

use crate::support::consts::PLATFORM;

pub struct Turn<'a> {
    pub query: &'a str,
    pub session_id: &'a str,
    pub token: &'a str,
    pub pow_session: &'a str,
    pub hash: &'a str,
    pub parent_id: &'a str,
}

pub fn payload(turn: &Turn<'_>) -> Value {
    json!({
        "query": turn.query,
        "sessionId": turn.session_id,
        "platform_type": PLATFORM,
        "hash": turn.hash,
        "parent_id": turn.parent_id,
        "token": turn.token,
        "domain_search": {},
        "powSessionToken": turn.pow_session,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turn() -> Turn<'static> {
        Turn {
            query: "Apa nama saya? Jawab nama saja.",
            session_id: "89556544-39a0-4675-8b0c-77713c25f612",
            token: "access-token",
            pow_session: "pow-token",
            hash: "aGFzaC1yYXc",
            parent_id: "follow-up-1",
        }
    }

    #[test]
    fn the_followup_payload_keeps_the_raw_hash() {
        let body = payload(&turn());
        assert_eq!(body.get("hash").unwrap(), "aGFzaC1yYXc");
    }

    #[test]
    fn the_hash_is_never_decoded() {
        let body = payload(&turn());
        let hash = body.get("hash").unwrap().as_str().unwrap();
        assert!(!hash.contains('%'));
        assert_eq!(hash, "aGFzaC1yYXc");
    }

    #[test]
    fn the_followup_payload_keeps_the_parent() {
        let body = payload(&turn());
        assert_eq!(body.get("parent_id").unwrap(), "follow-up-1");
    }

    #[test]
    fn the_followup_payload_keeps_the_web_platform() {
        let body = payload(&turn());
        assert_eq!(body.get("platform_type").unwrap(), "web");
        assert!(body.get("query_type").is_none());
    }

    #[test]
    fn the_followup_payload_keeps_every_required_field() {
        let body = payload(&turn());
        for key in [
            "query",
            "sessionId",
            "platform_type",
            "hash",
            "parent_id",
            "token",
            "domain_search",
            "powSessionToken",
        ] {
            assert!(body.get(key).is_some(), "missing {}", key);
        }
    }
}
