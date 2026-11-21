use serde::Deserialize;
use serde_json::{json, Value};

use crate::support::consts::{TOOL_CLOSE, TOOL_OPEN};

#[derive(Debug, Deserialize)]
pub struct Call {
    pub name: String,
    #[serde(default)]
    pub arguments: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Piece {
    Text(String),
    Call { name: String, arguments: String },
}

pub fn advertised(tools: &[Value]) -> String {
    if tools.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    out.push_str("[TOOLS]\n");
    out.push_str(
        "You may answer with one or more tool calls. Emit each call on its own line as \
         <tool>{\"name\":\"...\",\"arguments\":{...}}</tool> and nothing else on that line. \
         Do not wrap the block in prose or code fences.\n",
    );
    for tool in tools {
        let function = tool.get("function").unwrap_or(tool);
        let name = function
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("tool");
        let description = function
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let parameters = function
            .get("parameters")
            .cloned()
            .unwrap_or_else(|| json!({}));
        out.push_str("- ");
        out.push_str(name);
        out.push_str(": ");
        out.push_str(description);
        out.push_str("\n  parameters: ");
        out.push_str(&parameters.to_string());
        out.push('\n');
    }
    out.push_str("[/TOOLS]");
    out
}

pub fn parse_blocks(text: &str) -> Vec<Piece> {
    let mut pieces: Vec<Piece> = Vec::new();
    let mut rest = text;

    while let Some(start) = rest.find(TOOL_OPEN) {
        let before = &rest[..start];
        if !before.trim().is_empty() {
            pieces.push(Piece::Text(before.to_string()));
        }
        let after_open = &rest[start + TOOL_OPEN.len()..];
        let Some(end) = after_open.find(TOOL_CLOSE) else {
            pieces.push(Piece::Text(rest[start..].to_string()));
            rest = "";
            break;
        };
        let inside = &after_open[..end];
        match serde_json::from_str::<Call>(inside.trim()) {
            Ok(call) => {
                let arguments = if call.arguments.is_null() {
                    "{}".to_string()
                } else {
                    call.arguments.to_string()
                };
                pieces.push(Piece::Call {
                    name: call.name,
                    arguments,
                });
            }
            Err(_) => pieces.push(Piece::Text(format!(
                "{}{}{}",
                TOOL_OPEN, inside, TOOL_CLOSE
            ))),
        }
        rest = &after_open[end + TOOL_CLOSE.len()..];
    }

    if !rest.trim().is_empty() {
        pieces.push(Piece::Text(rest.to_string()));
    }

    pieces
}

pub fn has_calls(pieces: &[Piece]) -> bool {
    pieces
        .iter()
        .any(|piece| matches!(piece, Piece::Call { .. }))
}

pub fn visible_text(pieces: &[Piece]) -> String {
    let mut out = String::new();
    for piece in pieces {
        if let Piece::Text(text) = piece {
            if !out.is_empty() && !out.ends_with(char::is_whitespace) {
                out.push(' ');
            }
            out.push_str(text.trim());
        }
    }
    out.trim().to_string()
}

pub fn calls(pieces: &[Piece], nonce: &str) -> Vec<Value> {
    pieces
        .iter()
        .enumerate()
        .filter_map(|(index, piece)| match piece {
            Piece::Call { name, arguments } => Some(json!({
                "id": format!("call_{}_{}", nonce, index),
                "type": "function",
                "function": {
                    "name": name,
                    "arguments": arguments,
                }
            })),
            Piece::Text(_) => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tools() -> Vec<Value> {
        vec![json!({
            "type": "function",
            "function": {
                "name": "get_weather",
                "description": "Read the weather",
                "parameters": {"type":"object","properties":{"city":{"type":"string"}}}
            }
        })]
    }

    #[test]
    fn the_tool_block_lists_every_tool() {
        let text = advertised(&tools());
        assert!(text.starts_with("[TOOLS]"));
        assert!(text.ends_with("[/TOOLS]"));
        assert!(text.contains("get_weather"));
        assert!(text.contains("Read the weather"));
        assert!(text.contains("<tool>"));
    }

    #[test]
    fn no_tools_means_no_block() {
        assert!(advertised(&[]).is_empty());
    }

    #[test]
    fn a_tool_block_is_split_from_the_prose() {
        let pieces = parse_blocks(
            "Sure.\n<tool>{\"name\":\"get_weather\",\"arguments\":{\"city\":\"Oslo\"}}</tool>",
        );
        assert_eq!(pieces.len(), 2);
        assert_eq!(pieces[0], Piece::Text("Sure.\n".to_string()));
        assert_eq!(
            pieces[1],
            Piece::Call {
                name: "get_weather".to_string(),
                arguments: "{\"city\":\"Oslo\"}".to_string()
            }
        );
        assert!(has_calls(&pieces));
        assert_eq!(visible_text(&pieces), "Sure.");
    }

    #[test]
    fn two_calls_are_both_kept() {
        let pieces = parse_blocks(
            "<tool>{\"name\":\"a\",\"arguments\":{}}</tool><tool>{\"name\":\"b\",\"arguments\":{}}</tool>",
        );
        assert_eq!(calls(&pieces, "n1").len(), 2);
    }

    #[test]
    fn the_openai_call_shape_is_emitted() {
        let pieces = parse_blocks("<tool>{\"name\":\"a\",\"arguments\":{\"x\":1}}</tool>");
        let built = calls(&pieces, "n1");
        assert_eq!(built[0]["type"], "function");
        assert_eq!(built[0]["id"], "call_n1_0");
        assert_eq!(built[0]["function"]["name"], "a");
        let args = built[0]["function"]["arguments"].as_str().unwrap();
        assert_eq!(args, "{\"x\":1}");
    }

    #[test]
    fn a_broken_block_stays_as_prose() {
        let pieces = parse_blocks("<tool>not json</tool>");
        assert!(!has_calls(&pieces));
        assert!(matches!(&pieces[0], Piece::Text(text) if text.contains("not json")));
    }

    #[test]
    fn an_unclosed_block_stays_as_prose() {
        let pieces = parse_blocks("hi <tool>{\"name\":\"a\"}");
        assert!(!has_calls(&pieces));
    }

    #[test]
    fn plain_text_is_a_single_piece() {
        let pieces = parse_blocks("just words");
        assert_eq!(pieces, vec![Piece::Text("just words".to_string())]);
        assert_eq!(visible_text(&pieces), "just words");
    }

    #[test]
    fn a_null_argument_block_becomes_empty_json() {
        let pieces = parse_blocks("<tool>{\"name\":\"a\",\"arguments\":null}</tool>");
        match &pieces[0] {
            Piece::Call { arguments, .. } => assert_eq!(arguments, "{}"),
            other => panic!("wrong piece {:?}", other),
        }
    }
}
