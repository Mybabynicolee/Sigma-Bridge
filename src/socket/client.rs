use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::WebSocketStream;

use crate::net::client::Message;
use crate::net::egress::Proxy;
use crate::support::consts::{
    BROWSER_AGENT, SOCKET_HOST, SOCKET_OPEN_SECS, SOCKET_ORIGIN, SOCKET_PATH, SOCKET_PORT,
};
use crate::support::error::{AppError, AppResult};

pub type Socket = WebSocketStream<tokio_tungstenite::MaybeTlsStream<TcpStream>>;

pub async fn open(access: &str, session_id: &str, proxy: Option<&Proxy>) -> AppResult<Socket> {
    let url = format!(
        "wss://{}{}/?EIO=4&transport=websocket",
        SOCKET_HOST, SOCKET_PATH
    );

    let mut request = url
        .into_client_request()
        .map_err(|err| AppError::socket(format!("the socket url was refused: {}", err)))?;
    request.headers_mut().insert(
        "Origin",
        SOCKET_ORIGIN
            .parse()
            .map_err(|_| AppError::socket("the socket origin was refused"))?,
    );
    request.headers_mut().insert(
        "User-Agent",
        BROWSER_AGENT
            .parse()
            .map_err(|_| AppError::socket("the socket agent was refused"))?,
    );

    let connector = crate::net::client::tls_connector();

    let opened = tokio::time::timeout(
        Duration::from_secs(SOCKET_OPEN_SECS),
        connect(request, connector, proxy),
    )
    .await
    .map_err(|_| AppError::socket("the socket handshake timed out"))??;

    let mut socket = opened;
    handshake(&mut socket, access, session_id).await?;
    Ok(socket)
}

async fn connect(
    request: tokio_tungstenite::tungstenite::handshake::client::Request,
    connector: tokio_tungstenite::Connector,
    proxy: Option<&Proxy>,
) -> AppResult<Socket> {
    match proxy {
        Some(proxy) if proxy.is_socks() => {
            let (user, pass) = proxy.credentials().unwrap_or_default();
            let stream = tokio_socks::tcp::Socks5Stream::connect_with_password(
                proxy.authority().as_str(),
                (SOCKET_HOST, SOCKET_PORT),
                user.as_str(),
                pass.as_str(),
            )
            .await
            .map_err(|err| AppError::socket(format!("the socks tunnel failed: {}", err)))?;
            tokio_tungstenite::client_async_tls_with_config(
                request,
                stream.into_inner(),
                None,
                Some(connector),
            )
            .await
            .map(|(socket, _)| socket)
            .map_err(|err| AppError::socket(format!("the socket was refused: {}", err)))
        }
        _ => {
            tokio_tungstenite::connect_async_tls_with_config(request, None, false, Some(connector))
                .await
                .map(|(socket, _)| socket)
                .map_err(|err| AppError::socket(format!("the socket was refused: {}", err)))
        }
    }
}

async fn handshake(socket: &mut Socket, access: &str, session_id: &str) -> AppResult<()> {
    let frame = next_text(socket).await?;
    if !frame.starts_with('0') {
        return Err(AppError::socket("the engine did not open with a handshake"));
    }

    socket
        .send(Message::Text("40".to_string().into()))
        .await
        .map_err(|err| AppError::socket(format!("the room join failed: {}", err)))?;

    let auth = serde_json::json!({
        "token": access,
        "sessionId": session_id,
    });
    socket
        .send(Message::Text(format!("42{}", auth).into()))
        .await
        .map_err(|err| AppError::socket(format!("the auth frame failed: {}", err)))?;

    Ok(())
}

pub async fn emit(socket: &mut Socket, event: &str, payload: &Value) -> AppResult<()> {
    let body = serde_json::to_string(payload)
        .map_err(|err| AppError::socket(format!("the payload could not be written: {}", err)))?;
    let frame = format!("42{}", serde_json::json!([event, body]));
    socket
        .send(Message::Text(frame.into()))
        .await
        .map_err(|err| AppError::socket(format!("the {} frame failed: {}", event, err)))
}

