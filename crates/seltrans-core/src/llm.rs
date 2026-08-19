//! 大模型调用。两套后端：
//! - `openai`：`POST {base}/chat/completions`，`Authorization: Bearer`
//! - `anthropic`：`POST {base}/v1/messages`，`x-api-key` + `anthropic-version`
//!
//! 两边都走 SSE 流式。刻意**不发送 temperature** —— Claude 当前世代模型收到该参数会直接
//! 400，部分 OpenAI 兼容服务对推理模型也有同样限制；需要的话可以在供应商配置的
//! 「附加请求体」里自己加。

use futures_util::StreamExt;
use serde_json::{Map, Value, json};
use std::sync::OnceLock;
use std::time::Duration;

use crate::config::Provider;
use crate::logging;

#[derive(Debug, Clone)]
pub enum Event {
    Delta(String),
    Done,
    Error(String),
}

static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();

/// 共用的 tokio 运行时（GUI 主线程只跑 GTK，网络都扔到这里）
pub fn runtime() -> &'static tokio::runtime::Runtime {
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("无法创建 tokio 运行时")
    })
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(120))
        .user_agent(concat!("seltrans/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("无法创建 HTTP 客户端")
}

fn join(base: &str, path: &str) -> String {
    format!(
        "{}/{}",
        base.trim().trim_end_matches('/'),
        path.trim_start_matches('/')
    )
}

/// Claude 里只有部分世代支持 output_config.effort；Haiku 4.5 / Sonnet 4.5 这类收到会报错，
/// 所以按模型名判断后再发。低 effort 能显著压掉划词翻译的延迟。
fn anthropic_supports_effort(model: &str) -> bool {
    const OK: &[&str] = &[
        "opus-5",
        "opus-4-8",
        "opus-4-7",
        "opus-4-6",
        "opus-4-5",
        "sonnet-5",
        "sonnet-4-6",
        "fable-5",
        "mythos-5",
    ];
    OK.iter().any(|m| model.contains(m))
}

/// 把用户填的「附加请求体」合并进去（覆盖同名字段）
fn merge_extra(body: &mut Map<String, Value>, extra: &str) {
    let extra = extra.trim();
    if extra.is_empty() {
        return;
    }
    if let Ok(Value::Object(m)) = serde_json::from_str::<Value>(extra) {
        for (k, v) in m {
            body.insert(k, v);
        }
    }
}

fn build_body(p: &Provider, system: &str, user: &str, stream: bool) -> Value {
    let mut body = Map::new();
    body.insert("model".into(), json!(p.model));
    body.insert("stream".into(), json!(stream));

    if p.is_anthropic() {
        body.insert("max_tokens".into(), json!(8192));
        body.insert("system".into(), json!(system));
        body.insert(
            "messages".into(),
            json!([{"role": "user", "content": user}]),
        );
        if anthropic_supports_effort(&p.model) {
            body.insert("output_config".into(), json!({"effort": "low"}));
        }
    } else {
        body.insert(
            "messages".into(),
            json!([
                {"role": "system", "content": system},
                {"role": "user", "content": user},
            ]),
        );
    }

    merge_extra(&mut body, &p.extra_body);
    Value::Object(body)
}

fn apply_auth(req: reqwest::RequestBuilder, p: &Provider) -> reqwest::RequestBuilder {
    let key = p.api_key.trim();
    if p.is_anthropic() {
        let req = req.header("anthropic-version", "2023-06-01");
        if key.is_empty() {
            req
        } else {
            req.header("x-api-key", key)
        }
    } else if key.is_empty() {
        req
    } else {
        req.bearer_auth(key)
    }
}

fn endpoint(p: &Provider) -> String {
    if p.is_anthropic() {
        join(&p.base_url, "v1/messages")
    } else {
        join(&p.base_url, "chat/completions")
    }
}

fn truncate(s: &str, n: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let cut: String = s.chars().take(n).collect();
        format!("{cut}…")
    }
}

fn precheck(p: &Provider) -> Result<(), String> {
    if p.base_url.trim().is_empty() {
        return Err("这个供应商还没填 base_url，请到设置里补上".into());
    }
    if p.model.trim().is_empty() {
        return Err("还没选模型，请到设置里点「拉取模型」挑一个".into());
    }
    Ok(())
}

/// 从一行 SSE `data:` 里抽出增量文本。返回 Err 表示服务端回了 error 事件。
fn extract_delta(p: &Provider, j: &Value) -> Result<Option<String>, String> {
    if let Some(err) = j.get("error") {
        let msg = err
            .get("message")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| err.to_string());
        return Err(msg);
    }

    if p.is_anthropic() {
        if j.get("type").and_then(Value::as_str) == Some("content_block_delta") {
            let d = &j["delta"];
            if d.get("type").and_then(Value::as_str) == Some("text_delta") {
                return Ok(d.get("text").and_then(Value::as_str).map(str::to_string));
            }
        }
        Ok(None)
    } else {
        let d = &j["choices"][0]["delta"];
        // 有些服务把思考过程放在 reasoning_content 里，这里只取正文
        Ok(d.get("content")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string))
    }
}

