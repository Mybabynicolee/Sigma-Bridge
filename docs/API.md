# SigmaBridge API

SigmaBridge exposes a small, OpenAI-compatible HTTP API for text chat. It serves the `sigma-flash` model and implements model listing, chat completions, optional SSE streaming, and best-effort function/tool-call formatting.

## Base URL and authentication

The default base URL is:

```text
http://localhost:1122/v1
```

The port defaults to `1122` and can be changed with `settings.port` in `config.json`. The server listens on `0.0.0.0` using plain HTTP. **No API key is required, and the server does not authenticate requests.** Do not expose it to an untrusted network as-is; put it behind a TLS/authentication reverse proxy or restrict network access.

## Endpoints

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/v1/models` | List the model provided by the bridge. |
| `POST` | `/v1/chat/completions` | Create a text chat completion, optionally as an SSE stream. |

Other paths, including other OpenAI API routes, are not implemented and return `404`.

### List models

```bash
curl http://localhost:1122/v1/models
```

Example response:

```json
{
  "object": "list",
  "data": [
    {
      "id": "sigma-flash",
      "object": "model",
      "created": 0,
      "owned_by": "sigma"
    }
  ]
}
```

### Create a chat completion

Send a JSON request to `POST /v1/chat/completions`. `Content-Type: application/json` is recommended. An `Authorization` header is not needed.

```bash
curl http://localhost:1122/v1/chat/completions \
  -H 'Content-Type: application/json' \
  -d '{
    "model": "sigma-flash",
    "messages": [
      {"role": "system", "content": "Answer concisely."},
      {"role": "user", "content": "What is a websocket?"}
    ]
  }'
```

#### Request fields

| Field | Type | Behavior |
|---|---|---|
| `messages` | array | Required and must contain at least one message. Each message may have `role` and `content`. The request is flattened into a labeled text transcript for the upstream service. |
| `model` | string | Use `sigma-flash`. If omitted or set to another string, the request is still served by `sigma-flash`; an unexpected model name is logged, not rejected. |
| `stream` | boolean | Optional; defaults to `false`. Set to `true` for Server-Sent Events (SSE). |
| `tools` | array | Optional function/tool definitions. See [Tool calls](#tool-calls). |
| `tool_choice` | string or object | Optional. The string `"none"` disables tool handling. Other values permit tool handling but do not force a particular tool to be called. |

Message roles are mapped as follows: `system` and `developer` become `system`; `assistant` stays `assistant`; `tool` and `function` become `tool`. Other or missing roles are treated as `user`.

`content` may be a string or an array of content parts. For an array, the bridge joins any parts with a string `text` property using newlines. Non-text content, including images, is not sent through as multimodal input. Empty non-assistant messages are skipped; an empty or effectively text-free message list is rejected.

The bridge only reads the fields listed above. Common OpenAI options such as `temperature`, `top_p`, `max_tokens`, `n`, `stop`, and `response_format` are not applied.

#### Non-streaming response

A successful non-streaming request returns a JSON object with the usual chat-completion envelope. `usage` is currently reported as zero for all token counts; it is not a measured usage value.

```json
{
  "id": "chatcmpl-…",
  "object": "chat.completion",
  "created": 0,
  "model": "sigma-flash",
  "choices": [
    {
      "index": 0,
      "message": {
        "role": "assistant",
        "content": "A WebSocket is a persistent, two-way connection between a client and a server."
      },
      "finish_reason": "stop"
    }
  ],
  "usage": {
    "prompt_tokens": 0,
    "completion_tokens": 0,
    "total_tokens": 0
  },
  "system_fingerprint": "sigma"
}
```

The `created` value is a Unix timestamp in seconds. The `id` is generated for each response.

## Streaming

Set `"stream": true`. The response uses `Content-Type: text/event-stream` and sends SSE `data:` frames containing OpenAI-style `chat.completion.chunk` objects, followed by `data: [DONE]`.

```bash
curl -N http://localhost:1122/v1/chat/completions \
  -H 'Content-Type: application/json' \
  -d '{
    "model": "sigma-flash",
    "stream": true,
    "messages": [{"role": "user", "content": "Say hello."}]
  }'
```

The stream begins with an assistant-role chunk, then sends content chunks, a final chunk with `finish_reason: "stop"`, and the `[DONE]` marker. Errors that occur after the SSE response begins are sent as an SSE JSON error frame, followed by `[DONE]`.

Streaming tool-call support is less complete than standard OpenAI tool-call streaming: generated text is sent as it arrives, and any parsed tool calls are sent in trailing chunks. Clients should not rely on incremental function-argument deltas.

## Tool calls

Pass OpenAI-style function definitions in `tools`. SigmaBridge includes the tool names, descriptions, and parameter schemas in the text prompt and parses a response only when it contains the bridge's tool markup:

```text
<tool>{"name":"tool_name","arguments":{"key":"value"}}</tool>
```

If such markup is parsed, a non-streaming response contains `message.tool_calls` and `finish_reason: "tool_calls"`. The `function.arguments` value is a JSON-encoded string, as in the OpenAI chat-completion shape. Any ordinary assistant text outside the markup is returned as `message.content` (or `null` if there is no such text).

The bridge formats calls for the client; it does **not** execute the functions. Tool use is best-effort rather than native enforcement: `tool_choice: "none"` disables it, while other `tool_choice` values merely allow parsing and do not guarantee the requested function will be selected or called.

## Errors and request limits

Structured errors use this shape:

```json
{
  "error": {
    "message": "...",
    "type": "invalid_request_error",
    "param": null,
    "code": "bad_request"
  }
}
```

| HTTP status | Typical cause | Error code/type |
|---|---|---|
| `400` | Missing `messages`, an empty messages array, or no usable text messages | `bad_request` / `invalid_request_error` |
| `404` | Unserved path | `not_found` / `invalid_request_error` |
| `401` | Upstream access token refused | `upstream_refused` / `invalid_request_error` |
| `429` | Account or upstream quota exhausted | `quota_exhausted` / `rate_limit_error` |
| `502` | Upstream, network, proof, or socket failure | `upstream_error`, `proof_failed`, or `socket_failed` / `api_error` |

The HTTP implementation accepts request bodies up to 4 MiB and headers up to 32 KiB. It reads request bodies using `Content-Length`; chunked request bodies are not handled. Note that malformed JSON currently exits request handling without a structured `400` response, so the connection may close instead.

## Server setup

Run SigmaBridge from the project directory (or specify its root) after configuring its upstream account data:

```bash
./run.sh
# or, with the built binary:
./target/release/sigmabridge serve
```

`config.json` uses a `settings` object. The default configuration is:

```json
{
  "settings": {
    "sleep_seconds": 3600,
    "round_robin": true,
    "sticky": 3,
    "port": 1122
  }
}
```

The server reads account refresh tokens from `data.txt`; alternatively, setting `SIGMA_ACCESS` makes it use that single access token instead. A missing `config.json` is created with defaults. Requests depend on valid upstream credentials and remaining upstream quota, so API availability is subject to those accounts and the upstream service.
