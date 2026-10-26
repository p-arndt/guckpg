use std::time::Duration;

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::settings::{AiConfig, ProviderKind, RequestAuth};
use crate::error::{AppError, AppResult};

const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Bounds the silence between streamed chunks, not the whole response: a long
/// generation may legitimately stream for minutes.
const READ_TIMEOUT: Duration = Duration::from_secs(120);
const MODELS_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_STREAMED_TOOL_CALLS: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

pub struct Turn {
    pub text: String,
    pub calls: Vec<ToolCall>,
}

pub struct Conversation {
    config: AiConfig,
    auth: RequestAuth,
    client: reqwest::Client,
    protocol: &'static dyn Protocol,
    history: Vec<Value>,
    system: String,
}

fn malformed() -> AppError {
    AppError::msg("The AI provider returned an invalid or unsupported response.")
}

fn incomplete() -> AppError {
    AppError::msg("The AI provider did not complete its response. Try again.")
}

fn stopped_early() -> AppError {
    AppError::msg("The AI provider stopped before completing its response.")
}

fn too_large() -> AppError {
    AppError::msg("The AI provider response exceeded the size limit.")
}

fn required_string<'a>(value: &'a Value, key: &str) -> AppResult<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(malformed)
}

fn http_client() -> AppResult<reqwest::Client> {
    client_with_read_timeout(READ_TIMEOUT)
}

fn client_with_read_timeout(read_timeout: Duration) -> AppResult<reqwest::Client> {
    // The app installs the process-wide provider at startup. This is a no-op then and
    // only matters for callers (such as tests) that run without that setup.
    let _ = rustls::crypto::ring::default_provider().install_default();
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(CONNECT_TIMEOUT)
        .read_timeout(read_timeout)
        .build()
        .map_err(|_| AppError::msg("Could not initialize the AI HTTP client."))
}

fn apply_auth(request: reqwest::RequestBuilder, auth: &RequestAuth) -> reqwest::RequestBuilder {
    match auth {
        RequestAuth::None => request,
        RequestAuth::Bearer(key) => request.bearer_auth(key),
        RequestAuth::Basic(basic) => request.basic_auth(&basic.username, Some(&basic.password)),
    }
}

fn endpoint(config: &AiConfig, path: &str) -> AppResult<String> {
    Ok(format!(
        "{}/{path}",
        config.request_url()?.as_str().trim_end_matches('/')
    ))
}

/// Feeds the body to `on_chunk` while enforcing the response size limit.
async fn read_limited(
    response: reqwest::Response,
    mut on_chunk: impl FnMut(&[u8]) -> AppResult<()>,
) -> AppResult<()> {
    if response
        .content_length()
        .is_some_and(|size| size > MAX_RESPONSE_BYTES as u64)
    {
        return Err(too_large());
    }
    let mut total = 0usize;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| {
            if error.is_timeout() {
                AppError::msg("The AI provider stopped responding.")
            } else {
                AppError::msg("Could not read the AI provider response.")
            }
        })?;
        if chunk.len() > MAX_RESPONSE_BYTES - total {
            return Err(too_large());
        }
        total += chunk.len();
        on_chunk(&chunk)?;
    }
    Ok(())
}

impl Conversation {
    pub fn new(
        config: AiConfig,
        auth: RequestAuth,
        system: &str,
        messages: &[ChatMessage],
    ) -> AppResult<Self> {
        config.validate()?;
        let protocol = protocol(config.provider);
        let mut history = Vec::new();
        if protocol.system_in_history() {
            history.push(json!({"role": "system", "content": system}));
        }
        for message in messages {
            history.push(serde_json::to_value(message)?);
        }
        Ok(Self {
            config,
            auth,
            client: http_client()?,
            protocol,
            history,
            system: system.into(),
        })
    }

    fn request(&self, tools: &[Value]) -> Value {
        self.protocol
            .body(&self.config.model, &self.system, &self.history, tools)
    }

    pub async fn complete(
        &mut self,
        tools: &[Value],
        on_thinking: &impl Fn(String),
        on_text: &impl Fn(String),
    ) -> AppResult<Turn> {
        let request = self
            .client
            .post(endpoint(&self.config, self.protocol.path())?)
            .json(&self.request(tools));
        let response = apply_auth(request, &self.auth)
            .send()
            .await
            .map_err(|error| {
                if error.is_timeout() {
                    AppError::msg("The AI provider request timed out.")
                } else {
                    AppError::msg(
                        "Could not contact the AI provider. Check the URL and connection.",
                    )
                }
            })?;
        if !response.status().is_success() {
            return Err(AppError::msg(format!(
                "AI provider returned HTTP {}. Check the provider settings.",
                response.status().as_u16()
            )));
        }
        let sink = Callbacks {
            thinking: on_thinking,
            text: on_text,
        };
        let mut decoder = self.protocol.decoder();
        let mut pending = Vec::new();
        read_limited(response, |chunk| {
            pending.extend_from_slice(chunk);
            let mut start = 0;
            while let Some(offset) = pending[start..].iter().position(|b| *b == b'\n') {
                let end = start + offset + 1;
                decoder.line(&pending[start..end], &sink)?;
                start = end;
            }
            pending.drain(..start);
            Ok(())
        })
        .await?;
        if !pending.is_empty() {
            decoder.line(&pending, &sink)?;
        }
        let value = decoder.finish(&sink)?;
        self.accept(value)
    }