pub async fn next_frame(socket: &mut Socket, seconds: u64) -> AppResult<Frame> {
    let deadline = Duration::from_secs(seconds);
    let waited = tokio::time::timeout(deadline, async {
        loop {
            match socket.next().await {
                Some(Ok(Message::Text(text))) => return Some(text.to_string()),
                Some(Ok(Message::Binary(raw))) => {
                    return Some(String::from_utf8_lossy(&raw).to_string())
                }
                Some(Ok(Message::Ping(body))) => {
                    let _ = socket.send(Message::Pong(body)).await;
                    continue;
                }
                Some(Ok(Message::Close(_))) | None => return None,
                Some(Ok(_)) => continue,
                Some(Err(err)) => return Some(format!("__error__{}", err)),
            }
        }
    })
    .await;

    match waited {
        Ok(Some(raw)) => Frame::parse(&raw),
        Ok(None) => Err(AppError::socket("the socket closed before the turn ended")),
        Err(_) => Err(AppError::socket("the turn timed out")),
    }
}

async fn next_text(socket: &mut Socket) -> AppResult<String> {
    match next_frame(socket, SOCKET_OPEN_SECS).await? {
        Frame::Engine(payload) => Ok(format!("0{}", payload)),
        Frame::Event { name, .. } => Ok(format!("42{}", name)),
        Frame::Reply { data } => Ok(data),
        Frame::Unknown(raw) => Ok(raw),
    }
}

#[derive(Debug, Clone)]
pub enum Frame {
    Engine(String),
    Event { name: String, data: Value },
    Reply { data: String },
    Unknown(String),
}

impl Frame {
    pub fn parse(raw: &str) -> AppResult<Frame> {
        if raw.starts_with("42") {
            let body = &raw[2..];
            let parsed: Value = serde_json::from_str(body).unwrap_or(Value::Null);
            if let Some(array) = parsed.as_array() {
                let name = array
                    .first()
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                let data = array.get(1).cloned().unwrap_or(Value::Null);
                return Ok(Frame::Event { name, data });
            }
            return Ok(Frame::Unknown(raw.to_string()));
        }
        if raw.starts_with('0') {
            return Ok(Frame::Engine(raw[1..].to_string()));
        }
        if raw.starts_with("40") || raw.starts_with("41") {
            return Ok(Frame::Reply {
                data: raw.to_string(),
            });
        }
        Ok(Frame::Unknown(raw.to_string()))
    }

    pub fn name(&self) -> &str {
        match self {
            Frame::Event { name, .. } => name,
            _ => "",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_engine_open_frame_is_read() {
        let frame = Frame::parse(r#"0{"sid":"abc","upgrades":[]}"#).unwrap();
        assert!(matches!(frame, Frame::Engine(ref payload) if payload.contains("abc")));
    }

    #[test]
    fn a_summary_frame_carries_its_name_and_payload() {
        let frame = Frame::parse(r#"42["summary",{"value":"hi","seq":1}]"#).unwrap();
        assert_eq!(frame.name(), "summary");
        match frame {
            Frame::Event { data, .. } => assert_eq!(data.get("value").unwrap(), "hi"),
            _ => panic!("wrong frame"),
        }
    }

    #[test]
    fn the_payload_may_arrive_as_a_json_string() {
        let frame = Frame::parse(r#"42["summary","{\"value\":\"hi\"}"]"#).unwrap();
        assert_eq!(frame.name(), "summary");
        match frame {
            Frame::Event { data, .. } => {
                assert_eq!(data.as_str(), Some(r#"{"value":"hi"}"#));
            }
            _ => panic!("wrong frame"),
        }
    }

    #[test]
    fn a_room_reply_is_kept_as_text() {
        let frame = Frame::parse("40").unwrap();
        assert!(matches!(frame, Frame::Reply { .. }));
    }

    #[test]
    fn the_close_frame_is_survivable() {
        let frame = Frame::parse("431[]").unwrap();
        assert_eq!(frame.name(), "");
    }
}
