//! OpenAI Responses API 客户端 — 唯一协议（对齐 MCode.md 1.5），SSE 流式。
//! 实现 `agent::ModelBackend`，供主循环调度。

use anyhow::Result;
use futures::StreamExt;
use serde_json::{json, Value};

use crate::agent::{Message, ModelBackend, TurnEvent, TurnResult};
use crate::config::Provider;

pub struct ResponsesBackend {
    pub provider: Provider,
    pub model: String,
    client: reqwest::Client,
}

impl ResponsesBackend {
    pub fn new(provider: Provider, model: String) -> Result<Self> {
        let key = std::env::var(&provider.api_key_env)
            .map_err(|_| anyhow::anyhow!("环境变量 {} 未设置（apiKey 只存环境变量名）", provider.api_key_env))?;
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
    async fn run_turn(
        &self,
        system: &str,
        history: &[Message],
        tools: &[crate::tools::ToolDefinition],
        on_event: &mut (dyn FnMut(TurnEvent) + Send),
        cancel: tokio::sync::watch::Receiver<bool>,
    ) -> Result<TurnResult> {
        // 组装 Responses 请求体：input 为消息数组，tools 为扁平 function 定义
        let input: Vec<Value> = history
            .iter()
            .map(|m| {
                let role = match m.role {
                    crate::agent::Role::User | crate::agent::Role::Tool => "user",
                    crate::agent::Role::Assistant => "assistant",
                };
                let content: Vec<Value> = m
                    .content
                    .iter()
                    .map(|c| match c {
                        crate::agent::ContentBlock::Text { text } => {
                            json!({ "type": "input_text", "text": text })
                        }
                        crate::agent::ContentBlock::Image { data, .. } => {
                            json!({ "type": "input_image", "image_url": data })
                        }
                        _ => json!({ "type": "input_text", "text": "[tool block]" }),
                    })
                    .collect();
                json!({ "role": role, "content": content })
            })
            .collect();

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
            anyhow::bail!("Responses API {status}: {}", truncate(&text, 500));
        }

        let mut stream = resp.bytes_stream();
        let mut buf = String::new();
        let mut reply = String::new();
        let mut tool_calls: usize = 0;
        let mut usage = crate::util::TokenUsage::default();

        while let Some(chunk) = stream.next().await {
            if *cancel.borrow() {
                return Ok(TurnResult { reply, tool_calls, interrupted: true });
            }
            let chunk = chunk?;
            buf.push_str(&String::from_utf8_lossy(&chunk));
            // SSE：按空行分帧，data: 前缀取 payload
            while let Some(pos) = buf.find("\n\n") {
                let frame: String = buf.drain(..pos + 2).collect();
                for line in frame.lines() {
                    let Some(data) = line.strip_prefix("data: ") else { continue };
                    let Ok(v) = serde_json::from_str::<Value>(data) else { continue };
                    match v["type"].as_str() {
                        Some("response.output_text.delta") => {
                            if let Some(d) = v["delta"].as_str() {
                                reply.push_str(d);
                                on_event(TurnEvent::TextDelta(d.to_string()));
                            }
                        }
                        Some("response.function_call_arguments.done") => {
                            tool_calls += 1;
                            on_event(TurnEvent::ToolCallStart {
                                id: v["item_id"].as_str().unwrap_or("").into(),
                                name: v["name"].as_str().unwrap_or("").into(),
                                input: serde_json::from_str(v["arguments"].as_str().unwrap_or("{}")).unwrap_or(json!({})),
                            });
                        }
                        Some("response.completed") => {
                            if let Some(u) = v["response"]["usage"].as_object() {
                                usage.input = u.get("input_tokens").and_then(|x| x.as_u64()).unwrap_or(0);
                                usage.output = u.get("output_tokens").and_then(|x| x.as_u64()).unwrap_or(0);
                                on_event(TurnEvent::Usage(usage.clone()));
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        Ok(TurnResult { reply, tool_calls, interrupted: false })
    }
}

fn truncate(s: &str, max: usize) -> &str {
    match s.char_indices().nth(max) {
        Some((i, _)) => &s[..i],
        None => s,
    }
}
