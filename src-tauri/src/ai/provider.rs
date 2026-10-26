use std::time::Duration;

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::settings::{AiConfig, ProviderKind, RequestAuth};
use crate::error::{AppError, AppResult};

const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
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
    history: Vec<Value>,
    system: String,
}

fn malformed() -> AppError {
    AppError::msg("The AI provider returned an invalid or unsupported response.")
}

fn required_string<'a>(value: &'a Value, key: &str) -> AppResult<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(malformed)
}

impl Conversation {
    pub fn new(
        config: AiConfig,
        auth: RequestAuth,
        system: &str,
        messages: &[ChatMessage],
    ) -> AppResult<Self> {
        config.validate()?;
        let _ = rustls::crypto::ring::default_provider().install_default();
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|_| AppError::msg("Could not initialize the AI HTTP client."))?;
        let mut history = Vec::new();
        if config.provider != ProviderKind::OpenaiResponses {
            history.push(json!({"role": "system", "content": system}));
        }
        for message in messages {
            if !matches!(message.role.as_str(), "user" | "assistant") {
                return Err(AppError::msg("Invalid AI chat message role."));
            }
            history.push(serde_json::to_value(message)?);
        }
        Ok(Self {
            config,
            auth,
            client,
            history,
            system: system.into(),
        })
    }

    fn request(&self, tools: &[Value]) -> Value {
        match self.config.provider {
            ProviderKind::OpenaiResponses => {
                let tools: Vec<Value> = tools
                    .iter()
                    .map(|tool| {
                        let mut function = tool["function"].clone();
                        function["type"] = json!("function");
                        function
                    })
                    .collect();
                json!({
                    "model": self.config.model,
                    "instructions": self.system,
                    "input": self.history,
                    "tools": tools,
                    "store": false,
                    "include": ["reasoning.encrypted_content"],
                    "stream": false
                })
            }
            ProviderKind::Ollama => json!({
                "model": self.config.model, "messages": self.history,
                "tools": tools, "stream": false
            }),
            ProviderKind::OpenaiCompatible => json!({
                "model": self.config.model, "messages": self.history,
                "tools": tools, "stream": false
            }),
        }
    }

    pub async fn complete(&mut self, tools: &[Value]) -> AppResult<Turn> {
        let suffix = match self.config.provider {
            ProviderKind::Ollama => "api/chat",
            ProviderKind::OpenaiResponses => "responses",
            ProviderKind::OpenaiCompatible => "chat/completions",
        };
        let endpoint = format!(
            "{}/{suffix}",
            self.config.url()?.as_str().trim_end_matches('/')
        );
        let mut request = self.client.post(endpoint).json(&self.request(tools));
        request = match &self.auth {
            RequestAuth::None => request,
            RequestAuth::Bearer(key) => request.bearer_auth(key),
            RequestAuth::Basic(basic) => request.basic_auth(&basic.username, Some(&basic.password)),
        };
        let response = request.send().await.map_err(|error| {
            if error.is_timeout() {
                AppError::msg("The AI provider request timed out.")
            } else {
                AppError::msg("Could not contact the AI provider. Check the URL and connection.")
            }
        })?;
        if !response.status().is_success() {
            return Err(AppError::msg(format!(
                "AI provider returned HTTP {}. Check the provider settings.",
                response.status().as_u16()
            )));
        }
        if response
            .content_length()
            .is_some_and(|size| size > MAX_RESPONSE_BYTES as u64)
        {
            return Err(AppError::msg(
                "The AI provider response exceeded the size limit.",
            ));
        }
        let mut bytes = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk =
                chunk.map_err(|_| AppError::msg("Could not read the AI provider response."))?;
            if chunk.len() > MAX_RESPONSE_BYTES - bytes.len() {
                return Err(AppError::msg(
                    "The AI provider response exceeded the size limit.",
                ));
            }
            bytes.extend_from_slice(&chunk);
        }
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| malformed())?;
        self.accept(value)
    }

    fn accept(&mut self, value: Value) -> AppResult<Turn> {
        match self.config.provider {
            ProviderKind::OpenaiResponses => self.accept_responses(value),
            ProviderKind::Ollama | ProviderKind::OpenaiCompatible => self.accept_chat(value),
        }
    }

    fn accept_responses(&mut self, value: Value) -> AppResult<Turn> {
        if value["status"] != "completed" {
            return Err(AppError::msg(
                "The AI provider did not complete its response. Try again.",
            ));
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
                        match part["type"].as_str() {
                            Some("output_text") => turn
                                .text
                                .push_str(part["text"].as_str().ok_or_else(malformed)?),
                            Some("refusal") => turn
                                .text
                                .push_str(part["refusal"].as_str().ok_or_else(malformed)?),
                            _ => return Err(malformed()),
                        }
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
        self.history.extend(output.iter().cloned());
        Ok(turn)
    }

    fn accept_chat(&mut self, value: Value) -> AppResult<Turn> {
        let ollama = self.config.provider == ProviderKind::Ollama;
        let message = if ollama {
            if value["done"] != true {
                return Err(AppError::msg(
                    "The AI provider did not complete its response. Try again.",
                ));
            }
            if let Some(reason) = value["done_reason"].as_str()
                && !matches!(reason, "stop" | "tool_calls")
            {
                return Err(AppError::msg(
                    "The AI provider stopped before completing its response.",
                ));
            }
            &value["message"]
        } else {
            let choices = value["choices"].as_array().ok_or_else(malformed)?;
            if choices.len() != 1 {
                return Err(malformed());
            }
            if !matches!(
                choices[0]["finish_reason"].as_str(),
                Some("stop" | "tool_calls")
            ) {
                return Err(AppError::msg(
                    "The AI provider stopped before completing its response.",
                ));
            }
            &choices[0]["message"]
        };
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
                if !ollama && call["type"] != "function" {
                    return Err(malformed());
                }
                turn.calls.push(ToolCall {
                    id: if ollama {
                        format!("ollama-{}-{index}", self.history.len())
                    } else {
                        required_string(call, "id")?.into()
                    },
                    name: required_string(&call["function"], "name")?.into(),
                    arguments: parse_arguments(&call["function"]["arguments"], !ollama)?,
                });
            }
        }
        validate_turn(&turn)?;
        if !ollama {
            let reason = &value["choices"][0]["finish_reason"];
            if (reason == "tool_calls") != !turn.calls.is_empty() {
                return Err(malformed());
            }
        }
        self.history.push(message.clone());
        Ok(turn)
    }

    pub fn tool_result(&mut self, call: &ToolCall, result: Value) -> AppResult<()> {
        let output = serde_json::to_string(&result)?;
        self.history.push(match self.config.provider {
            ProviderKind::OpenaiResponses => {
                json!({"type": "function_call_output", "call_id": call.id, "output": output})
            }
            ProviderKind::Ollama => {
                json!({"role": "tool", "tool_name": call.name, "content": output})
            }
            ProviderKind::OpenaiCompatible => {
                json!({"role": "tool", "tool_call_id": call.id, "content": output})
            }
        });
        Ok(())
    }
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
                role: "user".into(),
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
                json!({"choices": [{"finish_reason": "stop", "message": {"role": "assistant", "content": "answer"}}]}),
            ),
            (
                ProviderKind::OpenaiResponses,
                "/v1/responses",
                json!({"status": "completed", "output": [{"type": "message", "role": "assistant", "status": "completed", "content": [{"type": "output_text", "text": "answer"}]}]}),
            ),
        ] {
            let (url, server) = mock_response("200 OK", body.to_string()).await;
            let mut conversation = conversation(provider);
            conversation.config.base_url = if provider == ProviderKind::Ollama {
                url
            } else {
                format!("{url}/v1")
            };
            conversation.auth = RequestAuth::Bearer("test-key".into());
            assert_eq!(
                conversation.complete(&tools()).await.unwrap().text,
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
            assert_eq!(body["stream"], false);
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
            assert!(conversation.complete(&[]).await.is_err());
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
        assert!(conversation.complete(&[]).await.is_err());
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
        let error = conversation.complete(&[]).await.err().unwrap().to_string();
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
            let error = conversation.complete(&[]).await.err().unwrap().to_string();
            assert!(error.contains("size limit"));
            server.await.unwrap();
        }
    }
}
