//! HTTP-only helpers for the v2 alias / multi-Plan black-box suite.
//!
//! Tests talk to Gateway and dashboard JSON. They do not construct private
//! gateway types. `CoreStateInner` is used only to boot an isolated data dir.

#![allow(dead_code)]

use ocg_core::crypto::{KeyCipher, StaticKeyCipher};
use ocg_core::db::Database;
use ocg_core::gateway;
use ocg_core::models::ProxyMode;
use ocg_core::state::{CoreStateInner, GatewayHandle};
use reqwest::StatusCode;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

#[path = "../fake_upstream.rs"]
mod fake_upstream;

pub use fake_upstream::FakeReply;
use fake_upstream::{FakeCall, FakeCalls, start_fake_upstream, start_raw_disconnect_upstream};

pub const GATEWAY_KEY: &str = "gw-v2-contract";
pub const GO_ACCOUNT_KEY: &str = "v2-secret-KEY-9f3a2c1b-go";
pub const GO_ACCOUNT_KEY_2: &str = "v2-secret-KEY-9f3a2c1b-go-2";
pub const GOAT_ACCOUNT_KEY: &str = "v2-secret-KEY-9f3a2c1b-goat";
pub const SCNET_ACCOUNT_KEY: &str = "sk-tp-v2-secret-KEY-9f3a2c1b";
pub const CUSTOM_ACCOUNT_KEY: &str = "v2-secret-KEY-9f3a2c1b-custom";

pub const OPENCODE_PROVIDER_ID: &str = "opencode";
pub const GO_OFFERING_ID: &str = "go";
pub const COMMAND_CODE_PROVIDER_ID: &str = "command-code";
pub const GOAT_OFFERING_ID: &str = "goat";
pub const SCNET_PROVIDER_ID: &str = "scnet";
pub const SCNET_BASIC_OFFERING_ID: &str = "token-plan-basic";
pub const SCNET_STANDARD_OFFERING_ID: &str = "token-plan-standard";
pub const SCNET_PREMIUM_OFFERING_ID: &str = "token-plan-premium";
pub const CUSTOM_PROVIDER_ID: &str = "custom";
pub const CUSTOM_OFFERING_ID: &str = "api";
pub const CUSTOM_UNROUTABLE_MODEL_ID: &str = "custom-unroutable-model";

pub const GO_ALIAS: &str = "deepseek-v4-flash";
pub const GOAT_UNIQUE_RAW_ID: &str = "deepseek/deepseek-v4-flash";
pub const FREE_MODEL: &str = "deepseek-v4-flash-free";
pub const AMBIGUOUS_ERROR_TYPE: &str = "ambiguous_model_id";
pub const CUSTOM_OVERLAP_RAW_ID: &str = "shared-raw-model";

pub const SUCCESS_CHAT_BODY: &str = r#"{"id":"ok","object":"chat.completion","model":"upstream-should-not-leak","choices":[{"index":0,"message":{"role":"assistant","content":"ok"},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":2,"prompt_tokens_details":{"cached_tokens":0}}}"#;

pub const MIXED_UPSTREAM_MODELS_BODY: &str = r#"{"object":"list","data":[{"id":"deepseek-v4-flash"},{"id":"deepseek/deepseek-v4-flash"},{"id":"vendor-raw-not-an-alias"},{"id":"minimax-m2.7"},{"id":"grok-4.5"}]}"#;

pub const CATALOG_CONTRACT: &str = include_str!("catalog_contract.json");
pub const ALIAS_CONTRACT: &str = include_str!("alias_contract.json");

const CHAT_STREAM_HEAD: &str = concat!(
    "data: {\"id\":\"chat-stream\",\"model\":\"deepseek-v4-flash\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"ok\"},\"finish_reason\":null}]}\n\n"
);

pub fn loopback_client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("v2 test client should build")
}

pub fn catalog_contract() -> Value {
    serde_json::from_str(CATALOG_CONTRACT).expect("catalog contract fixture")
}

pub fn alias_contract() -> Value {
    serde_json::from_str(ALIAS_CONTRACT).expect("alias contract fixture")
}

pub struct V2Harness {
    pub state: Arc<CoreStateInner>,
    pub dir: PathBuf,
    pub handle: GatewayHandle,
    pub client: reqwest::Client,
    pub port: u16,
    pub upstream_base_url: String,
    fake_calls: Option<FakeCalls>,
    disconnect_calls: Option<Arc<std::sync::atomic::AtomicUsize>>,
    stop_fake: Option<tokio::sync::oneshot::Sender<()>>,
}

impl V2Harness {
    pub async fn start() -> Self {
        Self::start_with_upstream(None).await
    }