    fn accept(&mut self, value: Value) -> AppResult<Turn> {
        self.protocol.accept(value, &mut self.history)
    }

    pub fn tool_result(&mut self, call: &ToolCall, result: Value) -> AppResult<()> {
        let output = serde_json::to_string(&result)?;
        self.history.push(self.protocol.tool_result(call, output));
        Ok(())
    }
}

/// Everything that differs between the supported provider wire protocols.
trait Protocol: Sync {
    fn path(&self) -> &'static str;
    /// Whether the system prompt travels as the first history message.
    fn system_in_history(&self) -> bool;
    fn body(&self, model: &str, system: &str, history: &[Value], tools: &[Value]) -> Value;
    fn decoder(&self) -> Decoder;
    /// Validates a fully streamed response and appends it to `history` only if valid.
    fn accept(&self, value: Value, history: &mut Vec<Value>) -> AppResult<Turn>;
    fn tool_result(&self, call: &ToolCall, output: String) -> Value;
}

struct OllamaChat;
struct ChatCompletions;
struct Responses;

fn protocol(kind: ProviderKind) -> &'static dyn Protocol {
    match kind {
        ProviderKind::Ollama => &OllamaChat,
        ProviderKind::OpenaiCompatible => &ChatCompletions,
        ProviderKind::OpenaiResponses => &Responses,
    }
}

impl Protocol for OllamaChat {
    fn path(&self) -> &'static str {
        "api/chat"
    }

    fn system_in_history(&self) -> bool {
        true
    }

    fn body(&self, model: &str, _system: &str, history: &[Value], tools: &[Value]) -> Value {
        json!({"model": model, "messages": history, "tools": tools, "stream": true})
    }

    fn decoder(&self) -> Decoder {
        Decoder::Ollama(OllamaStream::default())
    }

    fn accept(&self, value: Value, history: &mut Vec<Value>) -> AppResult<Turn> {
        if value["done"] != true {
            return Err(incomplete());
        }
        if let Some(reason) = value["done_reason"].as_str()
            && !matches!(reason, "stop" | "tool_calls")
        {
            return Err(stopped_early());
        }
        let message = &value["message"];
        let turn = parse_chat_message(message, Some(history.len()))?;
        history.push(message.clone());
        Ok(turn)
    }

    fn tool_result(&self, call: &ToolCall, output: String) -> Value {
        json!({"role": "tool", "tool_name": call.name, "content": output})
    }
}

impl Protocol for ChatCompletions {
    fn path(&self) -> &'static str {
        "chat/completions"
    }

    fn system_in_history(&self) -> bool {
        true
    }

    fn body(&self, model: &str, _system: &str, history: &[Value], tools: &[Value]) -> Value {
        json!({"model": model, "messages": history, "tools": tools, "stream": true})
    }

    fn decoder(&self) -> Decoder {
        Decoder::Chat(Sse::default())
    }

    fn accept(&self, value: Value, history: &mut Vec<Value>) -> AppResult<Turn> {
        let choices = value["choices"].as_array().ok_or_else(malformed)?;
        if choices.len() != 1 {
            return Err(malformed());
        }
        let reason = choices[0]["finish_reason"].as_str();
        if !matches!(reason, Some("stop" | "tool_calls")) {
            return Err(stopped_early());
        }
        let message = &choices[0]["message"];
        let turn = parse_chat_message(message, None)?;
        if (reason == Some("tool_calls")) == turn.calls.is_empty() {
            return Err(malformed());
        }
        history.push(message.clone());
        Ok(turn)
    }

    fn tool_result(&self, call: &ToolCall, output: String) -> Value {
        json!({"role": "tool", "tool_call_id": call.id, "content": output})
    }
}

impl Protocol for Responses {
    fn path(&self) -> &'static str {
        "responses"
    }

    fn system_in_history(&self) -> bool {
        false
    }

    fn body(&self, model: &str, system: &str, history: &[Value], tools: &[Value]) -> Value {
        let tools: Vec<Value> = tools
            .iter()
            .map(|tool| {
                let mut function = tool["function"].clone();
                function["type"] = json!("function");
                function
            })
            .collect();
        json!({
            "model": model,
            "instructions": system,
            "input": history,
            "tools": tools,
            "store": false,
            "include": ["reasoning.encrypted_content"],
            "stream": true
        })
    }

    fn decoder(&self) -> Decoder {
        Decoder::Responses(Sse::default())
    }

    fn accept(&self, value: Value, history: &mut Vec<Value>) -> AppResult<Turn> {
        if value["status"] != "completed" {
            return Err(incomplete());
        }
        let output = value["output"].as_array().ok_or_else(malformed)?;
        let mut turn = Turn {
            text: String::new(),
            calls: Vec::new(),
        };
        for item in output {
            match required_string(item, "type")? {
                "message" => {
                    if item["status"] != "completed" || item["role"] != "assistant" {
                        return Err(malformed());
                    }
                    for part in item["content"].as_array().ok_or_else(malformed)? {
                        let field = match part["type"].as_str() {
                            Some("output_text") => "text",
                            Some("refusal") => "refusal",
                            _ => return Err(malformed()),
                        };
                        turn.text
                            .push_str(part[field].as_str().ok_or_else(malformed)?);
                    }
                }
                "function_call" => {
                    if item
                        .get("status")
                        .is_some_and(|status| status != "completed")
                    {
                        return Err(malformed());
                    }
                    turn.calls.push(ToolCall {
                        id: required_string(item, "call_id")?.into(),
                        name: required_string(item, "name")?.into(),
                        arguments: parse_arguments(&item["arguments"], true)?,
                    });
                }
                "reasoning" => {}
                _ => return Err(malformed()),
            }
        }
        validate_turn(&turn)?;
        // Stateless Responses requests must replay native reasoning and call items intact.
        history.extend(output.iter().cloned());
        Ok(turn)
    }

    fn tool_result(&self, call: &ToolCall, output: String) -> Value {
        json!({"type": "function_call_output", "call_id": call.id, "output": output})
    }
}

