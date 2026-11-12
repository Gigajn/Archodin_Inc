//! A Gemini LLM for langchain-rust's [`LLM`] trait: generateContent chat, function
//! calling and streaming. [`Provider::gemini`](crate::Provider::gemini) puts it in a
//! Model chain.

use std::pin::Pin;

use async_trait::async_trait;
use futures::{Stream, StreamExt};
use langchain_rust::language_models::llm::LLM;
use langchain_rust::language_models::options::CallOptions;
use langchain_rust::language_models::{
    GenerateResult, LLMError, LLMEvent, LLMEventStream, TokenUsage,
};
use langchain_rust::schemas::{Message, MessageType, StreamData, ToolCall};
use serde_json::{json, Map, Value};

use crate::http_guard::{
    read_capped, shared_client, text_capped, validate_endpoint, MAX_RESPONSE_BYTES,
};

const DEFAULT_BASE_URL: &str = "https://generativelanguage.googleapis.com";

/// Google's Gemini models through the Generative Language API.
#[derive(Clone)]
pub struct Gemini {
    base_url: String,
    api_key: String,
    model: String,
    options: CallOptions,
}

impl Gemini {
    /// A Gemini LLM for `model` (for example `gemini-2.5-flash`).
    pub fn new(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            base_url: DEFAULT_BASE_URL.into(),
            api_key: api_key.into(),
            model: model.into(),
            options: CallOptions::default(),
        }
    }

    /// Overrides the API root, for a proxy or a fake server.
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into().trim_end_matches('/').to_string();
        self
    }

    /// Sets the call options (temperature, token limit, Tools, ...).
    pub fn with_options(mut self, options: CallOptions) -> Self {
        self.options = options;
        self
    }

    /// The request URL for `method`. The model name is interpolated into the
    /// path, so it may only contain the characters a model name needs —
    /// anything else (`/`, `?`, `#`, …) is refused instead of letting the
    /// name escape into another path or query. The base URL is operator
    /// configuration (a proxy or a fake server), so it goes through the
    /// operator check, not the strict one.
    fn url(&self, method: &str) -> Result<String, LLMError> {
        if self.model.is_empty()
            || !self
                .model
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        {
            return Err(LLMError::OtherError(format!(
                "Gemini model name {:?} has characters a model name cannot have",
                self.model
            )));
        }
        let url = format!("{}/v1beta/models/{}:{method}", self.base_url, self.model);
        validate_endpoint(&url)
            .map_err(|e| LLMError::OtherError(format!("Gemini URL refused: {e}")))?;
        Ok(url)
    }

    /// Sends one request with the shared client (KR-01): 30 s total timeout,
    /// at most 5 redirects to allowed targets, and the error body is read
    /// capped at 2 MiB instead of unbounded.
    async fn send(&self, url: &str, messages: &[Message]) -> Result<reqwest::Response, LLMError> {
        validate_endpoint(url)
            .map_err(|e| LLMError::OtherError(format!("Gemini URL refused: {e}")))?;
        let response = shared_client()
            .map_err(|e| LLMError::OtherError(format!("could not build HTTP client: {e}")))?
            .post(url)
            .header("x-goog-api-key", &self.api_key)
            .json(&self.payload(messages))
            .send()
            .await?;
        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }
        let body = read_capped(response)
            .await
            .map_err(|e| LLMError::OtherError(format!("Gemini error body: {e}")))?;
        let body = text_capped(body);
        let message = serde_json::from_str::<Value>(&body)
            .ok()
            .and_then(|v| v["error"]["message"].as_str().map(str::to_string))
            .unwrap_or(body);
        Err(LLMError::OtherError(format!(
            "Gemini error {}: {message}",
            status.as_u16()
        )))
    }

    fn payload(&self, messages: &[Message]) -> Value {
        let mut payload = Map::new();
        let system: Vec<&str> = messages
            .iter()
            .filter(|m| m.message_type == MessageType::SystemMessage)
            .map(|m| m.content.as_str())
            .collect();
        if !system.is_empty() {
            payload.insert(
                "systemInstruction".into(),
                json!({"parts": [{"text": system.join("\n\n")}]}),
            );
        }
        payload.insert("contents".into(), Value::Array(contents(messages)));
        if let Some(functions) = self.options.functions.as_ref().filter(|f| !f.is_empty()) {
            let declarations: Vec<Value> = functions
                .iter()
                .map(|f| {
                    json!({"name": f.name, "description": f.description,
                        "parameters": gemini_schema(f.parameters.clone())})
                })
                .collect();
            payload.insert(
                "tools".into(),
                json!([{"functionDeclarations": declarations}]),
            );
        }
        let mut config = Map::new();
        let o = &self.options;
        if let Some(v) = o.temperature {
            config.insert("temperature".into(), json!(v));
        }
        if let Some(v) = o.max_tokens {
            config.insert("maxOutputTokens".into(), json!(v));
        }
        if let Some(v) = o.top_p {
            config.insert("topP".into(), json!(v));
        }
        if let Some(v) = o.top_k {
            config.insert("topK".into(), json!(v));
        }
        if let Some(v) = o.stop_words.as_ref().filter(|w| !w.is_empty()) {
            config.insert("stopSequences".into(), json!(v));
        }
        if !config.is_empty() {
            payload.insert("generationConfig".into(), Value::Object(config));
        }
        Value::Object(payload)
    }
}