    pub async fn start_with_chat_success(account_keys: &[&str]) -> Self {
        let mut replies = HashMap::new();
        for key in account_keys {
            replies.insert(
                (*key).to_string(),
                VecDeque::from([FakeReply {
                    status: 200,
                    body: SUCCESS_CHAT_BODY,
                }]),
            );
        }
        replies.insert(
            String::new(),
            VecDeque::from([FakeReply {
                status: 200,
                body: SUCCESS_CHAT_BODY,
            }]),
        );
        Self::start_with_upstream(Some(replies)).await
    }

    pub async fn start_with_upstream(
        replies: Option<HashMap<String, VecDeque<FakeReply>>>,
    ) -> Self {
        let dir = temp_data_dir();
        let db = Database::open(dir.clone()).unwrap();
        let cipher: Arc<dyn KeyCipher + Send + Sync> = Arc::new(StaticKeyCipher::new("v2-tests"));
        let state = Arc::new(CoreStateInner::new(db, dir.clone(), cipher).unwrap());

        let (upstream, fake_calls, stop_fake) = if let Some(replies) = replies {
            let (base, calls, stop) = start_fake_upstream(replies).await;
            (Some(base), Some(calls), Some(stop))
        } else {
            (None, None, None)
        };

        let mut config = state.config();
        config.gateway_key = GATEWAY_KEY.into();
        config.proxy_mode = ProxyMode::Direct;
        let upstream_base_url = if let Some(base) = upstream {
            // Go and Zen share this suffix; the fake server is path-agnostic.
            format!("{}/zen/go", base.trim_end_matches('/'))
        } else {
            // Isolated tests must never touch a real provider. A closed
            // loopback port fails closed without leaving the machine.
            "http://127.0.0.1:1".into()
        };
        config.upstream_base_url = upstream_base_url.clone();
        state.set_config(config).unwrap();

        let handle =
            gateway::start_gateway_on(state.clone(), SocketAddr::from(([127, 0, 0, 1], 0)))
                .await
                .unwrap();
        let client = loopback_client();
        wait_ready(&client, handle.port).await;
        Self {
            state,
            dir,
            port: handle.port,
            handle,
            client,
            upstream_base_url,
            fake_calls,
            disconnect_calls: None,
            stop_fake,
        }
    }

    pub fn dashboard(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}/dashboard/api{path}", self.port)
    }

    pub fn gateway(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{path}", self.port)
    }

    pub async fn get_json(&self, path: &str) -> (StatusCode, Value) {
        let response = self.client.get(self.dashboard(path)).send().await.unwrap();
        let status = response.status();
        let body = decode_json(response).await;
        (status, body)
    }

    pub async fn post_json(&self, path: &str, body: &Value) -> (StatusCode, Value) {
        let response = self
            .client
            .post(self.dashboard(path))
            .json(body)
            .send()
            .await
            .unwrap();
        let status = response.status();
        let body = decode_json(response).await;
        (status, body)
    }

    pub async fn patch_json(&self, path: &str, body: &Value) -> (StatusCode, Value) {
        let response = self
            .client
            .patch(self.dashboard(path))
            .json(body)
            .send()
            .await
            .unwrap();
        let status = response.status();
        let body = decode_json(response).await;
        (status, body)
    }

    pub async fn catalog(&self) -> Value {
        let (status, body) = self.get_json("/providers/catalog").await;
        assert_eq!(
            status,
            StatusCode::OK,
            "catalog must be readable on loopback: {body}"
        );
        body
    }

    pub async fn accounts(&self) -> Value {
        let (status, body) = self.get_json("/accounts").await;
        assert_eq!(status, StatusCode::OK, "account list: {body}");
        body
    }

    pub async fn create_account(&self, payload: Value) -> (StatusCode, Value) {
        self.post_json("/accounts", &payload).await
    }

    pub async fn create_go_account(&self, name: &str, key: &str) -> Value {
        let revision = self.settings_revision().await;
        let (status, body) = self
            .create_account(json!({
                "provider_id": OPENCODE_PROVIDER_ID,
                "offering_id": GO_OFFERING_ID,
                "name": name,
                "key": key,
                "expected_revision": revision
            }))
            .await;
        assert_eq!(status, StatusCode::OK, "create Go account: {body}");
        body
    }

    pub async fn account_by_id(&self, id: &str) -> Value {
        self.accounts()
            .await
            .as_array()
            .into_iter()
            .flatten()
            .find(|account| account["id"] == id)
            .cloned()
            .unwrap_or_else(|| panic!("account {id} missing from dashboard list"))
    }

    pub async fn settings_revision(&self) -> u64 {
        let (_, settings) = self.get_json("/settings").await;
        settings["revision"].as_u64().unwrap_or(0)
    }

    pub async fn chat(&self, model: &str) -> (StatusCode, Value) {
        let response = self
            .client
            .post(self.gateway("/v1/chat/completions"))
            .header(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {GATEWAY_KEY}"),
            )
            .json(&json!({
                "model": model,
                "messages": [{"role": "user", "content": "ping"}],
                "max_tokens": 3,
                "stream": false
            }))
            .send()
            .await
            .unwrap();
        let status = response.status();
        let body = decode_json(response).await;
        (status, body)
    }

    pub async fn list_client_models(&self) -> (StatusCode, Value) {
        let response = self
            .client
            .get(self.gateway("/v1/models"))
            .header(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {GATEWAY_KEY}"),
            )
            .send()
            .await
            .unwrap();
        let status = response.status();
        let body = decode_json(response).await;
        (status, body)
    }

    pub async fn claude_desktop_models(&self) -> (StatusCode, Value) {
        let response = self
            .client
            .get(self.gateway("/claude-desktop/v1/models"))
            .header(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {GATEWAY_KEY}"),
            )
            .send()
            .await
            .unwrap();
        let status = response.status();
        let body = decode_json(response).await;
        (status, body)
    }

    pub async fn forward_logs(&self) -> Value {
        let (status, body) = self.get_json("/logs/forward?limit=50").await;
        assert_eq!(status, StatusCode::OK, "forward logs: {body}");
        body
    }

    pub async fn gateway_logs(&self) -> Value {
        let (status, body) = self.get_json("/logs/gateway?limit=100").await;
        assert_eq!(status, StatusCode::OK, "gateway logs: {body}");
        body
    }

    pub fn fake_calls(&self) -> Vec<FakeCall> {
        self.fake_calls
            .as_ref()
            .map(|calls| calls.lock().expect("fake call log").clone())
            .unwrap_or_default()
    }

    pub fn fake_call_keys(&self) -> Vec<String> {
        self.fake_calls().into_iter().map(|call| call.key).collect()
    }

    pub fn disconnect_call_count(&self) -> usize {
        self.disconnect_calls
            .as_ref()
            .map(|calls| calls.load(std::sync::atomic::Ordering::Relaxed))
            .unwrap_or(0)
    }

    pub fn shutdown(mut self) {
        gateway::stop_gateway(self.handle);
        if let Some(stop) = self.stop_fake.take() {
            let _ = stop.send(());
        }
        let _ = fs::remove_dir_all(&self.dir);
    }
}