/// 流式翻译。每来一段就往 `tx` 里塞一个 Event，结束时塞 Done 或 Error。
pub async fn stream_translate(
    p: Provider,
    system: String,
    user: String,
    tx: async_channel::Sender<Event>,
) {
    if let Err(e) = precheck(&p) {
        logging::error(&format!("请求前置检查未通过：{e}"));
        let _ = tx.send(Event::Error(e)).await;
        return;
    }

    // 这里是排查"模型说没收到内容"最关键的一条日志：记录真正发出去的用户消息
    logging::info(&format!(
        "发起翻译 | 供应商={} kind={} 模型={} 端点={} | system={} 字符 | user={} 字符 | user 预览: {}",
        p.name,
        p.kind,
        p.model,
        endpoint(&p),
        system.chars().count(),
        user.chars().count(),
        logging::preview(&user)
    ));

    if logging::is_blank(&user) {
        let msg = "待翻译的内容是空的（或只有空白/零宽字符），已拦下，不发请求".to_string();
        logging::error(&msg);
        let _ = tx.send(Event::Error(msg)).await;
        return;
    }

    let body = build_body(&p, &system, &user, true);
    let req = apply_auth(client().post(endpoint(&p)), &p).json(&body);

    let resp = match req.send().await {
        Ok(r) => r,
        Err(e) => {
            let msg = format!("请求发不出去：{e}");
            logging::error(&msg);
            let _ = tx.send(Event::Error(msg)).await;
            return;
        }
    };

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        logging::error(&format!("HTTP {status} | 响应体：{}", truncate(&text, 600)));
        let _ = tx
            .send(Event::Error(format!(
                "HTTP {status}\n{}",
                truncate(&text, 600)
            )))
            .await;
        return;
    }

    let mut stream = resp.bytes_stream();
    // 按字节缓冲再按行切，避免在多字节 UTF-8 中间截断（中文会变乱码）
    let mut buf: Vec<u8> = Vec::new();
    let mut got_any = false;
    let mut total_chars = 0usize;

    while let Some(item) = stream.next().await {
        let chunk = match item {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.send(Event::Error(format!("连接中断：{e}"))).await;
                return;
            }
        };
        buf.extend_from_slice(&chunk);

        while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
            let raw: Vec<u8> = buf.drain(..=pos).collect();
            let line = String::from_utf8_lossy(&raw);
            let line = line.trim();

            if line.is_empty() || line.starts_with(':') {
                continue;
            }
            let Some(data) = line.strip_prefix("data:") else {
                continue; // event: / id: 之类的字段，JSON 里也带 type，不用管
            };
            let data = data.trim();
            if data == "[DONE]" {
                logging::info(&format!("翻译完成，共 {total_chars} 字符"));
                let _ = tx.send(Event::Done).await;
                return;
            }
            let Ok(j) = serde_json::from_str::<Value>(data) else {
                continue;
            };

            match extract_delta(&p, &j) {
                Err(e) => {
                    logging::error(&format!("服务端返回 error 事件：{e}"));
                    let _ = tx.send(Event::Error(e)).await;
                    return;
                }
                Ok(Some(text)) => {
                    got_any = true;
                    total_chars += text.chars().count();
                    if tx.send(Event::Delta(text)).await.is_err() {
                        logging::info("窗口已关闭，中止流式接收");
                        return; // 窗口关了
                    }
                }
                Ok(None) => {}
            }
        }
    }

    if !got_any {
        let msg = "服务端没有返回任何内容。检查模型名是否正确、账户是否还有额度".to_string();
        logging::error(&msg);
        let _ = tx.send(Event::Error(msg)).await;
    } else {
        logging::info(&format!("翻译完成，共 {total_chars} 字符"));
        let _ = tx.send(Event::Done).await;
    }
}

/// 拉取该供应商的可用模型列表
pub async fn list_models(p: Provider) -> Result<Vec<String>, String> {
    if p.base_url.trim().is_empty() {
        return Err("请先填写 base_url".into());
    }
    let url = if p.is_anthropic() {
        join(&p.base_url, "v1/models")
    } else {
        join(&p.base_url, "models")
    };

    let resp = apply_auth(client().get(&url), &p)
        .send()
        .await
        .map_err(|e| format!("请求失败：{e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("HTTP {status}\n{}", truncate(&text, 400)));
    }

    let j: Value = resp
        .json()
        .await
        .map_err(|e| format!("返回的不是合法 JSON：{e}"))?;

    let arr = j
        .get("data")
        .and_then(Value::as_array)
        .or_else(|| j.get("models").and_then(Value::as_array))
        .ok_or_else(|| "返回里没有 data 字段，这个服务可能不支持 /models 接口".to_string())?;

    let mut models: Vec<String> = arr
        .iter()
        .filter_map(|m| {
            m.get("id")
                .or_else(|| m.get("name"))
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect();
    models.sort();
    models.dedup();

    if models.is_empty() {
        Err("模型列表是空的".into())
    } else {
        Ok(models)
    }
}

/// 发一条最短的请求验证 key / base_url / 模型名是否都对
pub async fn test_connection(p: Provider) -> Result<String, String> {
    precheck(&p)?;
    let body = build_body(&p, "You are a helpful assistant.", "ping", false);
    let resp = apply_auth(client().post(endpoint(&p)), &p)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("请求失败：{e}"))?;

    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(format!("HTTP {status}\n{}", truncate(&text, 500)));
    }

    let j: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    let reply = if p.is_anthropic() {
        j["content"][0]["text"].as_str().unwrap_or("")
    } else {
        j["choices"][0]["message"]["content"].as_str().unwrap_or("")
    };
    Ok(format!("连接正常，模型回了：{}", truncate(reply, 60)))
}
