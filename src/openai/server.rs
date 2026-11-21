use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

use crate::openai::{prompt, route, sse};
use crate::support::consts::MODEL_ID;
use crate::support::error::{AppError, AppResult};
use crate::support::log::{lg, lr, ly};

const MAX_HEADER_BYTES: usize = 32 * 1024;
const MAX_BODY_BYTES: usize = 4 * 1024 * 1024;
const READ_IDLE_SECS: u64 = 120;

pub struct Server {
    pub bridge: Arc<route::Bridge>,
}

impl Server {
    pub async fn serve(&self, port: u16) -> AppResult<()> {
        let listener = TcpListener::bind(("0.0.0.0", port)).await.map_err(|err| {
            AppError::network(format!("port {} could not be bound: {}", port, err))
        })?;
        lg(&format!("Listening on http://0.0.0.0:{}/v1", port));

        loop {
            match listener.accept().await {
                Ok((stream, peer)) => {
                    let bridge = self.bridge.clone();
                    tokio::spawn(async move {
                        if let Err(err) = handle(stream, bridge).await {
                            lr(&format!("The request from {} failed: {}", peer, err));
                        }
                    });
                }
                Err(err) => {
                    ly(&format!("The listener paused for a moment: {}", err));
                    tokio::time::sleep(Duration::from_millis(250)).await;
                }
            }
        }
    }
}

async fn handle(mut stream: TcpStream, bridge: Arc<route::Bridge>) -> AppResult<()> {
    let request = read_request(&mut stream).await?;

    let path = request.path.split('?').next().unwrap_or(&request.path);
    match (request.method.as_str(), path) {
        ("GET", "/v1/models") => {
            let body = sse::model_list().to_string();
            write_reply(&mut stream, 200, "application/json", &body).await
        }
        ("POST", "/v1/chat/completions") => {
            let value: Value = serde_json::from_str(&request.body)
                .map_err(|_| AppError::api("the request body is not valid json"))?;
            let parsed = match prompt::parse(&value) {
                Ok(parsed) => parsed,
                Err(message) => {
                    let body = crate::openai::error::body(
                        &message,
                        "invalid_request_error",
                        "bad_request",
                    )
                    .to_string();
                    return write_reply(&mut stream, 400, "application/json", &body).await;
                }
            };
            if !parsed.model.is_empty() && parsed.model != MODEL_ID {
                ly(&format!(
                    "The client asked for {} but the bridge always serves {}",
                    parsed.model, MODEL_ID
                ));
            }
            if parsed.stream {
                stream_chat(stream, bridge, parsed).await
            } else {
                whole_chat(stream, bridge, parsed).await
            }
        }
        _ => {
            let body = crate::openai::error::body(
                "the path is not served",
                "invalid_request_error",
                "not_found",
            )
            .to_string();
            write_reply(&mut stream, 404, "application/json", &body).await
        }
    }
}

async fn whole_chat(
    mut stream: TcpStream,
    bridge: Arc<route::Bridge>,
    request: prompt::Request,
) -> AppResult<()> {
    let id = chat_id();
    let created = created_at();
    match route::answer(&bridge, &request, None).await {
        Ok(reply) => {
            let (text, calls, finish) = route::visible_of(&reply, &request);
            let body = if calls.is_empty() {
                sse::whole(&id, created, &request, &text, finish)
            } else {
                sse::whole_with_calls(&id, created, &text, &calls, finish)
            };
            lg(&format!(
                "Answered one request, {} threads swept",
                reply.purged
            ));
            write_reply(&mut stream, 200, "application/json", &body.to_string()).await
        }
        Err(err) => {
            let status = crate::openai::error::http_status(&err);
            let body = crate::openai::error::from_error(&err).to_string();
            write_reply(&mut stream, status, "application/json", &body).await
        }
    }
}

