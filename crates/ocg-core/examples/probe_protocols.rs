//! One-shot upstream protocol probe against a local OCG data dir.
//! Usage: cargo run -p ocg-core --example probe_protocols -- [data-dir]
//! Decrypts the first enabled account key and probes chat / responses / messages
//! for every model returned by upstream GET /v1/models.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ocg_core::crypto::MachineBoundCipher;
use ocg_core::db::Database;
use ocg_core::state::CoreStateInner;
use serde_json::{Value, json};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let data_dir = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let home = std::env::var("USERPROFILE")
                .or_else(|_| std::env::var("HOME"))
                .expect("home");
            PathBuf::from(home).join(".ocg-mgr")
        });

    let cipher = Arc::new(MachineBoundCipher::new());
    let db = Database::open(data_dir.clone())?;
    let state = CoreStateInner::new(db, data_dir, cipher)?;
    let (config, client) = state.upstream_context();
    let base = config.upstream_base_url.trim_end_matches('/').to_string();

    let accounts = state
        .db
        .lock()
        .list_accounts()?
        .into_iter()
        .filter(|a| a.enabled)
        .collect::<Vec<_>>();
    let account = accounts
        .first()
        .ok_or_else(|| anyhow::anyhow!("no enabled accounts in data dir"))?;
    let key = state.decrypt_key(&account.key_cipher)?;
    println!(
        "probe account={} name={} upstream={base}",
        account.id, account.name
    );

    let models_url = format!("{base}/v1/models");
    let models_resp = client
        .get(&models_url)
        .header("Authorization", format!("Bearer {key}"))
        .timeout(Duration::from_secs(60))
        .send()
        .await?;
    let models_status = models_resp.status();
    let models_body = models_resp.text().await?;
    if !models_status.is_success() {
        anyhow::bail!("GET /v1/models failed: {models_status} {models_body}");
    }
    let models_json: Value = serde_json::from_str(&models_body)?;
    let mut models = models_json
        .get("data")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|m| m.get("id").and_then(Value::as_str).map(str::to_owned))
        .collect::<Vec<_>>();
    models.sort();
    models.dedup();
    println!("models ({}): {}", models.len(), models.join(", "));
    println!();
    println!(
        "{:<22} {:<8} {:<8} {:<8}  notes",
        "model", "chat", "resp", "msg"
    );
    println!("{}", "-".repeat(80));

    let timeout = Duration::from_secs(90);
    for model in &models {
        let chat = probe_chat(&client, &base, &key, model, timeout).await;
        let resp = probe_responses(&client, &base, &key, model, timeout).await;
        let msg = probe_messages(&client, &base, &key, model, timeout).await;
        println!(
            "{:<22} {:<8} {:<8} {:<8}  {}",
            model,
            fmt_cell(&chat),
            fmt_cell(&resp),
            fmt_cell(&msg),
            summarize_notes(&chat, &resp, &msg)
        );
    }

    println!();
    println!("legend: OK=2xx success body; 4xx/5xx=HTTP status; ERR=transport/decode");
    Ok(())
}

#[derive(Clone)]
struct ProbeResult {
    status: u16,
    ms: u128,
    snippet: String,
    ok_shape: bool,
}

fn fmt_cell(r: &ProbeResult) -> String {
    if r.status == 0 {
        "ERR".to_string()
    } else if r.ok_shape {
        "OK".to_string()
    } else {
        r.status.to_string()
    }
}

fn summarize_notes(chat: &ProbeResult, resp: &ProbeResult, msg: &ProbeResult) -> String {
    let mut parts = Vec::new();
    for (name, r) in [("c", chat), ("r", resp), ("m", msg)] {
        if r.status == 0 {
            parts.push(format!("{name}:{}", short(&r.snippet, 40)));
        } else if !r.ok_shape {
            parts.push(format!("{name}:{} {}", r.status, short(&r.snippet, 50)));
        } else {
            parts.push(format!("{name}:{}ms", r.ms));
        }
    }
    parts.join(" | ")
}

fn short(s: &str, n: usize) -> String {
    let t = s.replace('\n', " ");
    t.chars().take(n).collect()
}

async fn probe_chat(
    client: &reqwest::Client,
    base: &str,
    key: &str,
    model: &str,
    timeout: Duration,
) -> ProbeResult {
    let body = json!({
        "model": model,
        "messages": [{"role": "user", "content": "ping"}],
        "max_tokens": 3,
        "stream": false
    });
    send(
        client,
        &format!("{base}/v1/chat/completions"),
        key,
        false,
        body,
        timeout,
        |v| {
            v.get("object").and_then(Value::as_str) == Some("chat.completion")
                || v.pointer("/choices/0/message").is_some()
        },
    )
    .await
}

async fn probe_responses(
    client: &reqwest::Client,
    base: &str,
    key: &str,
    model: &str,
    timeout: Duration,
) -> ProbeResult {
    let body = json!({
        "model": model,
        "input": "ping",
        "store": false,
        "max_output_tokens": 3,
        "stream": false
    });
    send(
        client,
        &format!("{base}/v1/responses"),
        key,
        false,
        body,
        timeout,
        |v| {
            v.get("object").and_then(Value::as_str) == Some("response") || v.get("output").is_some()
        },
    )
    .await
}

async fn probe_messages(
    client: &reqwest::Client,
    base: &str,
    key: &str,
    model: &str,
    timeout: Duration,
) -> ProbeResult {
    let body = json!({
        "model": model,
        "messages": [{"role": "user", "content": "ping"}],
        "max_tokens": 3,
        "stream": false
    });
    send(
        client,
        &format!("{base}/v1/messages"),
        key,
        true,
        body,
        timeout,
        |v| {
            v.get("type").and_then(Value::as_str) == Some("message")
                || v.get("content").and_then(Value::as_array).is_some()
        },
    )
    .await
}

async fn send(
    client: &reqwest::Client,
    url: &str,
    key: &str,
    anthropic: bool,
    body: Value,
    timeout: Duration,
    ok_shape: impl Fn(&Value) -> bool,
) -> ProbeResult {
    let started = Instant::now();
    let mut req = client
        .post(url)
        .timeout(timeout)
        .header("Content-Type", "application/json")
        .json(&body);
    if anthropic {
        req = req
            .header("x-api-key", key)
            .header("anthropic-version", "2023-06-01");
    } else {
        req = req.header("Authorization", format!("Bearer {key}"));
    }
    match req.send().await {
        Ok(resp) => {
            let status = resp.status().as_u16();
            let text = resp.text().await.unwrap_or_default();
            let ms = started.elapsed().as_millis();
            let parsed: Result<Value, _> = serde_json::from_str(&text);
            let ok_shape = status < 300 && parsed.as_ref().is_ok_and(ok_shape);
            let snippet = if ok_shape {
                "ok".into()
            } else {
                text.chars().take(160).collect()
            };
            ProbeResult {
                status,
                ms,
                snippet,
                ok_shape,
            }
        }
        Err(e) => ProbeResult {
            status: 0,
            ms: started.elapsed().as_millis(),
            snippet: e.to_string(),
            ok_shape: false,
        },
    }
}