/// The conversation as Gemini `contents`: roles are `user` and `model`, and the
/// results of one step's Tool calls travel together in one `user` entry.
fn contents(messages: &[Message]) -> Vec<Value> {
    let mut names = std::collections::HashMap::new();
    let mut out: Vec<Value> = Vec::new();
    for message in messages {
        match message.message_type {
            MessageType::SystemMessage => {}
            MessageType::HumanMessage => {
                let mut parts = Vec::new();
                if !message.content.is_empty() {
                    parts.push(json!({"text": message.content}));
                }
                for image in message.images.iter().flatten() {
                    parts.push(image_part(&image.image_url));
                }
                if !parts.is_empty() {
                    out.push(json!({"role": "user", "parts": parts}));
                }
            }
            MessageType::AIMessage => {
                let mut parts = Vec::new();
                if !message.content.is_empty() {
                    parts.push(json!({"text": message.content}));
                }
                for call in message.parse_tool_calls() {
                    names.insert(call.id.clone(), call.name.clone());
                    parts.push(json!({"functionCall": {
                        "name": call.name, "args": call.arguments_value()}}));
                }
                if !parts.is_empty() {
                    out.push(json!({"role": "model", "parts": parts}));
                }
            }
            MessageType::ToolMessage => {
                let id = message.id.clone().unwrap_or_default();
                let name = names.get(&id).cloned().unwrap_or(id);
                let part = json!({"functionResponse": {
                    "name": name, "response": {"result": message.content}}});
                match out.last_mut() {
                    Some(last) if is_tool_results(last) => {
                        last["parts"].as_array_mut().expect("parts").push(part)
                    }
                    _ => out.push(json!({"role": "user", "parts": [part]})),
                }
            }
        }
    }
    out
}

fn is_tool_results(content: &Value) -> bool {
    content["role"] == "user"
        && content["parts"]
            .as_array()
            .is_some_and(|p| p.iter().all(|p| p.get("functionResponse").is_some()))
}

fn image_part(image: &str) -> Value {
    let inline = image.strip_prefix("data:").and_then(|rest| {
        let (header, data) = rest.split_once(',')?;
        Some((header.strip_suffix(";base64")?, data))
    });
    match inline {
        Some((mime, data)) => json!({"inlineData": {"mimeType": mime, "data": data}}),
        None => json!({"fileData": {"fileUri": image}}),
    }
}