/// Parses an assistant chat message. Ollama sends no call IDs and native JSON
/// arguments; `ollama_round` (the history length) makes its generated IDs unique.
fn parse_chat_message(message: &Value, ollama_round: Option<usize>) -> AppResult<Turn> {
    if message["role"] != "assistant" {
        return Err(malformed());
    }
    let text = match &message["content"] {
        Value::String(text) => text.clone(),
        Value::Null => String::new(),
        _ => return Err(malformed()),
    };
    let mut turn = Turn {
        text,
        calls: Vec::new(),
    };
    if let Some(calls) = message.get("tool_calls").filter(|v| !v.is_null()) {
        for (index, call) in calls.as_array().ok_or_else(malformed)?.iter().enumerate() {
            let id = match ollama_round {
                Some(round) => format!("ollama-{round}-{index}"),
                None => {
                    if call["type"] != "function" {
                        return Err(malformed());
                    }
                    required_string(call, "id")?.into()
                }
            };
            turn.calls.push(ToolCall {
                id,
                name: required_string(&call["function"], "name")?.into(),
                arguments: parse_arguments(&call["function"]["arguments"], ollama_round.is_none())?,
            });
        }
    }
    validate_turn(&turn)?;
    Ok(turn)
}

/// Receives streamed reasoning and answer text as it arrives.
trait Sink {
    fn thinking(&self, text: &str);
    fn text(&self, text: &str);
}

struct Callbacks<'a, T, X> {
    thinking: &'a T,
    text: &'a X,
}

impl<T: Fn(String), X: Fn(String)> Sink for Callbacks<'_, T, X> {
    fn thinking(&self, text: &str) {
        (self.thinking)(text.to_string());
    }

    fn text(&self, text: &str) {
        if !text.is_empty() {
            (self.text)(text.to_string());
        }
    }
}

/// Incrementally decodes a streamed body into the provider's non-streamed response shape.
enum Decoder {
    Ollama(OllamaStream),
    Chat(Sse<ChatStream>),
    Responses(Sse<ResponsesStream>),
}

impl Decoder {
    /// One newline-terminated line, or the unterminated remainder at the end of the body.
    fn line(&mut self, line: &[u8], sink: &impl Sink) -> AppResult<()> {
        match self {
            Decoder::Ollama(decoder) => decoder.line(line, sink),
            Decoder::Chat(decoder) => decoder.line(line, sink),
            Decoder::Responses(decoder) => decoder.line(line, sink),
        }
    }

    fn finish(self, sink: &impl Sink) -> AppResult<Value> {
        match self {
            Decoder::Ollama(decoder) => Ok(decoder.finish()),
            Decoder::Chat(decoder) => decoder.finish(sink),
            Decoder::Responses(decoder) => decoder.finish(sink),
        }
    }
}

/// Ollama streams newline-delimited JSON objects.
#[derive(Default)]
struct OllamaStream {
    content: String,
    tool_calls: Option<Vec<Value>>,
    done: bool,
    done_reason: Option<Value>,
}

impl OllamaStream {
    fn line(&mut self, line: &[u8], sink: &impl Sink) -> AppResult<()> {
        let part: Value = serde_json::from_slice(line).map_err(|_| malformed())?;
        let message = &part["message"];
        if let Some(thinking) = message["thinking"].as_str() {
            sink.thinking(thinking);
        }
        if let Some(content) = message["content"].as_str() {
            sink.text(content);
            self.content.push_str(content);
        }
        if let Some(calls) = message.get("tool_calls") {
            self.tool_calls
                .get_or_insert_default()
                .extend(calls.as_array().ok_or_else(malformed)?.iter().cloned());
        }
        if part["done"] == true {
            self.done = true;
            if let Some(reason) = part.get("done_reason") {
                self.done_reason = Some(reason.clone());
            }
        }
        Ok(())
    }

    fn finish(self) -> Value {
        let mut message = json!({"role": "assistant", "content": self.content});
        if let Some(calls) = self.tool_calls {
            message["tool_calls"] = Value::Array(calls);
        }
        let mut value = json!({"message": message});
        if self.done {
            value["done"] = json!(true);
            if let Some(reason) = self.done_reason {
                value["done_reason"] = reason;
            }
        }
        value
    }
}

/// Handles the JSON payload of each server-sent event.
trait SseEvents: Default {
    fn event(&mut self, event: Value, sink: &impl Sink) -> AppResult<()>;
    fn finish(self) -> Value;
}

#[derive(Default)]
struct Sse<E> {
    data: String,
    events: E,
}

impl<E: SseEvents> Sse<E> {
    fn dispatch(&mut self, sink: &impl Sink) -> AppResult<()> {
        let data = std::mem::take(&mut self.data);
        if data == "[DONE]" {
            return Ok(());
        }
        let event = serde_json::from_str(&data).map_err(|_| malformed())?;
        self.events.event(event, sink)
    }

