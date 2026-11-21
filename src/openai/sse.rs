#![allow(dead_code)]

use serde_json::{json, Value};

use crate::openai::prompt::Request;
use crate::support::consts::{MODEL_ID, MODEL_OWNED, STREAM_DONE, STREAM_PING};

pub fn chunk(id: &str, created: i64, delta: Value, finish: Option<&str>) -> String {
    let body = json!({
        "id": id,
        "object": "chat.completion.chunk",
        "created": created,
        "model": MODEL_ID,
        "choices": [{
            "index": 0,
            "delta": delta,
            "finish_reason": finish,
        }],
    });
    frame(&body.to_string())
}

pub fn opening(id: &str, created: i64) -> String {
    chunk(
        id,
        created,
        json!({"role": "assistant", "content": ""}),
        None,
    )
}

pub fn text(id: &str, created: i64, piece: &str) -> String {
    chunk(id, created, json!({"content": piece}), None)
}

pub fn close(id: &str, created: i64, finish: &str) -> String {
    chunk(id, created, json!({}), Some(finish))
}

pub fn done() -> String {
    frame(STREAM_DONE)
}

pub fn ping() -> String {
    format!("{}\n\n", STREAM_PING)
}

pub fn json_line(value: &Value) -> String {
    frame(&value.to_string())
}

pub fn whole(id: &str, created: i64, request: &Request, text: &str, finish: &str) -> Value {
    let _ = request;
    json!({
        "id": id,
        "object": "chat.completion",
        "created": created,
        "model": MODEL_ID,
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": text,
            },
            "finish_reason": finish,
        }],
        "usage": {
            "prompt_tokens": 0,
            "completion_tokens": 0,
            "total_tokens": 0,
        },
        "system_fingerprint": MODEL_OWNED,
    })
}

pub fn whole_with_calls(
    id: &str,
    created: i64,
    text: &str,
    calls: &[Value],
    finish: &str,
) -> Value {
    let mut message = json!({
        "role": "assistant",
        "content": if text.is_empty() { Value::Null } else { Value::String(text.to_string()) },
    });
    if !calls.is_empty() {
        message["tool_calls"] = Value::Array(calls.to_vec());
    }
    json!({
        "id": id,
        "object": "chat.completion",
        "created": created,
        "model": MODEL_ID,
        "choices": [{
            "index": 0,
            "message": message,
            "finish_reason": finish,
        }],
        "usage": {
            "prompt_tokens": 0,
            "completion_tokens": 0,
            "total_tokens": 0,
        },
        "system_fingerprint": MODEL_OWNED,
    })
}

pub fn model_list() -> Value {
    json!({
        "object": "list",
        "data": [{
            "id": MODEL_ID,
            "object": "model",
            "created": 0,
            "owned_by": MODEL_OWNED,
        }]
    })
}

fn frame(body: &str) -> String {
    format!("data: {}\n\n", body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_frame_ends_with_a_blank_line() {
        let line = text("chatcmpl-1", 1, "hi");
        assert!(line.starts_with("data: "));
        assert!(line.ends_with("\n\n"));
    }

    #[test]
    fn the_chunk_carries_the_openai_envelope() {
        let line = text("chatcmpl-1", 1, "hi");
        let body = line.trim_start_matches("data: ").trim();
        let value: Value = serde_json::from_str(body).unwrap();
        assert_eq!(value["object"], "chat.completion.chunk");
        assert_eq!(value["model"], "sigma-flash");
        assert_eq!(value["choices"][0]["delta"]["content"], "hi");
        assert!(value["choices"][0]["finish_reason"].is_null());
    }

    #[test]
    fn the_opening_chunk_announces_the_assistant() {
        let line = opening("chatcmpl-1", 1);
        let value: Value = serde_json::from_str(line.trim_start_matches("data: ").trim()).unwrap();
        assert_eq!(value["choices"][0]["delta"]["role"], "assistant");
    }

    #[test]
    fn the_close_chunk_names_the_reason() {
        let line = close("chatcmpl-1", 1, "stop");
        let value: Value = serde_json::from_str(line.trim_start_matches("data: ").trim()).unwrap();
        assert_eq!(value["choices"][0]["finish_reason"], "stop");
        assert_eq!(value["choices"][0]["delta"], json!({}));
    }

    #[test]
    fn the_stream_ends_with_done() {
        assert_eq!(done(), "data: [DONE]\n\n");
    }

    #[test]
    fn the_ping_is_a_comment_line() {
        assert_eq!(ping(), ": ping\n\n");
    }

    #[test]
    fn the_whole_reply_has_no_stream_shape() {
        let request = Request {
            model: MODEL_ID.into(),
            stream: false,
            messages: vec![],
            tools: vec![],
            tool_choice: None,
        };
        let value = whole("chatcmpl-1", 1, &request, "hi", "stop");
        assert_eq!(value["object"], "chat.completion");
        assert_eq!(value["choices"][0]["message"]["content"], "hi");
        assert_eq!(value["choices"][0]["finish_reason"], "stop");
        assert_eq!(value["system_fingerprint"], "sigma");
    }

    #[test]
    fn a_tool_reply_carries_the_calls_and_no_text() {
        let calls =
            vec![json!({"id":"call_1","type":"function","function":{"name":"a","arguments":"{}"}})];
        let value = whole_with_calls("chatcmpl-1", 1, "", &calls, "tool_calls");
        assert!(value["choices"][0]["message"]["content"].is_null());
        assert_eq!(
            value["choices"][0]["message"]["tool_calls"][0]["function"]["name"],
            "a"
        );
        assert_eq!(value["choices"][0]["finish_reason"], "tool_calls");
    }

    #[test]
    fn the_model_list_names_one_model() {
        let value = model_list();
        assert_eq!(value["object"], "list");
        assert_eq!(value["data"][0]["id"], "sigma-flash");
        assert_eq!(value["data"][0]["owned_by"], "sigma");
    }
}
