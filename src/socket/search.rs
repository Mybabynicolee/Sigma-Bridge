use serde_json::{json, Value};

use crate::support::consts::PLATFORM;

pub struct Turn<'a> {
    pub query: &'a str,
    pub session_id: &'a str,
    pub token: &'a str,
    pub pow_session: &'a str,
}

pub fn payload(turn: &Turn<'_>) -> Value {
    json!({
        "query": turn.query,
        "sessionId": turn.session_id,
        "platform_type": PLATFORM,
        "utmData": utm(),
        "followupId": Value::Null,
        "domain_search": {},
        "token": turn.token,
        "powSessionToken": turn.pow_session,
    })
}

pub fn utm() -> Value {
    json!({
        "utmSource": Value::Null,
        "utmMedium": Value::Null,
        "utmCampaign": Value::Null,
        "utmTerm": Value::Null,
        "utmContent": Value::Null,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turn() -> Turn<'static> {
        Turn {
            query: "Reply with exactly: HAROK_1",
            session_id: "3f0a1c7e-5b24-4d81-9a6f-2c8e7b4105dd",
            token: "access-token",
            pow_session: "pow-token",
        }
    }

    #[test]
    fn the_search_payload_carries_the_web_platform() {
        let body = payload(&turn());
        assert_eq!(body.get("platform_type").unwrap(), "web");
    }

    #[test]
    fn the_search_payload_never_sends_a_query_type() {
        let body = payload(&turn());
        assert!(body.get("query_type").is_none());
    }

    #[test]
    fn the_search_payload_keeps_every_required_field() {
        let body = payload(&turn());
        for key in [
            "query",
            "sessionId",
            "platform_type",
            "utmData",
            "followupId",
            "domain_search",
            "token",
            "powSessionToken",
        ] {
            assert!(body.get(key).is_some(), "missing {}", key);
        }
        assert!(body.get("followupId").unwrap().is_null());
        assert_eq!(body.get("domain_search").unwrap(), &json!({}));
        assert_eq!(body.get("token").unwrap(), "access-token");
        assert_eq!(body.get("powSessionToken").unwrap(), "pow-token");
    }

    #[test]
    fn the_utm_block_is_five_nulls() {
        let utm = utm();
        assert_eq!(utm.as_object().unwrap().len(), 5);
        assert!(utm.as_object().unwrap().values().all(|v| v.is_null()));
    }
}
