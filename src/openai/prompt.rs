use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct Request {
    pub model: String,
    pub stream: bool,
    pub messages: Vec<Message>,
    pub tools: Vec<Value>,
    pub tool_choice: Option<Value>,
}

pub fn parse(value: &Value) -> Result<Request, String> {
    let messages = value
        .get("messages")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "the body needs a messages array".to_string())?;
    if messages.is_empty() {
        return Err("the messages array is empty".to_string());
    }

    let mut parsed: Vec<Message> = Vec::with_capacity(messages.len());
    for entry in messages {
        let role = role_of(entry.get("role").and_then(|v| v.as_str()).unwrap_or("user"));
        let content = content_of(entry.get("content"));
        if content.is_empty() && role != Role::Assistant {
            continue;
        }
        parsed.push(Message { role, content });
    }
    if parsed.is_empty() {
        return Err("the messages array holds no text".to_string());
    }

    let tools = value
        .get("tools")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    Ok(Request {
        model: value
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        stream: value
            .get("stream")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        messages: parsed,
        tools,
        tool_choice: value.get("tool_choice").cloned(),
    })
}

fn role_of(raw: &str) -> Role {
    match raw {
        "system" | "developer" => Role::System,
        "assistant" => Role::Assistant,
        "tool" | "function" => Role::Tool,
        _ => Role::User,
    }
}

fn content_of(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(parts)) => {
            let mut out = String::new();
            for part in parts {
                if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                    if !out.is_empty() {
                        out.push('\n');
                    }
                    out.push_str(text);
                }
            }
            out
        }
        _ => String::new(),
    }
}

pub fn flatten(request: &Request) -> String {
    let mut out = String::new();
    for (index, message) in request.messages.iter().enumerate() {
        let label = match message.role {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::Tool => "tool",
        };
        if index > 0 {
            out.push('\n');
        }
        out.push_str(label);
        out.push_str(": ");
        out.push_str(message.content.trim());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(text: &str) -> Value {
        serde_json::from_str(text).unwrap()
    }

    #[test]
    fn the_request_reads_the_model_and_the_stream_flag() {
        let request = parse(&body(
            r#"{"model":"sigma-flash","stream":true,"messages":[{"role":"user","content":"hi"}]}"#,
        ))
        .unwrap();
        assert_eq!(request.model, "sigma-flash");
        assert!(request.stream);
        assert_eq!(request.messages.len(), 1);
        assert_eq!(request.messages[0].role, Role::User);
    }

    #[test]
    fn an_array_content_is_joined() {
        let request = parse(&body(
            r#"{"messages":[{"role":"user","content":[{"type":"text","text":"a"},{"type":"text","text":"b"}]}]}"#,
        ))
        .unwrap();
        assert_eq!(request.messages[0].content, "a\nb");
    }

    #[test]
    fn the_roles_are_normalised() {
        let request = parse(&body(
            r#"{"messages":[{"role":"developer","content":"s"},{"role":"function","content":"t"}]}"#,
        ))
        .unwrap();
        assert_eq!(request.messages[0].role, Role::System);
        assert_eq!(request.messages[1].role, Role::Tool);
    }

    #[test]
    fn the_transcript_is_flat_with_labels() {
        let request = parse(&body(
            r#"{"messages":[{"role":"system","content":"be brief"},{"role":"user","content":"hi"}]}"#,
        ))
        .unwrap();
        assert_eq!(flatten(&request), "system: be brief\nuser: hi");
    }

    #[test]
    fn a_body_without_messages_is_refused() {
        assert!(parse(&body(r#"{"model":"sigma-flash"}"#)).is_err());
    }

    #[test]
    fn an_empty_messages_array_is_refused() {
        assert!(parse(&body(r#"{"messages":[]}"#)).is_err());
    }

    #[test]
    fn the_tools_are_kept_verbatim() {
        let request = parse(&body(
            r#"{"messages":[{"role":"user","content":"hi"}],"tools":[{"type":"function","function":{"name":"x"}}]}"#,
        ))
        .unwrap();
        assert_eq!(request.tools.len(), 1);
        assert_eq!(request.tools[0]["function"]["name"], "x");
    }
}