pub async fn start_output_then_disconnect_upstream() -> (
    String,
    Arc<std::sync::atomic::AtomicUsize>,
    tokio::sync::oneshot::Sender<()>,
) {
    let payload = CHAT_STREAM_HEAD;
    let raw = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ntransfer-encoding: chunked\r\nconnection: close\r\n\r\n{:X}\r\n{}\r\n",
        payload.len(),
        payload
    )
    .into_bytes();
    start_raw_disconnect_upstream(raw).await
}

pub async fn start_v2_with_disconnect_upstream() -> V2Harness {
    let dir = temp_data_dir();
    let db = Database::open(dir.clone()).unwrap();
    let cipher: Arc<dyn KeyCipher + Send + Sync> = Arc::new(StaticKeyCipher::new("v2-tests"));
    let state = Arc::new(CoreStateInner::new(db, dir.clone(), cipher).unwrap());
    let (base, calls, stop) = start_output_then_disconnect_upstream().await;
    let mut config = state.config();
    config.gateway_key = GATEWAY_KEY.into();
    config.proxy_mode = ProxyMode::Direct;
    config.upstream_base_url = format!("{}/zen/go", base.trim_end_matches('/'));
    state.set_config(config).unwrap();
    let handle = gateway::start_gateway_on(state.clone(), SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap();
    let client = loopback_client();
    wait_ready(&client, handle.port).await;
    V2Harness {
        state,
        dir,
        port: handle.port,
        handle,
        client,
        upstream_base_url: format!("{}/zen/go", base.trim_end_matches('/')),
        fake_calls: None,
        disconnect_calls: Some(calls),
        stop_fake: Some(stop),
    }
}

pub fn catalog_entry<'a>(
    catalog: &'a Value,
    provider_id: &str,
    offering_id: &str,
) -> Option<&'a Value> {
    catalog
        .as_array()?
        .iter()
        .find(|entry| entry["provider_id"] == provider_id && entry["offering_id"] == offering_id)
}

pub fn catalog_aliases(entry: &Value) -> Vec<Value> {
    match &entry["model_aliases"] {
        Value::Array(items) => items.clone(),
        _ => Vec::new(),
    }
}

pub fn alias_name_list(entry: &Value) -> Vec<String> {
    catalog_aliases(entry)
        .into_iter()
        .filter_map(|item| {
            item.as_str()
                .or_else(|| item["alias"].as_str())
                .map(str::to_string)
        })
        .collect()
}