    fn line(&mut self, line: &[u8], sink: &impl Sink) -> AppResult<()> {
        let line = std::str::from_utf8(line)
            .map_err(|_| malformed())?
            .trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            if !self.data.is_empty() {
                self.dispatch(sink)?;
            }
        } else if let Some(value) = line.strip_prefix("data:") {
            if !self.data.is_empty() {
                self.data.push('\n');
            }
            self.data.push_str(value.strip_prefix(' ').unwrap_or(value));
        }
        Ok(())
    }

    fn finish(mut self, sink: &impl Sink) -> AppResult<Value> {
        if !self.data.is_empty() {
            self.dispatch(sink)?;
        }
        Ok(self.events.finish())
    }
}

struct PartialCall {
    id: String,
    kind: String,
    name: String,
    arguments: String,
}

impl Default for PartialCall {
    fn default() -> Self {
        Self {
            id: String::new(),
            kind: "function".into(),
            name: String::new(),
            arguments: String::new(),
        }
    }
}

/// Chat Completions deltas; tool call names and arguments arrive in fragments.
#[derive(Default)]
struct ChatStream {
    content: String,
    tool_calls: Vec<PartialCall>,
    finish_reason: Option<Value>,
}

impl SseEvents for ChatStream {
    fn event(&mut self, event: Value, sink: &impl Sink) -> AppResult<()> {
        if event.get("error").is_some() {
            return Err(AppError::msg(
                "The AI provider could not complete the request.",
            ));
        }
        let choices = event["choices"].as_array().ok_or_else(malformed)?;
        if choices.is_empty() {
            return Ok(());
        }
        if choices.len() != 1 || choices[0]["index"].as_u64().unwrap_or(0) != 0 {
            return Err(malformed());
        }
        let choice = &choices[0];
        let delta = &choice["delta"];
        if delta
            .get("role")
            .and_then(Value::as_str)
            .is_some_and(|role| role != "assistant")
        {
            return Err(malformed());
        }
        for field in ["content", "refusal"] {
            if let Some(text) = delta[field].as_str() {
                self.content.push_str(text);
                sink.text(text);
            }
        }
        if let Some(calls) = delta.get("tool_calls").filter(|v| !v.is_null()) {
            for call in calls.as_array().ok_or_else(malformed)? {
                let index = call["index"].as_u64().ok_or_else(malformed)? as usize;
                if index >= MAX_STREAMED_TOOL_CALLS {
                    return Err(malformed());
                }
                if self.tool_calls.len() <= index {
                    self.tool_calls.resize_with(index + 1, PartialCall::default);
                }
                let target = &mut self.tool_calls[index];
                if let Some(id) = call["id"].as_str() {
                    target.id = id.into();
                }
                if let Some(kind) = call["type"].as_str() {
                    target.kind = kind.into();
                }
                if let Some(name) = call["function"]["name"].as_str() {
                    target.name.push_str(name);
                }
                if let Some(arguments) = call["function"]["arguments"].as_str() {
                    target.arguments.push_str(arguments);
                }
            }
        }
        if !choice["finish_reason"].is_null() {
            self.finish_reason = Some(choice["finish_reason"].clone());
        }
        Ok(())
    }

    fn finish(self) -> Value {
        let mut message = json!({"role": "assistant", "content": self.content});
        if !self.tool_calls.is_empty() {
            message["tool_calls"] = self
                .tool_calls
                .into_iter()
                .map(|call| {
                    json!({"id": call.id, "type": call.kind,
                        "function": {"name": call.name, "arguments": call.arguments}})
                })
                .collect();
        }
        let mut choice = json!({"message": message});
        if let Some(reason) = self.finish_reason {
            choice["finish_reason"] = reason;
        }
        json!({"choices": [choice]})
    }
}

/// Responses events: text deltas are only shown live, the final response carries everything.
#[derive(Default)]
struct ResponsesStream {
    completed: Value,
}

impl SseEvents for ResponsesStream {
    fn event(&mut self, event: Value, sink: &impl Sink) -> AppResult<()> {
        match event["type"].as_str() {
            Some("response.output_text.delta" | "response.refusal.delta") => {
                sink.text(event["delta"].as_str().ok_or_else(malformed)?);
            }
            Some("response.completed") => {
                self.completed = event.get("response").ok_or_else(malformed)?.clone();
            }
            Some("response.failed" | "response.incomplete" | "error") => {
                return Err(incomplete());
            }
            _ => {}
        }
        Ok(())
    }

    fn finish(self) -> Value {
        self.completed
    }
}

pub async fn ollama_models(config: &AiConfig, auth: RequestAuth) -> AppResult<Vec<String>> {
    let request = http_client()?
        .get(endpoint(config, "api/tags")?)
        .timeout(MODELS_TIMEOUT);
    let response = apply_auth(request, &auth)
        .send()
        .await
        .map_err(|_| AppError::msg("Could not contact Ollama. Check the URL and connection."))?;
    if !response.status().is_success() {
        return Err(AppError::msg(format!(
            "Ollama returned HTTP {}.",
            response.status().as_u16()
        )));
    }
    let mut bytes = Vec::new();
    read_limited(response, |chunk| {
        bytes.extend_from_slice(chunk);
        Ok(())
    })
    .await?;
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| malformed())?;
    value["models"]
        .as_array()
        .ok_or_else(malformed)?
        .iter()
        .map(|model| Ok(required_string(model, "name")?.to_string()))
        .collect()
}

