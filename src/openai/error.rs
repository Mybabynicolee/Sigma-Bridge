use serde_json::{json, Value};

use crate::support::error::AppError;

pub fn body(message: &str, kind: &str, code: &str) -> Value {
    json!({
        "error": {
            "message": message,
            "type": kind,
            "param": Value::Null,
            "code": code,
        }
    })
}

pub fn http_status(error: &AppError) -> u16 {
    match error {
        AppError::Config(_) => 500,
        AppError::Network(_) => 502,
        AppError::Api(message) => {
            let text = message.to_ascii_lowercase();
            if text.contains("token") || text.contains("unauthorized") {
                401
            } else if text.contains("limit") || text.contains("quota") {
                429
            } else {
                502
            }
        }
        AppError::Proof(_) => 502,
        AppError::Socket(_) => 502,
        AppError::Quota(_) => 429,
    }
}

pub fn kind_of(error: &AppError) -> &'static str {
    match error {
        AppError::Config(_) => "invalid_request_error",
        AppError::Network(_) => "api_error",
        AppError::Api(_) => "invalid_request_error",
        AppError::Proof(_) => "api_error",
        AppError::Socket(_) => "api_error",
        AppError::Quota(_) => "rate_limit_error",
    }
}

pub fn code_of(error: &AppError) -> &'static str {
    match error {
        AppError::Config(_) => "config_error",
        AppError::Network(_) => "upstream_error",
        AppError::Api(_) => "upstream_refused",
        AppError::Proof(_) => "proof_failed",
        AppError::Socket(_) => "socket_failed",
        AppError::Quota(_) => "quota_exhausted",
    }
}

pub fn from_error(error: &AppError) -> Value {
    body(&error.to_string(), kind_of(error), code_of(error))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shape_matches_the_openai_error_frame() {
        let value = body("nope", "invalid_request_error", "bad");
        assert!(value.get("error").is_some());
        let error = value.get("error").unwrap();
        assert_eq!(error.get("message").unwrap(), "nope");
        assert_eq!(error.get("type").unwrap(), "invalid_request_error");
        assert_eq!(error.get("code").unwrap(), "bad");
        assert!(error.get("param").unwrap().is_null());
    }

    #[test]
    fn a_plan_limit_reply_is_a_429() {
        let error = AppError::quota("every account has used up its plan limit, stopping");
        assert_eq!(http_status(&error), 429);
        assert_eq!(kind_of(&error), "rate_limit_error");
    }

    #[test]
    fn a_token_refusal_is_a_401() {
        let error = AppError::api("the access token was refused");
        assert_eq!(http_status(&error), 401);
    }

    #[test]
    fn a_network_fault_is_a_502() {
        assert_eq!(http_status(&AppError::network("x")), 502);
        assert_eq!(http_status(&AppError::socket("x")), 502);
        assert_eq!(http_status(&AppError::proof("x")), 502);
    }

    #[test]
    fn every_error_maps_to_a_frame() {
        let value = from_error(&AppError::proof("the proof could not be solved"));
        assert_eq!(
            value.get("error").unwrap().get("code").unwrap(),
            "proof_failed"
        );
    }
}