pub fn alias_names(entry: &Value) -> HashSet<String> {
    alias_name_list(entry).into_iter().collect()
}

pub fn form_field_ids(entry: &Value) -> HashSet<String> {
    entry["form_fields"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|field| field["id"].as_str().map(str::to_string))
        .collect()
}

pub fn matching_acknowledgements(notice: &Value) -> Value {
    json!([{
        "acknowledgement_id": notice["acknowledgement_id"],
        "version": notice["version"]
    }])
}

pub fn custom_create_payload(
    name: &str,
    key: &str,
    revision: u64,
    base_url: &str,
    model_id: &str,
) -> Value {
    json!({
        "provider_id": CUSTOM_PROVIDER_ID,
        "offering_id": CUSTOM_OFFERING_ID,
        "name": name,
        "key": key,
        "expected_revision": revision,
        "custom_config": {
            "base_url": base_url,
            "upstream_protocol": "chat_completions",
            "auth_scheme": "bearer"
        },
        "model_capabilities": [{
            "model_id": model_id,
            "protocol": "chat_completions"
        }]
    })
}

pub fn overlapping_raw_ids(catalog: &Value) -> Vec<(String, Vec<(String, String)>)> {
    let mut by_raw: HashMap<String, Vec<(String, String)>> = HashMap::new();
    let Some(entries) = catalog.as_array() else {
        return Vec::new();
    };
    for entry in entries {
        let provider = entry["provider_id"].as_str().unwrap_or_default();
        let offering = entry["offering_id"].as_str().unwrap_or_default();
        for alias in catalog_aliases(entry) {
            let raw = alias["upstream_model"]
                .as_str()
                .or_else(|| alias["upstream_model_id"].as_str());
            if let Some(raw) = raw {
                by_raw
                    .entry(raw.to_string())
                    .or_default()
                    .push((provider.to_string(), offering.to_string()));
            }
        }
    }
    by_raw
        .into_iter()
        .filter_map(|(raw, plans)| {
            let mut unique = plans.clone();
            unique.sort();
            unique.dedup();
            if unique.len() > 1 {
                Some((raw, unique))
            } else {
                None
            }
        })
        .collect()
}

pub fn scnet_entries(catalog: &Value) -> Vec<&Value> {
    catalog
        .as_array()
        .into_iter()
        .flatten()
        .filter(|entry| {
            entry["provider_id"] == SCNET_PROVIDER_ID
                || entry["display_name"]
                    .as_str()
                    .is_some_and(|name| name.to_ascii_lowercase().contains("scnet"))
        })
        .collect()
}

pub fn error_type(body: &Value) -> Option<&str> {
    body.pointer("/error/type")
        .and_then(Value::as_str)
        .or_else(|| body.get("type").and_then(Value::as_str))
}

pub fn error_message(body: &Value) -> String {
    if let Some(message) = body.pointer("/error/message").and_then(Value::as_str) {
        return message.to_string();
    }
    match &body["error"] {
        Value::String(message) => message.clone(),
        other => other.to_string(),
    }
}

pub fn json_contains_secret(value: &Value, secret: &str) -> bool {
    if secret.is_empty() {
        return false;
    }
    match value {
        Value::String(text) => text.contains(secret),
        Value::Array(items) => items.iter().any(|item| json_contains_secret(item, secret)),
        Value::Object(map) => map.values().any(|item| json_contains_secret(item, secret)),
        _ => false,
    }
}

pub fn client_model_ids(body: &Value) -> Vec<String> {
    body["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| item["id"].as_str().map(str::to_string))
        .collect()
}

pub fn required_catalog_fields() -> Vec<String> {
    catalog_contract()["required_entry_fields"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|value| value.as_str().map(str::to_string))
        .collect()
}

pub fn risk_notice_fields() -> Vec<String> {
    catalog_contract()["risk_notice_fields"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|value| value.as_str().map(str::to_string))
        .collect()
}

pub fn missing_fields(entry: &Value, fields: &[String]) -> Vec<String> {
    fields
        .iter()
        .filter(|field| entry.get(field.as_str()).is_none() || entry[field.as_str()].is_null())
        .cloned()
        .collect()
}

fn temp_data_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ocg-v2-contract-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

async fn wait_ready(client: &reqwest::Client, port: u16) {
    let url = format!("http://127.0.0.1:{port}/dashboard/api/auth/status");
    for _ in 0..50 {
        if client.get(&url).send().await.is_ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("gateway on port {port} did not become ready");
}

async fn decode_json(response: reqwest::Response) -> Value {
    let text = response.text().await.unwrap_or_default();
    serde_json::from_str(&text).unwrap_or_else(|_| json!({ "raw": text }))
}