/// Gemini accepts an OpenAPI subset of JSON Schema: drops the keywords it rejects
/// and turns `["string", "null"]` types into `nullable`.
fn gemini_schema(schema: Value) -> Value {
    match schema {
        Value::Object(map) => {
            let mut out = Map::new();
            for (key, value) in map {
                match key.as_str() {
                    "$schema" | "additionalProperties" | "title" | "$defs" | "definitions" => {}
                    "type" if value.is_array() => {
                        let kinds: Vec<&Value> = value.as_array().unwrap().iter().collect();
                        if let Some(kind) = kinds.iter().find(|k| **k != "null") {
                            out.insert("type".into(), (*kind).clone());
                        }
                        if kinds.iter().any(|k| **k == "null") {
                            out.insert("nullable".into(), json!(true));
                        }
                    }
                    _ => {
                        out.insert(key, gemini_schema(value));
                    }
                }
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.into_iter().map(gemini_schema).collect()),
        other => other,
    }
}

/// What one response, or one streamed chunk of it, adds.
#[derive(Default)]
struct Piece {
    text: String,
    calls: Vec<(String, Value)>,
    usage: Option<TokenUsage>,
}

fn parse_piece(value: &Value) -> Result<Piece, LLMError> {
    let mut piece = Piece::default();
    if let Some(usage) = value.get("usageMetadata") {
        let count = |key: &str| usage[key].as_u64().unwrap_or_default() as u32;
        let mut tokens = TokenUsage::new(count("promptTokenCount"), count("candidatesTokenCount"));
        tokens.total_tokens = usage["totalTokenCount"]
            .as_u64()
            .map_or(tokens.total_tokens, |t| t as u32);
        piece.usage = Some(tokens);
    }
    let candidate = &value["candidates"][0];
    if candidate.is_null() {
        let reason = value["promptFeedback"]["blockReason"].as_str();
        return match reason {
            Some(reason) => Err(LLMError::OtherError(format!(
                "Gemini blocked the prompt: {reason}"
            ))),
            None if piece.usage.is_some() => Ok(piece),
            None => Err(LLMError::ContentNotFound("candidates[0]".into())),
        };
    }
    for part in candidate["content"]["parts"]
        .as_array()
        .into_iter()
        .flatten()
    {
        if let Some(call) = part.get("functionCall") {
            let name = call["name"].as_str().unwrap_or_default().to_string();
            let args = call.get("args").cloned().unwrap_or_else(|| json!({}));
            piece.calls.push((name, args));
        } else if part["thought"] != true {
            if let Some(text) = part["text"].as_str() {
                piece.text.push_str(text);
            }
        }
    }
    Ok(piece)
}

/// Joins the pieces of one answer; Gemini has no call ids, so they are made up.
#[derive(Default)]
struct Answer {
    text: String,
    calls: Vec<ToolCall>,
    usage: Option<TokenUsage>,
}

impl Answer {
    fn add(&mut self, piece: Piece) {
        self.text.push_str(&piece.text);
        for (name, args) in piece.calls {
            let id = format!("{name}#{}", self.calls.len());
            self.calls.push(ToolCall::from_value(id, name, &args));
        }
        self.usage = piece.usage.or(self.usage.take());
    }

    fn finish(self) -> GenerateResult {
        GenerateResult {
            tokens: self.usage,
            generation: self.text,
            tool_calls: self.calls,
            thinking: Vec::new(),
        }
    }
}

/// The JSON objects of the `data:` lines of a server-sent event stream.
/// The stream is capped at [`MAX_RESPONSE_BYTES`] in total: a server that
/// never stops sending no longer grows the buffer without bound.
fn sse_chunks(
    response: reqwest::Response,
) -> impl Stream<Item = Result<Value, LLMError>> + Send + 'static {
    async_stream::try_stream! {
        let mut bytes = response.bytes_stream();
        let mut buffer: Vec<u8> = Vec::new();
        let mut total: usize = 0;
        while let Some(chunk) = bytes.next().await {
            let chunk = chunk?;
            total = total.saturating_add(chunk.len());
            if total > MAX_RESPONSE_BYTES {
                Err::<(), _>(LLMError::OtherError(format!(
                    "Gemini stream exceeded the {MAX_RESPONSE_BYTES} byte limit"
                )))?;
            }
            buffer.extend_from_slice(&chunk);
            while let Some(end) = buffer.iter().position(|b| *b == b'\n') {
                let line: Vec<u8> = buffer.drain(..=end).collect();
                if let Some(value) = data_line(&line)? {
                    yield value;
                }
            }
        }
        if let Some(value) = data_line(&buffer)? {
            yield value;
        }
    }
}

