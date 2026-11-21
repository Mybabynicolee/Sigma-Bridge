#![allow(dead_code)]

use serde_json::Value;

use crate::socket::client::Frame;
use crate::support::consts::{
    EVENT_COUNTER, EVENT_ERROR, EVENT_FINISHED, EVENT_LIMIT, EVENT_SUMMARY, EVENT_THREAD,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finish {
    pub thread_hash: Option<String>,
    pub follow_up_id: Option<String>,
    pub thread_id: Option<String>,
}

impl Finish {
    pub fn turn_id(&self) -> Option<&str> {
        self.follow_up_id.as_deref().or(self.thread_id.as_deref())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Signal {
    Text(String),
    Tracked { hash: String, follow_up_id: String },
    Counter(u32),
    Finished(Finish),
    Refused(String),
    Ignored,
}

pub fn classify(frame: &Frame) -> Signal {
    let name = frame.name();
    let data = match frame {
        Frame::Event { data, .. } => data.clone(),
        _ => Value::Null,
    };
    let inner = unwrap(&data);

    match name {
        EVENT_SUMMARY => match text_of(&inner) {
            Some(text) if !text.is_empty() => Signal::Text(text),
            _ => Signal::Ignored,
        },
        EVENT_THREAD => {
            let hash = field(&inner, &["hash", "thread_hash"]);
            let follow_up_id = field(&inner, &["followUpId", "followup_id", "parent_id"]);
            match (hash, follow_up_id) {
                (Some(hash), Some(follow_up_id)) => Signal::Tracked { hash, follow_up_id },
                _ => Signal::Ignored,
            }
        }
        EVENT_COUNTER => {
            let counter = inner
                .get("counter")
                .and_then(|v| v.as_u64())
                .or_else(|| inner.get("remaining").and_then(|v| v.as_u64()));
            match counter {
                Some(value) => Signal::Counter(value.min(u32::MAX as u64) as u32),
                None => Signal::Ignored,
            }
        }
        EVENT_FINISHED => Signal::Finished(Finish {
            thread_hash: field(&inner, &["hash", "thread_hash"]),
            follow_up_id: field(&inner, &["followUpId", "followup_id"]),
            thread_id: field(&inner, &["threadId", "thread_id"]),
        }),
        EVENT_ERROR | EVENT_LIMIT | "pow_required" | "unauthorized" => {
            Signal::Refused(text_of(&inner).unwrap_or_else(|| "the turn was refused".to_string()))
        }
        _ => {
            if name == EVENT_SUMMARY || name.is_empty() {
                return Signal::Ignored;
            }
            match inner.get("message").and_then(|v| v.as_str()) {
                Some(message) if inner.get("value").is_none() => {
                    Signal::Refused(message.to_string())
                }
                _ => Signal::Ignored,
            }
        }
    }
}

fn unwrap(value: &Value) -> Value {
    if let Some(text) = value.as_str() {
        if let Ok(parsed) = serde_json::from_str::<Value>(text) {
            return parsed;
        }
    }
    if let Some(inner) = value.get("value") {
        if let Some(text) = inner.as_str() {
            if let Ok(parsed) = serde_json::from_str::<Value>(text) {
                return parsed;
            }
        }
    }
    value.clone()
}

pub fn text_of(value: &Value) -> Option<String> {
    if let Some(text) = value.as_str() {
        return Some(text.to_string());
    }
    if let Some(text) = value.get("value").and_then(|v| v.as_str()) {
        return Some(text.to_string());
    }
    if let Some(text) = value.get("text").and_then(|v| v.as_str()) {
        return Some(text.to_string());
    }
    if let Some(text) = value.get("message").and_then(|v| v.as_str()) {
        return Some(text.to_string());
    }
    None
}

fn field(value: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(found) = value.get(*key).and_then(|v| v.as_str()) {
            if !found.is_empty() {
                return Some(found.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(raw: &str) -> Frame {
        Frame::parse(raw).unwrap()
    }

    #[test]
    fn a_summary_chunk_becomes_text() {
        let signal = classify(&frame(r#"42["summary",{"value":"Ha","seq":1}]"#));
        assert_eq!(signal, Signal::Text("Ha".to_string()));
    }

    #[test]
    fn a_summary_chunk_may_carry_a_json_string() {
        let signal = classify(&frame(r#"42["summary","{\"value\":\"Hi\"}"]"#));
        assert_eq!(signal, Signal::Text("Hi".to_string()));
    }

    #[test]
    fn an_empty_summary_chunk_is_ignored() {
        assert_eq!(
            classify(&frame(r#"42["summary",{"value":""}]"#)),
            Signal::Ignored
        );
    }

    #[test]
    fn the_thread_frame_tracks_the_raw_hash() {
        let signal = classify(&frame(
            r#"42["thread_id",{"hash":"aGFzaA","followUpId":"f-1","followup_type":"chat"}]"#,
        ));
        assert_eq!(
            signal,
            Signal::Tracked {
                hash: "aGFzaA".to_string(),
                follow_up_id: "f-1".to_string()
            }
        );
    }

    #[test]
    fn the_counter_frame_is_read() {
        let signal = classify(&frame(
            r#"42["limit_counter",{"counter":6,"query_type":"text-chat"}]"#,
        ));
        assert_eq!(signal, Signal::Counter(6));
    }

    #[test]
    fn the_finished_frame_carries_the_turn_id() {
        let signal = classify(&frame(
            r#"42["finished",{"followUpId":"f-9","threadId":"t-9","platform_type":"web"}]"#,
        ));
        match signal {
            Signal::Finished(finish) => {
                assert_eq!(finish.follow_up_id.as_deref(), Some("f-9"));
                assert_eq!(finish.thread_id.as_deref(), Some("t-9"));
                assert_eq!(finish.turn_id(), Some("f-9"));
            }
            other => panic!("wrong signal {:?}", other),
        }
    }

    #[test]
    fn the_error_frame_is_a_refusal() {
        let signal = classify(&frame(
            r#"42["error",{"message":"limit_exceeded metric=web_search_and_model"}]"#,
        ));
        match signal {
            Signal::Refused(text) => assert!(text.contains("limit_exceeded")),
            other => panic!("wrong signal {:?}", other),
        }
    }

    #[test]
    fn the_day_limit_frame_is_a_refusal() {
        let signal = classify(&frame(
            r#"42["limit_exceeded",{"message":"Request limit exceeded","query_type":"hermes","period":"day","metric":"hermes","reason":"limit"}]"#,
        ));
        match signal {
            Signal::Refused(text) => assert!(text.contains("Request limit exceeded")),
            other => panic!("wrong signal {:?}", other),
        }
    }

    #[test]
    fn an_unknown_message_frame_is_a_refusal() {
        let signal = classify(&frame(r#"42["something_new",{"message":"nope","code":7}]"#));
        assert_eq!(signal, Signal::Refused("nope".to_string()));
    }

    #[test]
    fn an_unknown_frame_is_ignored() {
        assert_eq!(
            classify(&frame(r#"42["search_found",{"n":1}]"#)),
            Signal::Ignored
        );
        assert_eq!(classify(&frame("431[]")), Signal::Ignored);
    }
}