async fn stream_chat(
    mut stream: TcpStream,
    bridge: Arc<route::Bridge>,
    request: prompt::Request,
) -> AppResult<()> {
    let id = chat_id();
    let created = created_at();

    stream
        .write_all(
            b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: keep-alive\r\nX-Accel-Buffering: no\r\n\r\n",
        )
        .await
        .map_err(|err| AppError::network(format!("the stream head failed: {}", err)))?;
    let _ = stream.flush().await;

    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    let bridge_task = bridge.clone();
    let request_clone = request.clone();

    let worker =
        tokio::spawn(async move { route::answer(&bridge_task, &request_clone, Some(tx)).await });

    let head = sse::opening(&id, created);
    let _ = stream.write_all(head.as_bytes()).await;
    let _ = stream.flush().await;

    let mut broken = false;
    while let Some(piece) = rx.recv().await {
        let frame = sse::text(&id, created, &piece);
        if stream.write_all(frame.as_bytes()).await.is_err() {
            broken = true;
            break;
        }
        let _ = stream.flush().await;
    }
    if broken {
        let _ = worker.await;
        return Ok(());
    }

    match worker.await {
        Ok(Ok(result)) => {
            let request_for_tools = request.clone();
            if !request_for_tools.tools.is_empty() {
                let pieces = crate::openai::tool::parse_blocks(&result.text);
                if crate::openai::tool::has_calls(&pieces) {
                    let nonce = crate::support::log::nonce(crate::support::consts::TOOL_NONCE_LEN);
                    let calls = crate::openai::tool::calls(&pieces, &nonce);
                    let mut tail = String::new();
                    for call in calls {
                        tail.push_str(&sse::chunk(
                            &id,
                            created,
                            serde_json::json!({"tool_calls": [call]}),
                            None,
                        ));
                    }
                    tail.push_str(&sse::close(&id, created, "tool_calls"));
                    tail.push_str(&sse::done());
                    let _ = stream.write_all(tail.as_bytes()).await;
                    let _ = stream.flush().await;
                    lg(&format!(
                        "Answered one stream with tool calls, {} threads swept",
                        result.purged
                    ));
                    return Ok(());
                }
            }
            let tail = format!("{}{}", sse::close(&id, created, "stop"), sse::done());
            let _ = stream.write_all(tail.as_bytes()).await;
            let _ = stream.flush().await;
            lg(&format!(
                "Answered one stream, {} threads swept",
                result.purged
            ));
            Ok(())
        }
        Ok(Err(err)) => {
            let frame = sse::json_line(&crate::openai::error::from_error(&err));
            let tail = format!("{}{}", frame, sse::done());
            let _ = stream.write_all(tail.as_bytes()).await;
            let _ = stream.flush().await;
            Ok(())
        }
        Err(err) => Err(AppError::network(format!("the worker stopped: {}", err))),
    }
}

struct Incoming {
    method: String,
    path: String,
    body: String,
}

async fn read_request(stream: &mut TcpStream) -> AppResult<Incoming> {
    let mut buffer: Vec<u8> = Vec::with_capacity(4096);
    let mut chunk = [0u8; 4096];
    let head_end;

    loop {
        let read =
            tokio::time::timeout(Duration::from_secs(READ_IDLE_SECS), stream.read(&mut chunk))
                .await
                .map_err(|_| AppError::network("the client went quiet"))?
                .map_err(|err| {
                    AppError::network(format!("the request could not be read: {}", err))
                })?;
        if read == 0 {
            return Err(AppError::network("the client closed early"));
        }
        buffer.extend_from_slice(&chunk[..read]);
        if let Some(found) = find_head(&buffer) {
            head_end = found;
            break;
        }
        if buffer.len() > MAX_HEADER_BYTES {
            return Err(AppError::network("the request head is too large"));
        }
    }

    let head = String::from_utf8_lossy(&buffer[..head_end]).to_string();
    let mut lines = head.lines();
    let first = lines.next().unwrap_or_default();
    let mut parts = first.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let path = parts.next().unwrap_or_default().to_string();

    let length = head
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.trim().eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.trim().parse::<usize>().ok())
        .unwrap_or(0);
    if length > MAX_BODY_BYTES {
        return Err(AppError::network("the request body is too large"));
    }

    let mut body = buffer[head_end + 4..].to_vec();
    while body.len() < length {
        let read =
            tokio::time::timeout(Duration::from_secs(READ_IDLE_SECS), stream.read(&mut chunk))
                .await
                .map_err(|_| AppError::network("the client went quiet mid-body"))?
                .map_err(|err| AppError::network(format!("the body could not be read: {}", err)))?;
        if read == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..read]);
    }
    body.truncate(length);

    Ok(Incoming {
        method,
        path,
        body: String::from_utf8_lossy(&body).to_string(),
    })
}

fn find_head(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

async fn write_reply(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &str,
) -> AppResult<()> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        _ => "OK",
    };
    let head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        status,
        reason,
        content_type,
        body.len()
    );
    stream
        .write_all(head.as_bytes())
        .await
        .map_err(|err| AppError::network(format!("the reply head failed: {}", err)))?;
    stream
        .write_all(body.as_bytes())
        .await
        .map_err(|err| AppError::network(format!("the reply body failed: {}", err)))?;
    let _ = stream.flush().await;
    Ok(())
}

pub fn chat_id() -> String {
    format!("chatcmpl-{}", crate::support::log::nonce(24))
}

pub fn created_at() -> i64 {
    chrono::Utc::now().timestamp()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_head_is_found_at_the_blank_line() {
        let buffer = b"GET / HTTP/1.1\r\nHost: x\r\n\r\nbody";
        assert_eq!(find_head(buffer), Some(23));
    }

    #[test]
    fn a_missing_head_keeps_reading() {
        assert!(find_head(b"GET / HTTP/1.1\r\nHost: x\r\n").is_none());
    }

    #[test]
    fn every_chat_id_is_unique() {
        let first = chat_id();
        let second = chat_id();
        assert!(first.starts_with("chatcmpl-"));
        assert_ne!(first, second);
    }

    #[test]
    fn the_created_time_is_seconds() {
        let now = created_at();
        assert!(now > 1_700_000_000);
    }
}