fn data_line(line: &[u8]) -> Result<Option<Value>, LLMError> {
    let line = String::from_utf8_lossy(line);
    let Some(data) = line.trim().strip_prefix("data:") else {
        return Ok(None);
    };
    let data = data.trim();
    if data.is_empty() || data == "[DONE]" {
        return Ok(None);
    }
    Ok(Some(serde_json::from_str(data)?))
}

#[async_trait]
impl LLM for Gemini {
    async fn generate(&self, messages: &[Message]) -> Result<GenerateResult, LLMError> {
        let response = self.send(&self.url("generateContent")?, messages).await?;
        // Read capped (2 MiB) instead of `response.json()`, which reads
        // without a limit.
        let body = read_capped(response)
            .await
            .map_err(|e| LLMError::OtherError(e.to_string()))?;
        let value: Value = serde_json::from_slice(&body)?;
        let mut answer = Answer::default();
        answer.add(parse_piece(&value)?);
        Ok(answer.finish())
    }

    async fn stream(
        &self,
        messages: &[Message],
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamData, LLMError>> + Send>>, LLMError> {
        let url = format!("{}?alt=sse", self.url("streamGenerateContent")?);
        let response = self.send(&url, messages).await?;
        let stream = sse_chunks(response).map(|chunk| {
            let chunk = chunk?;
            let piece = parse_piece(&chunk)?;
            Ok(StreamData::new(chunk, piece.usage, piece.text))
        });
        Ok(Box::pin(stream))
    }

    fn generate_stream<'a>(&'a self, messages: &'a [Message]) -> LLMEventStream<'a> {
        Box::pin(async_stream::try_stream! {
            // `?` cannot live inside `format!(...)` here: hoist it out so the
            // error converts into the stream's error like the `?` below does.
            let base = self.url("streamGenerateContent")?;
            let url = format!("{base}?alt=sse");
            let response = self.send(&url, messages).await?;
            let chunks = sse_chunks(response);
            futures::pin_mut!(chunks);
            let mut answer = Answer::default();
            while let Some(chunk) = chunks.next().await {
                let piece = parse_piece(&chunk?)?;
                if !piece.text.is_empty() {
                    yield LLMEvent::Text(piece.text.clone());
                }
                answer.add(piece);
            }
            yield LLMEvent::Done(answer.finish());
        })
    }

    fn add_options(&mut self, options: CallOptions) {
        self.options.merge_options(options)
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    fn block_on<F: std::future::Future>(future: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            // A current-thread runtime with no special settings always builds.
            .expect("a test runtime always builds")
            .block_on(future)
    }

    fn other_error(err: LLMError) -> String {
        match err {
            LLMError::OtherError(message) => message,
            other => panic!("expected an OtherError, got {other:?}"),
        }
    }

    #[test]
    fn model_names_are_limited_to_the_characters_a_name_needs() {
        let good = Gemini::new("key", "gemini-2.5-flash")
            .url("generateContent")
            .expect("a plain model name builds a URL");
        assert_eq!(
            good,
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent"
        );
        for bad in ["../evil", "a/b", "a?b=c", "a#b", "a b", "a@b", "a:b", "", "a%b"] {
            let err = Gemini::new("key", bad).url("generateContent");
            assert!(err.is_err(), "model name {bad:?} must be refused");
        }
    }

    /// A refused base URL or model name fails in `generate` before any
    /// request is sent — so these tests need no network.
    #[test]
    fn generate_refuses_bad_urls_before_any_request() {
        let messages: &[Message] = &[];
        for llm in [
            Gemini::new("key", "../evil"),
            Gemini::new("key", "gemini-2.5-flash").with_base_url("file:///etc/passwd"),
            Gemini::new("key", "gemini-2.5-flash")
                .with_base_url("https://user:pass@example.com"),
            Gemini::new("key", "gemini-2.5-flash").with_base_url("not a url"),
        ] {
            let err = block_on(llm.generate(messages));
            assert!(err.is_err(), "the request must be refused, got {err:?}");
        }
        let message = other_error(
            block_on(
                Gemini::new("key", "gemini-2.5-flash")
                    .with_base_url("file:///etc/passwd")
                    .generate(messages),
            )
            .unwrap_err(),
        );
        assert!(
            message.contains("refused"),
            "the error must say the URL was refused, got {message:?}"
        );
    }
}
