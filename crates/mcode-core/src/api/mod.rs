//! OpenAI Responses API 客户端 — 唯一协议（对齐 MCode.md 1.5 与 CLI api/client.ts），SSE 流式。
//! 实现 `agent::ModelBackend`。无请求级超时（只有用户打断能终止长生成）。

use anyhow::Result;
use futures::StreamExt;
use serde_json::{json, Value};

use crate::agent::{
    ApiMessage, Attachment, ModelBackend, StreamEvent, StreamTurn, ToolCall,
};
use crate::config::Provider;

pub struct ResponsesBackend {
    provider: Provider,
    model: String,
    client: reqwest::Client,
}

impl ResponsesBackend {
    pub fn new(provider: Provider, model: String) -> Result<Self> {
        let key = provider
            .resolve_api_key()
            .ok_or_else(|| anyhow::anyhow!("API key 不可用：环境变量 {} 未设置且值不是明文 key", provider.api_key_env))?;
        let client = reqwest::Client::builder()
            .default_headers({
                let mut h = reqwest::header::HeaderMap::new();
                h.insert("Authorization", format!("Bearer {key}").parse()?);
                h
            })
            .build()?;
        Ok(Self { provider, model, client })
    }

    fn url(&self) -> String {
        format!("{}/responses", self.provider.base_url.trim_end_matches('/'))
    }
}

#[async_trait::async_trait]
impl ModelBackend for ResponsesBackend {
    async fn stream_turn(
        &self,
        system: &str,
        history: &[ApiMessage],
        user_input: &str,
        attachments: &[Attachment],
        tools: &[crate::tools::ToolDefinition],
        on_event: &mut (dyn FnMut(StreamEvent) + Send),
        cancel: tokio::sync::watch::Receiver<bool>,
    ) -> Result<StreamTurn> {
        // input = 历史（user→input_text / assistant→output_text）+ 当前输入（可带图片附件）
        let mut input: Vec<Value> = history
            .iter()
            .map(|m| {
                let (t, text) = match m.role {
                    crate::agent::Role::User => ("input_text", &m.text),
                    _ => ("output_text", &m.text),
                };
                json!({ "role": role_str(m.role), "content": [{ "type": t, "text": text }] })
            })
            .collect();
        let mut user_content = vec![json!({ "type": "input_text", "text": user_input })];
        for a in attachments {
            user_content.push(json!({ "type": "input_image", "image_url": a.data_url }));
        }
        input.push(json!({ "role": "user", "content": user_content }));

        let tools_json: Vec<Value> = tools
            .iter()
            .map(|t| {
                json!({
                    "type": "function",
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters,
                })
            })
            .collect();

        let body = json!({
            "model": self.model,
            "instructions": system,
            "input": input,
            "tools": tools_json,
            "stream": true,
        });

        let resp = self.client.post(self.url()).json(&body).send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("API {status}: {}", truncate(&text, 500));
        }

        let mut stream = resp.bytes_stream();
        let mut buf = String::new();
        let mut turn = StreamTurn::default();

        while let Some(chunk) = stream.next().await {
            if *cancel.borrow() {
                turn.interrupted = true;
                return Ok(turn);
            }
            let chunk = chunk?;
            buf.push_str(&String::from_utf8_lossy(&chunk));
            // SSE 按行取 data: 载荷（对齐 CLI parseSSE）
            while let Some(pos) = buf.find('\n') {
                let line: String = buf.drain(..=pos).collect();
                let line = line.trim_end();
                let Some(data) = line.strip_prefix("data:") else { continue };
                let data = data.trim();
                if data.is_empty() || data == "[DONE]" {
                    continue;
                }
                let Ok(v) = serde_json::from_str::<Value>(data) else { continue };
                match v["type"].as_str() {
                    Some("response.output_text.delta") => {
                        if let Some(d) = v["delta"].as_str() {
                            if !d.is_empty() {
                                turn.reply.push_str(d);
                                on_event(StreamEvent::TextDelta(d.to_string()));
                            }
                        }
                    }
                    Some("response.reasoning_summary_text.delta") | Some("response.reasoning_text.delta") => {
                        if let Some(d) = v["delta"].as_str() {
                            if !d.is_empty() {
                                on_event(StreamEvent::Reasoning(d.to_string()));
                            }
                        }
                    }
                    Some("response.output_item.done") => {
                        let item = &v["item"];
                        if item["type"] == "function_call" {
                            turn.tool_calls.push(ToolCall {
                                call_id: item["call_id"].as_str().unwrap_or("").into(),
                                name: item["name"].as_str().unwrap_or("").into(),
                                arguments: item["arguments"].as_str().unwrap_or("{}").into(),
                            });
                        }
                    }
                    Some("response.completed") => {
                        if let Some(u) = v["response"]["usage"].as_object() {
                            let usage = crate::util::TokenUsage {
                                input: u.get("input_tokens").and_then(|x| x.as_u64()).unwrap_or(0),
                                output: u.get("output_tokens").and_then(|x| x.as_u64()).unwrap_or(0),
                                cache_read: u
                                    .get("input_tokens_details")
                                    .and_then(|d| d.get("cached_tokens"))
                                    .and_then(|x| x.as_u64())
                                    .unwrap_or(0),
                                cache_write: 0,
                            };
                            turn.usage = Some(usage);
                        }
                    }
                    _ => {}
                }
            }
        }

        Ok(turn)
    }
}

fn role_str(r: crate::agent::Role) -> &'static str {
    match r {
        crate::agent::Role::User | crate::agent::Role::Tool => "user",
        crate::agent::Role::Assistant => "assistant",
    }
}

fn truncate(s: &str, max: usize) -> String {
    match s.char_indices().nth(max) {
        Some((i, _)) => s[..i].to_string(),
        None => s.to_string(),
    }
}