fn parse_arguments(value: &Value, string: bool) -> AppResult<Value> {
    let arguments = if string {
        serde_json::from_str(value.as_str().ok_or_else(malformed)?).map_err(|_| malformed())?
    } else {
        value.clone()
    };
    if !arguments.is_object() {
        return Err(malformed());
    }
    Ok(arguments)
}

fn validate_turn(turn: &Turn) -> AppResult<()> {
    if turn.text.trim().is_empty() && turn.calls.is_empty() {
        return Err(malformed());
    }
    for (index, call) in turn.calls.iter().enumerate() {
        if turn.calls[..index].iter().any(|other| other.id == call.id) {
            return Err(malformed());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn conversation(provider: ProviderKind) -> Conversation {
        Conversation::new(
            AiConfig {
                provider,
                model: "test-model".into(),
                ..Default::default()
            },
            RequestAuth::None,
            "system instruction",
            &[ChatMessage {
                role: Role::User,
                content: "question".into(),
            }],
        )
        .unwrap()
    }

    fn tools() -> Vec<Value> {
        vec![
            json!({"type": "function", "function": {"name": "query", "description": "Read rows", "parameters": {"type": "object", "properties": {}}}}),
        ]
    }

    fn decode(provider: ProviderKind, body: &str, sink: &impl Sink) -> AppResult<Value> {
        let mut decoder = protocol(provider).decoder();
        for line in body.split_inclusive('\n') {
            decoder.line(line.as_bytes(), sink)?;
        }
        decoder.finish(sink)
    }

    fn sse(events: &[Value]) -> String {
        events
            .iter()
            .map(|event| format!("data: {event}\n\n"))
            .collect()
    }

    #[test]
    fn ollama_stream_combines_thinking_content_and_tool_calls() {
        let thinking = std::cell::RefCell::new(String::new());
        let sink = Callbacks {
            thinking: &|text: String| thinking.borrow_mut().push_str(&text),
            text: &|_| {},
        };
        let body = format!(
            "{}\n{}\n",
            json!({"message": {"thinking": "first", "content": "hello "}}),
            json!({"message": {"thinking": " second", "content": "world", "tool_calls": [{"function": {"name": "query", "arguments": {}}}]}, "done": true})
        );
        let result = decode(ProviderKind::Ollama, &body, &sink).unwrap();
        assert_eq!(*thinking.borrow(), "first second");
        let mut conversation = conversation(ProviderKind::Ollama);
        let turn = conversation.accept(result).unwrap();
        assert_eq!(turn.text, "hello world");
        assert_eq!(turn.calls.len(), 1);
    }

    #[test]
    fn sse_streams_text_and_reassembles_fragmented_tool_calls() {
        let chunks = std::cell::RefCell::new(Vec::new());
        let on_text = |text: String| chunks.borrow_mut().push(text);
        let sink = Callbacks {
            thinking: &|_| {},
            text: &on_text,
        };
        let compatible = decode(
            ProviderKind::OpenaiCompatible,
            &sse(&[
                json!({"choices":[{"index":0,"delta":{"content":"Hi "}}]}),
                json!({"choices":[{"index":0,"delta":{"content":"there", "tool_calls":[{"index":0,"id":"call1","type":"function","function":{"name":"que","arguments":"{\"sql\":"}}]}}]}),
                json!({"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"name":"ry","arguments":"\"select 1\"}"}}]},"finish_reason":"tool_calls"}]}),
            ]),
            &sink,
        )
        .unwrap();
        let mut chat = conversation(ProviderKind::OpenaiCompatible);
        let turn = chat.accept(compatible).unwrap();
        assert_eq!(*chunks.borrow(), vec!["Hi ", "there"]);
        assert_eq!(turn.text, "Hi there");
        assert_eq!(turn.calls[0].name, "query");
        assert_eq!(turn.calls[0].arguments["sql"], "select 1");

        let output = json!([{"type":"reasoning", "id":"r1", "encrypted_content":"secret"}, {"type":"function_call", "call_id":"c1", "name":"query", "arguments":"{}", "status":"completed"}]);
        let responses = decode(
            ProviderKind::OpenaiResponses,
            &sse(&[
                json!({"type":"response.output_text.delta","delta":"live"}),
                json!({"type":"response.completed","response":{"status":"completed", "output":output}}),
            ]),
            &sink,
        )
        .unwrap();
        let mut conversation = conversation(ProviderKind::OpenaiResponses);
        let turn = conversation.accept(responses).unwrap();
        assert_eq!(turn.calls.len(), 1);
        assert_eq!(conversation.history.last().unwrap()["call_id"], "c1");
        assert_eq!(chunks.borrow().last().unwrap(), "live");
        assert!(
            decode(
                ProviderKind::OpenaiResponses,
                &sse(&[json!({"type":"response.failed"})]),
                &sink
            )
            .is_err()
        );
    }

    #[test]
    fn streamed_text_accumulates_without_quadratic_copies() {
        let sink = Callbacks {
            thinking: &|_| {},
            text: &|_| {},
        };
        let pieces: Vec<Value> = (0..2000)
            .map(|i| json!({"choices":[{"index":0,"delta":{"content":format!("{i},")}}]}))
            .chain(std::iter::once(
                json!({"choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}),
            ))
            .collect();
        let value = decode(ProviderKind::OpenaiCompatible, &sse(&pieces), &sink).unwrap();
        let expected: String = (0..2000).map(|i| format!("{i},")).collect();
        assert_eq!(value["choices"][0]["message"]["content"], expected);
        assert!(value["choices"][0]["message"].get("tool_calls").is_none());
    }

    #[tokio::test]
    async fn streams_each_provider_before_the_response_finishes() {
        use tokio::sync::oneshot;
        for provider in [
            ProviderKind::Ollama,
            ProviderKind::OpenaiCompatible,
            ProviderKind::OpenaiResponses,
        ] {
            let (first, last) = match provider {
                ProviderKind::Ollama => (
                    json!({"message":{"role":"assistant","content":"Hällo"},"done":false})
                        .to_string()
                        + "\n",
                    json!({"message":{"role":"assistant","content":"!"},"done":true}).to_string()
                        + "\n",
                ),
                ProviderKind::OpenaiCompatible => (
                    format!(
                        "data: {}\r\n\r\n",
                        json!({"choices":[{"index":0,"delta":{"content":"Hällo"},"finish_reason":null}]})
                    ),
                    format!(
                        "data: {}\r\n\r\ndata: [DONE]\r\n\r\n",
                        json!({"choices":[{"index":0,"delta":{"content":"!"},"finish_reason":"stop"}]})
                    ),
                ),
                ProviderKind::OpenaiResponses => (
                    format!(
                        "data: {}\n\n",
                        json!({"type":"response.output_text.delta","delta":"Hällo"})
                    ),
                    format!(
                        "data: {}\n\ndata: {}\n\n",
                        json!({"type":"response.output_text.delta","delta":"!"}),
                        json!({"type":"response.completed","response":{"status":"completed","output":[{"type":"message","role":"assistant","status":"completed","content":[{"type":"output_text","text":"Hällo!"}]}]}})
                    ),
                ),
            };
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let (proceed, wait) = oneshot::channel();
            let server = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = [0; 8192];
                let count = socket.read(&mut request).await.unwrap();
                assert!(count > 0);
                let header = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    first.len() + last.len()
                );
                socket.write_all(header.as_bytes()).await.unwrap();
                let split = first.find('ä').unwrap() + 1;
                socket.write_all(&first.as_bytes()[..split]).await.unwrap();
                socket.write_all(&first.as_bytes()[split..]).await.unwrap();
                wait.await.unwrap();
                socket.write_all(last.as_bytes()).await.unwrap();
            });
            let (seen, received) = oneshot::channel();
            let seen = std::cell::RefCell::new(Some(seen));
            let mut chat = conversation(provider);
            chat.config.base_url = format!("http://{address}");
            let task = async {
                let result = chat
                    .complete(&tools(), &|_| {}, &|text| {
                        if text == "Hällo"
                            && let Some(sender) = seen.borrow_mut().take()
                        {
                            let _ = sender.send(());
                        }
                    })
                    .await
                    .unwrap();
                assert_eq!(result.text, "Hällo!");
            };
            let release = async {
                tokio::time::timeout(Duration::from_secs(2), received)
                    .await
                    .unwrap()
                    .unwrap();
                proceed.send(()).unwrap();
            };
            tokio::join!(task, release);
            server.await.unwrap();
        }
    }

    #[test]
    fn responses_replays_native_reasoning_and_call_ids() {
        let mut conversation = conversation(ProviderKind::OpenaiResponses);
        let output = json!([
            {"id": "r1", "type": "reasoning", "summary": [], "encrypted_content": "encrypted"},
            {"id": "f1", "type": "function_call", "call_id": "call1", "name": "query", "arguments": "{}", "status": "completed"}
        ]);
        let turn = conversation
            .accept(json!({"status": "completed", "output": output}))
            .unwrap();
        conversation
            .tool_result(&turn.calls[0], json!({"rows": []}))
            .unwrap();
        let request = conversation.request(&tools());
        assert_eq!(request["input"][1], output[0]);
        assert_eq!(request["input"][2], output[1]);
        assert_eq!(request["input"][3]["call_id"], "call1");
        assert_eq!(request["tools"][0]["name"], "query");
        assert_eq!(request["store"], false);
        assert_eq!(request["include"][0], "reasoning.encrypted_content");
    }

    #[test]
    fn native_and_compatible_tool_arguments_have_correct_shapes() {
        let mut ollama = conversation(ProviderKind::Ollama);
        let turn = ollama.accept(json!({"done": true, "done_reason": "stop", "message": {"role": "assistant", "content": "", "tool_calls": [{"function": {"name": "query", "arguments": {"sql": "select 1"}}}]}})).unwrap();
        ollama
            .tool_result(&turn.calls[0], json!({"rows": []}))
            .unwrap();
        assert_eq!(ollama.history.last().unwrap()["tool_name"], "query");
        let mut compatible = conversation(ProviderKind::OpenaiCompatible);
        let turn = compatible.accept(json!({"choices": [{"finish_reason": "tool_calls", "message": {"role": "assistant", "content": null, "tool_calls": [{"id": "c1", "type": "function", "function": {"name": "query", "arguments": "{\"sql\":\"select 1\"}"}}]}}]})).unwrap();
        assert_eq!(turn.calls[0].arguments["sql"], "select 1");
        compatible
            .tool_result(&turn.calls[0], json!({"rows": []}))
            .unwrap();
        assert_eq!(compatible.history.last().unwrap()["tool_call_id"], "c1");
    }

    #[test]
    fn rejects_empty_truncated_and_malformed_output_without_mutating_history() {
        let mut compatible = conversation(ProviderKind::OpenaiCompatible);
        for response in [
            json!({"choices": []}),
            json!({"choices": [{"finish_reason": "length", "message": {"role": "assistant", "content": "partial"}}]}),
            json!({"choices": [{"finish_reason": "stop", "message": {"role": "assistant", "content": ""}}]}),
            json!({"choices": [{"finish_reason": "tool_calls", "message": {"role": "assistant", "content": null, "tool_calls": [{"id": "c1", "type": "function", "function": {"name": "query", "arguments": "bad-json"}}]}}]}),
        ] {
            assert!(compatible.accept(response).is_err());
            assert_eq!(compatible.history.len(), 2);
        }
        let mut responses = conversation(ProviderKind::OpenaiResponses);
        assert!(
            responses
                .accept(json!({"status": "incomplete", "output": []}))
                .is_err()
        );
    }

    async fn mock_response(
        status: &str,
        body: String,
    ) -> (String, tokio::task::JoinHandle<String>) {
        mock_http(format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )).await
    }

    async fn mock_http(response: String) -> (String, tokio::task::JoinHandle<String>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut buffer = [0; 8192];
            loop {
                let count = stream.read(&mut buffer).await.unwrap();
                assert!(count > 0);
                request.extend_from_slice(&buffer[..count]);
                if let Some(end) = request.windows(4).position(|part| part == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&request[..end]);
                    let length = headers
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length: ")
                                .and_then(|n| n.parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if request.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            let _ = stream.write_all(response.as_bytes()).await;
            String::from_utf8(request).unwrap()
        });
        (format!("http://{address}"), task)
    }

    #[tokio::test]
    async fn ollama_models_uses_tags_endpoint_and_saved_auth() {
        let (url, server) = mock_response(
            "200 OK",
            json!({"models": [{"name": "one:latest"}, {"name": "two"}]}).to_string(),
        )
        .await;
        let config = AiConfig {
            base_url: url,
            ..Default::default()
        };
        assert_eq!(
            ollama_models(&config, RequestAuth::Bearer("test-key".into()))
                .await
                .unwrap(),
            vec!["one:latest", "two"]
        );
        let request = server.await.unwrap();
        assert!(request.starts_with("GET /api/tags HTTP/1.1"));
        assert!(
            request
                .to_ascii_lowercase()
                .contains("authorization: bearer test-key")
        );
    }

    #[tokio::test]
    async fn ollama_models_sends_basic_auth() {
        let (url, server) = mock_response("200 OK", json!({"models": []}).to_string()).await;
        let config = AiConfig {
            base_url: url,
            ..Default::default()
        };
        let auth = RequestAuth::Basic(super::super::settings::BasicAuth {
            username: "user".into(),
            password: "pass".into(),
        });
        assert!(ollama_models(&config, auth).await.unwrap().is_empty());
        let request = server.await.unwrap();
        assert!(
            request
                .to_ascii_lowercase()
                .contains("authorization: basic dxnlcjpwyxnz")
        );
    }

    #[tokio::test]
    async fn sends_native_endpoints_and_protocol_bodies() {
        for (provider, path, body) in [
            (
                ProviderKind::Ollama,
                "/api/chat",
                json!({"done": true, "message": {"role": "assistant", "content": "answer"}}),
            ),
            (
                ProviderKind::OpenaiCompatible,
                "/v1/chat/completions",
                json!({"choices": [{"index":0, "finish_reason": "stop", "delta": {"content": "answer"}}]}),
            ),
            (
                ProviderKind::OpenaiResponses,
                "/v1/responses",
                json!({"type":"response.completed", "response":{"status": "completed", "output": [{"type": "message", "role": "assistant", "status": "completed", "content": [{"type": "output_text", "text": "answer"}]}]}}),
            ),
        ] {
            let payload = if provider == ProviderKind::Ollama {
                body.to_string()
            } else {
                format!("data: {body}\n\n")
            };
            let (url, server) = mock_response("200 OK", payload).await;
            let mut conversation = conversation(provider);
            conversation.config.base_url = if provider == ProviderKind::Ollama {
                url
            } else {
                format!("{url}/v1")
            };
            conversation.auth = RequestAuth::Bearer("test-key".into());
            assert_eq!(
                conversation
                    .complete(&tools(), &|_| {}, &|_| {})
                    .await
                    .unwrap()
                    .text,
                "answer"
            );
            let request = server.await.unwrap();
            assert!(request.starts_with(&format!("POST {path} HTTP/1.1")));
            assert!(
                request
                    .to_ascii_lowercase()
                    .contains("authorization: bearer test-key")
            );
            let body: Value =
                serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap()).unwrap();
            assert_eq!(body["stream"], true);
            assert_eq!(body["model"], "test-model");
        }
    }

    #[tokio::test]
    async fn basic_auth_sends_one_header_without_credentials_in_body() {
        for provider in [
            ProviderKind::Ollama,
            ProviderKind::OpenaiResponses,
            ProviderKind::OpenaiCompatible,
        ] {
            let (url, server) = mock_response("401 Unauthorized", "".into()).await;
            let mut conversation = conversation(provider);
            conversation.config.base_url = url;
            conversation.auth = RequestAuth::Basic(super::super::settings::BasicAuth {
                username: "user".into(),
                password: "pass".into(),
            });
            assert!(conversation.complete(&[], &|_| {}, &|_| {}).await.is_err());
            let request = server.await.unwrap().to_ascii_lowercase();
            let (headers, body) = request.split_once("\r\n\r\n").unwrap();
            assert_eq!(headers.matches("authorization:").count(), 1);
            assert!(headers.contains("authorization: basic dxnlcjpwyxnz"));
            assert!(!body.contains("password"));
            assert!(!body.contains("dxnlcjpwyxnz"));
        }
        let (url, server) = mock_response("401 Unauthorized", "".into()).await;
        let mut conversation = conversation(ProviderKind::Ollama);
        conversation.config.base_url = url;
        assert!(conversation.complete(&[], &|_| {}, &|_| {}).await.is_err());
        assert!(
            !server
                .await
                .unwrap()
                .to_ascii_lowercase()
                .contains("authorization:")
        );
    }

    #[tokio::test]
    async fn http_errors_do_not_expose_response_body_or_follow_redirects() {
        let (url, server) = mock_response(
            "302 Found\r\nLocation: http://127.0.0.1:9/redirect",
            "private-provider-error".into(),
        )
        .await;
        let mut conversation = conversation(ProviderKind::Ollama);
        conversation.config.base_url = url;
        let error = conversation
            .complete(&[], &|_| {}, &|_| {})
            .await
            .err()
            .unwrap()
            .to_string();
        assert!(error.contains("302"));
        assert!(!error.contains("private-provider-error"));
        server.await.unwrap();
    }

    #[tokio::test]
    async fn rejects_oversized_responses_with_and_without_content_length() {
        for response in [
            format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                MAX_RESPONSE_BYTES + 1
            ),
            format!(
                "HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n{}",
                "x".repeat(MAX_RESPONSE_BYTES + 1)
            ),
        ] {
            let (url, server) = mock_http(response).await;
            let mut conversation = conversation(ProviderKind::Ollama);
            conversation.config.base_url = url;
            let error = conversation
                .complete(&[], &|_| {}, &|_| {})
                .await
                .err()
                .unwrap()
                .to_string();
            assert!(error.contains("size limit"));
            server.await.unwrap();
        }
    }

    async fn paced_server(
        delays: Vec<Duration>,
        body_parts: Vec<String>,
    ) -> (String, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0; 8192];
            let _ = socket.read(&mut request).await.unwrap();
            let length: usize = body_parts.iter().map(String::len).sum();
            let header =
                format!("HTTP/1.1 200 OK\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n");
            let _ = socket.write_all(header.as_bytes()).await;
            for (delay, part) in delays.into_iter().zip(body_parts) {
                tokio::time::sleep(delay).await;
                if socket.write_all(part.as_bytes()).await.is_err() {
                    return;
                }
            }
        });
        (format!("http://{address}"), task)
    }

    #[tokio::test]
    async fn read_timeout_bounds_silence_not_total_generation_time() {
        let parts: Vec<String> = (0..6)
            .map(|i| {
                json!({"message":{"role":"assistant","content":"x"},"done":i == 5}).to_string()
                    + "\n"
            })
            .collect();
        let read_timeout = Duration::from_millis(400);
        // Six chunks spread over ~1.2 s finish although each gap stays below the timeout.
        let (url, server) = paced_server(vec![Duration::from_millis(200); 6], parts.clone()).await;
        let mut chat = conversation(ProviderKind::Ollama);
        chat.client = client_with_read_timeout(read_timeout).unwrap();
        chat.config.base_url = url;
        let turn = chat.complete(&[], &|_| {}, &|_| {}).await.unwrap();
        assert_eq!(turn.text, "xxxxxx");
        server.await.unwrap();

        let mut delays = vec![Duration::ZERO; 6];
        delays[1] = Duration::from_millis(1500);
        let (url, server) = paced_server(delays, parts).await;
        let mut chat = conversation(ProviderKind::Ollama);
        chat.client = client_with_read_timeout(read_timeout).unwrap();
        chat.config.base_url = url;
        let error = chat
            .complete(&[], &|_| {}, &|_| {})
            .await
            .err()
            .unwrap()
            .to_string();
        assert!(error.contains("stopped responding"), "{error}");
        server.abort();
    }

    #[test]
    fn chat_roles_keep_the_frontend_wire_format() {
        let message: ChatMessage =
            serde_json::from_value(json!({"role": "assistant", "content": "hi"})).unwrap();
        assert_eq!(message.role, Role::Assistant);
        assert_eq!(
            serde_json::to_value(&message).unwrap(),
            json!({"role": "assistant", "content": "hi"})
        );
        assert!(
            serde_json::from_value::<ChatMessage>(json!({"role": "system", "content": "x"}))
                .is_err()
        );
    }

    #[tokio::test]
    async fn refuses_plain_http_to_remote_hosts_before_sending_credentials() {
        let mut chat = conversation(ProviderKind::OpenaiCompatible);
        chat.config.base_url = "http://ai.example.com/v1".into();
        chat.auth = RequestAuth::Bearer("test-key".into());
        let error = chat
            .complete(&[], &|_| {}, &|_| {})
            .await
            .err()
            .unwrap()
            .to_string();
        assert!(error.contains("HTTPS"));
        let config = AiConfig {
            base_url: "http://ai.example.com".into(),
            ..Default::default()
        };
        assert!(ollama_models(&config, RequestAuth::None).await.is_err());
    }
}
