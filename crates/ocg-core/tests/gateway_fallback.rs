use axum::Router;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{Duration, Utc};
use ocg_core::alias;
use ocg_core::crypto::{KeyCipher, StaticKeyCipher};
use ocg_core::db::{Database, ForwardLogQueryOptions};
use ocg_core::gateway;
use ocg_core::gateway::provider_adapter::install_goat_loopback_route_for_test;
use ocg_core::models::{
    Account, AccountUpdate, AppConfig, ForwardLog, ProxyListDirection, ProxyMode, RoutingMode,
};
use ocg_core::provider::{
    COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_ALIAS, COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM,
    COMMAND_CODE_PROVIDER_ID, GO_OFFERING_ID, GOAT_OFFERING_ID, OPENCODE_PROVIDER_ID,
    ZEN_FREE_ACCOUNT_ID,
};
use ocg_core::state::{CoreStateInner, GatewayHandle};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::net::TcpListener as StdTcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration as StdDuration;

#[path = "fixtures/fake_upstream.rs"]
mod fake_upstream;

use fake_upstream::{
    DelayedChunks, FakeCall as MockCall, FakeReply as MockReply, start_delayed_fake_upstream,
    start_fake_upstream, start_raw_disconnect_upstream,
};

const LIMITED_BODY: &str = r#"{"type":"error","error":{"type":"GoUsageLimitError","message":"Weekly usage limit reached. Resets in 3 days."}}"#;
const OPAQUE_ACCOUNT_KEY: &str = "opaque/account+key=42";
const ERROR_BODY_WITH_ECHOED_KEY: &str = r#"{"error":{"message":"provider rejected opaque/account+key=42","detail":"opaque/account+key=42"}}"#;
const SUCCESS_BODY: &str = r#"{"id":"ok","object":"chat.completion","model":"deepseek-v4-flash","choices":[{"index":0,"message":{"role":"assistant","content":"ok"},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":2,"prompt_tokens_details":{"cached_tokens":0}}}"#;
const SUCCESS_BODY_WITHOUT_USAGE: &str = r#"{"id":"ok","object":"chat.completion","model":"deepseek-v4-flash","choices":[{"index":0,"message":{"role":"assistant","content":"ok"},"finish_reason":"stop"}]}"#;
const SUCCESS_BODY_WITH_ECHOED_KEY: &str = r#"{"id":"ok","object":"chat.completion","model":"deepseek-v4-flash","choices":[{"index":0,"message":{"role":"assistant","content":"before opaque/account+key=42 after"},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":2,"prompt_tokens_details":{"cached_tokens":0}}}"#;
const SUCCESS_BODY_WITH_COMMON_KEY: &str = r#"{"id":"ok","object":"chat.completion","model":"deepseek-v4-flash","choices":[{"index":0,"message":{"role":"assistant","content":"before text after"},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":2,"prompt_tokens_details":{"cached_tokens":0}}}"#;
const SUCCESS_BODY_WITH_NESTED_ARGUMENT_KEY: &str = r#"{"id":"ok","object":"chat.completion","model":"deepseek-v4-flash","choices":[{"index":0,"message":{"role":"assistant","content":null,"tool_calls":[{"id":"call_1","type":"function","function":{"name":"run","arguments":"{\"data\":\"safe\",\"token\":\"data\"}"}}]},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":10,"completion_tokens":2,"prompt_tokens_details":{"cached_tokens":0}}}"#;
const RESPONSES_SUCCESS_BODY: &str = r#"{"id":"resp_ok","object":"response","status":"completed","model":"deepseek-v4-flash","output":[{"type":"message","id":"msg_1","role":"assistant","status":"completed","content":[{"type":"output_text","text":"ok","annotations":[]}]}],"usage":{"input_tokens":10,"output_tokens":2,"input_tokens_details":{"cached_tokens":0}}}"#;
const MESSAGES_SUCCESS_BODY: &str = r#"{"id":"msg-ok","type":"message","role":"assistant","model":"minimax-m2.7","content":[{"type":"text","text":"ok"}],"stop_reason":"end_turn","stop_sequence":null,"usage":{"input_tokens":10,"output_tokens":2,"cache_read_input_tokens":0}}"#;
const MESSAGES_SUCCESS_BODY_WITH_ECHOED_KEY_IN_THINKING: &str = r#"{"id":"msg-ok","type":"message","role":"assistant","model":"minimax-m2.7","content":[{"type":"thinking","thinking":"opaque/account+key=42","signature":"sig_123"},{"type":"text","text":"ok"}],"stop_reason":"end_turn","stop_sequence":null,"usage":{"input_tokens":10,"output_tokens":2,"cache_read_input_tokens":0}}"#;
const CHAT_STREAM_BODY: &str = concat!(
    "data: {\"id\":\"chat-stream\",\"model\":\"deepseek-v4-flash\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"ok\"},\"finish_reason\":null}]}\n\n",
    "data: {\"id\":\"chat-stream\",\"model\":\"deepseek-v4-flash\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":2,\"prompt_tokens_details\":{\"cached_tokens\":0}}}\n\n",
    "data: [DONE]\n\n"
);
const CHAT_STREAM_WITHOUT_USAGE: &str = concat!(
    "data: {\"id\":\"chat-stream\",\"model\":\"deepseek-v4-flash\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"ok\"},\"finish_reason\":null}]}\n\n",
    "data: {\"id\":\"chat-stream\",\"model\":\"deepseek-v4-flash\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
    "data: [DONE]\n\n"
);
const CHAT_STREAM_WITH_UNTERMINATED_KEY_TAIL: &str = concat!(
    "data: {\"id\":\"chat-stream\",\"model\":\"deepseek-v4-flash\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"ok\"},\"finish_reason\":null}]}\n\n",
    "data: {\"id\":\"chat-stream\",\"model\":\"deepseek-v4-flash\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"opaque/account+key=42\"},\"finish_reason\":\"stop\"}]}"
);
const CHAT_STREAM_WITH_SPLIT_ECHOED_KEY: &str = concat!(
    "data: {\"id\":\"chat-stream\",\"model\":\"deepseek-v4-flash\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"before opaque/account+\"},\"finish_reason\":null}]}\n\n",
    "data: {\"id\":\"chat-stream\",\"model\":\"deepseek-v4-flash\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"key=42 after\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":2,\"prompt_tokens_details\":{\"cached_tokens\":0}}}\n\n",
    "data: [DONE]\n\n"
);
const CHAT_STREAM_WITH_COMMON_KEY: &str = concat!(
    "data: {\"id\":\"chat-stream\",\"object\":\"chat.completion.chunk\",\"model\":\"deepseek-v4-flash\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"before text after\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":2,\"prompt_tokens_details\":{\"cached_tokens\":0}}}\n\n",
    "data: [DONE]\n\n"
);
const RESPONSES_STREAM_BODY: &str = concat!(
    "event: response.created\ndata: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_stream\",\"model\":\"deepseek-v4-flash\",\"status\":\"in_progress\"}}\n\n",
    "event: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"content_index\":0,\"delta\":\"ok\"}\n\n",
    "event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_stream\",\"model\":\"deepseek-v4-flash\",\"status\":\"completed\",\"usage\":{\"input_tokens\":10,\"output_tokens\":2,\"input_tokens_details\":{\"cached_tokens\":0}}}}\n\n"
);
const MESSAGES_STREAM_BODY: &str = concat!(
    "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg-stream\",\"model\":\"minimax-m2.7\",\"usage\":{\"input_tokens\":6,\"cache_read_input_tokens\":4}}}\n\n",
    "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
    "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"ok\"}}\n\n",
    "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
    "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":2}}\n\n",
    "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n"
);
const MESSAGES_STREAM_HEAD: &str = concat!(
    "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg-stream\",\"model\":\"minimax-m2.7\",\"usage\":{\"input_tokens\":6,\"cache_read_input_tokens\":4}}}\n\n",
    "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
    "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"ok\"}}\n\n"
);
const MESSAGES_STREAM_TAIL: &str = concat!(
    "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
    "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":2}}\n\n",
    "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n"
);
fn temp_data_dir(label: &str) -> PathBuf {
    let mut dir = std::env::temp_dir();
    dir.push(format!(
        "ocg-gateway-test-{}-{}",
        label,
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn free_port() -> u16 {
    let listener = StdTcpListener::bind(("127.0.0.1", 0)).unwrap();
    listener.local_addr().unwrap().port()
}

async fn start_mock_upstream(
    replies: HashMap<String, VecDeque<MockReply>>,
) -> (
    String,
    Arc<Mutex<Vec<MockCall>>>,
    tokio::sync::oneshot::Sender<()>,
) {
    start_fake_upstream(replies).await
}

async fn start_delayed_messages_upstream(
    content_type: &'static str,
    chunks: DelayedChunks,
) -> (String, Arc<AtomicUsize>, tokio::sync::oneshot::Sender<()>) {
    start_delayed_upstream(StatusCode::OK, content_type, chunks).await
}

async fn start_delayed_upstream(
    status: StatusCode,
    content_type: &'static str,
    chunks: DelayedChunks,
) -> (String, Arc<AtomicUsize>, tokio::sync::oneshot::Sender<()>) {
    start_sequenced_delayed_upstream(status, content_type, vec![chunks]).await
}

async fn start_sequenced_delayed_upstream(
    status: StatusCode,
    content_type: &'static str,
    responses: Vec<DelayedChunks>,
) -> (String, Arc<AtomicUsize>, tokio::sync::oneshot::Sender<()>) {
    start_delayed_fake_upstream(status, content_type, responses).await
}

#[tokio::test]
async fn fake_upstream_captures_protocol_auth_and_scripts_status_streams() {
    let replies = HashMap::from([
        (
            "chat-key".to_owned(),
            VecDeque::from([MockReply {
                status: StatusCode::UNAUTHORIZED.as_u16(),
                body: r#"{"error":"unauthorized"}"#,
            }]),
        ),
        (
            "responses-key".to_owned(),
            VecDeque::from([MockReply {
                status: StatusCode::FORBIDDEN.as_u16(),
                body: r#"{"error":"forbidden"}"#,
            }]),
        ),
        (
            "messages-key".to_owned(),
            VecDeque::from([MockReply {
                status: StatusCode::TOO_MANY_REQUESTS.as_u16(),
                body: r#"{"error":"rate limited"}"#,
            }]),
        ),
        (
            "gemini-key".to_owned(),
            VecDeque::from([MockReply {
                status: StatusCode::OK.as_u16(),
                body: "data: {\"usageMetadata\":{\"promptTokenCount\":1}}\n\n",
            }]),
        ),
        (
            String::new(),
            VecDeque::from([MockReply {
                status: StatusCode::OK.as_u16(),
                body: r#"{"ok":true}"#,
            }]),
        ),
    ]);
    let (base_url, calls, stop_fake) = start_mock_upstream(replies).await;
    let client = loopback_client();

    assert_eq!(
        client
            .post(format!("{base_url}/v1/chat/completions"))
            .header(reqwest::header::AUTHORIZATION, "Bearer chat-key")
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        client
            .post(format!("{base_url}/v1/responses"))
            .header(reqwest::header::AUTHORIZATION, "Bearer responses-key")
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        client
            .post(format!("{base_url}/v1/messages"))
            .header("x-api-key", "messages-key")
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::TOO_MANY_REQUESTS
    );
    let gemini = client
        .post(format!(
            "{base_url}/v1beta/models/fake:streamGenerateContent"
        ))
        .header("x-goog-api-key", "gemini-key")
        .send()
        .await
        .unwrap();
    assert_eq!(gemini.status(), StatusCode::OK);
    assert!(gemini.text().await.unwrap().contains("usageMetadata"));
    assert_eq!(
        client
            .post(format!("{base_url}/zen/free"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );

    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 5);
    assert_eq!(calls[0].path, "/v1/chat/completions");
    assert_eq!(calls[1].path, "/v1/responses");
    assert_eq!(calls[2].x_api_key.as_deref(), Some("messages-key"));
    assert_eq!(calls[3].path, "/v1beta/models/fake:streamGenerateContent");
    assert_eq!(calls[3].x_goog_api_key.as_deref(), Some("gemini-key"));
    assert_eq!(calls[4].method, axum::http::Method::POST);
    assert!(calls[4].authorization.is_none());
    assert!(calls[4].x_api_key.is_none());
    assert!(calls[4].x_goog_api_key.is_none());
    drop(calls);
    let _ = stop_fake.send(());
}

fn build_state(base_url: String, keys: &[&str]) -> (Arc<CoreStateInner>, PathBuf) {
    build_state_with_routing(base_url, keys, RoutingMode::StrictPriority, false)
}

/// Every request in this suite targets loopback listeners; never route them
/// through an ambient system/environment proxy (which aborts such
/// connections on some machines).
fn loopback_client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("test client should build")
}

fn build_state_with_routing(
    base_url: String,
    keys: &[&str],
    routing_mode: RoutingMode,
    conversation_sticky: bool,
) -> (Arc<CoreStateInner>, PathBuf) {
    let dir = temp_data_dir("state");
    let cipher: Arc<dyn KeyCipher + Send + Sync> = Arc::new(StaticKeyCipher::new("test"));
    let db = Database::open(dir.clone()).unwrap();
    let state = Arc::new(CoreStateInner::new(db, dir.clone(), cipher).unwrap());
    let mut config = state.config();
    // Pin the primary key value for the test requests. The mock upstream is
    // loopback: never route test traffic through an ambient proxy.
    config.gateway_key = "gw-test".into();
    config.upstream_base_url = base_url;
    config.proxy_mode = ProxyMode::Direct;
    config.routing_mode = routing_mode;
    config.conversation_sticky = conversation_sticky;
    state.set_config(config).unwrap();

    let now = Utc::now();
    for (idx, key) in keys.iter().enumerate() {
        let account = Account {
            id: format!("acct-{}", idx + 1),
            provider_id: ocg_core::provider::default_provider_id(),
            offering_id: ocg_core::provider::default_offering_id(),
            credential_kind: ocg_core::provider::default_credential_kind(),
            quota_scope: ocg_core::provider::default_quota_scope(),
            name: format!("acct-{}", idx + 1),
            username: None,
            password_cipher: None,
            key_cipher: state.encrypt_key(key).unwrap(),
            enabled: true,
            account_type: ocg_core::models::AccountType::Key,
            setup_step: ocg_core::models::AccountSetupStep::Ready,
            referral_code: None,
            purchase_date: String::new(),
            expires_on: String::new(),
            cooldown_until: None,
            cooldown_generic_until: None,
            cooldown_5h_until: None,
            cooldown_week_until: None,
            cooldown_month_until: None,
            cooldown_free_until: None,
            last_error: None,
            auth_error: None,
            notes: None,
            created_at: now + chrono::Duration::seconds(idx as i64),
            updated_at: now + chrono::Duration::seconds(idx as i64),
        };
        state.db.lock().create_account(&account).unwrap();
    }

    (state, dir)
}

async fn start_gateway(state: Arc<CoreStateInner>) -> (u16, GatewayHandle) {
    let port = free_port();
    let handle = gateway::start_gateway(state, port).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    (port, handle)
}

async fn chat(port: u16) -> (u16, String) {
    chat_with_conversation(port, None, "ping").await
}

async fn chat_with_conversation(
    port: u16,
    conversation_id: Option<&str>,
    user: &str,
) -> (u16, String) {
    let request = loopback_client()
        .post(format!("http://127.0.0.1:{}/v1/chat/completions", port))
        .header(reqwest::header::AUTHORIZATION, "Bearer gw-test")
        .header(reqwest::header::ACCEPT_ENCODING, "gzip")
        .json(&serde_json::json!({
            "model": "deepseek-v4-flash",
            "messages": [{"role": "user", "content": user}],
            "max_tokens": 3,
            "stream": false
        }));
    let request = if let Some(conversation_id) = conversation_id {
        request.header("x-ocg-conversation-id", conversation_id)
    } else {
        request
    };
    let response = request.send().await.unwrap();
    let status = response.status().as_u16();
    let body = response.text().await.unwrap();
    (status, body)
}

fn set_account_enabled(state: &Arc<CoreStateInner>, account_id: &str, enabled: bool) {
    state
        .db
        .lock()
        .update_account(
            account_id,
            &AccountUpdate {
                name: None,
                username: None,
                password: None,
                key: None,
                enabled: Some(enabled),
                referral_code: None,
                purchase_date: None,
                notes: None,
            },
            None,
            None,
        )
        .unwrap();
}

fn create_goat_account(
    state: &Arc<CoreStateInner>,
    source_account_id: &str,
    account_id: &str,
    key: &str,
) {
    let mut account = state
        .db
        .lock()
        .get_account(source_account_id)
        .unwrap()
        .expect("source account");
    account.id = account_id.to_string();
    account.provider_id = COMMAND_CODE_PROVIDER_ID.to_string();
    account.offering_id = GOAT_OFFERING_ID.to_string();
    account.name = account_id.to_string();
    account.key_cipher = state.encrypt_key(key).unwrap();
    account.cooldown_until = None;
    account.cooldown_generic_until = None;
    account.cooldown_5h_until = None;
    account.cooldown_week_until = None;
    account.cooldown_month_until = None;
    account.cooldown_free_until = None;
    account.auth_error = None;
    account.created_at = Utc::now();
    account.updated_at = account.created_at;
    account.enabled = false;
    state.db.lock().create_account(&account).unwrap();
    force_enable_unroutable_account_for_loopback_test(&state.data_dir, &account.id);
    assert!(
        state
            .db
            .lock()
            .get_account(&account.id)
            .unwrap()
            .unwrap()
            .enabled,
        "loopback GOAT fixture must be enabled in the already-open database"
    );
}

fn persist_goat_verified_catalog(state: &Arc<CoreStateInner>, account_id: &str, models: &[&str]) {
    let models: Vec<String> = models.iter().map(|model| (*model).to_string()).collect();
    let db = state.db.lock();
    let contract = db
        .capture_goat_verification_contract(account_id)
        .unwrap()
        .expect("GOAT verification contract");
    assert!(
        db.commit_goat_verification_if_contract_matches(
            &contract,
            ocg_core::provider::ConnectionVerificationStatus::Verified,
            Some(Utc::now()),
            None,
            Some(models.as_slice()),
        )
        .unwrap()
    );
    drop(db);
    state.reload_provider_contracts().unwrap();
}

/// Integration-test-only SQLite poke. Production `ocg-core` rlibs have no
/// persistent enablement bypass; a later `Database::open` sanitizes these rows.
fn force_enable_unroutable_account_for_loopback_test(data_dir: &Path, account_id: &str) {
    let conn = rusqlite::Connection::open(data_dir.join("data.sqlite"))
        .expect("loopback test sqlite should open");
    conn.busy_timeout(StdDuration::from_millis(5_000))
        .expect("loopback test sqlite should set busy timeout");
    let changed = conn
        .execute(
            "UPDATE accounts SET enabled = 1 WHERE id = ?1",
            [account_id],
        )
        .expect("loopback test enable poke should execute");
    assert_eq!(changed, 1, "loopback test account {account_id} must exist");
}

async fn gemini_call(port: u16, model: &str) -> (StatusCode, serde_json::Value) {
    let response = loopback_client()
        .post(format!(
            "http://127.0.0.1:{port}/v1beta/models/{model}:generateContent"
        ))
        .header("x-goog-api-key", "gw-test")
        .json(&serde_json::json!({
            "contents": [{"role": "user", "parts": [{"text": "ping"}]}]
        }))
        .send()
        .await
        .unwrap();
    let status = response.status();
    let body = response.json().await.unwrap();
    (status, body)
}

async fn models(port: u16) -> (StatusCode, String) {
    let response = loopback_client()
        .get(format!("http://127.0.0.1:{port}/v1/models"))
        .header(reqwest::header::AUTHORIZATION, "Bearer gw-test")
        .send()
        .await
        .unwrap();
    let status = response.status();
    let body = response.text().await.unwrap();
    (status, body)
}

async fn protocol_call(port: u16, path: &str, model: &str) -> (StatusCode, serde_json::Value) {
    let body = match path {
        "/v1/chat/completions" => serde_json::json!({
            "model": model,
            "messages": [{"role": "user", "content": "ping"}],
            "max_tokens": 3,
            "stream": false
        }),
        "/v1/responses" => serde_json::json!({
            "model": model,
            "input": "ping",
            "store": false,
            "max_output_tokens": 3,
            "stream": false
        }),
        "/v1/messages" => serde_json::json!({
            "model": model,
            "messages": [{"role": "user", "content": "ping"}],
            "max_tokens": 3,
            "stream": false
        }),
        _ => panic!("unsupported test path: {path}"),
    };
    let client = loopback_client();
    let request = client
        .post(format!("http://127.0.0.1:{port}{path}"))
        .json(&body);
    let request = if path == "/v1/messages" {
        request
            .header("x-api-key", "gw-test")
            .header("anthropic-version", "2023-06-01")
    } else {
        request.header(reqwest::header::AUTHORIZATION, "Bearer gw-test")
    };
    let response = request.send().await.unwrap();
    let status = response.status();
    assert!(
        response
            .headers()
            .get("x-ocg-request-id")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.starts_with("ocg-")),
        "{path} should return a request id"
    );
    let body = response.json().await.unwrap();
    (status, body)
}

async fn protocol_stream_call(port: u16, path: &str, model: &str) -> (StatusCode, String) {
    let body = match path {
        "/v1/chat/completions" => serde_json::json!({
            "model": model,
            "messages": [{"role": "user", "content": "ping"}],
            "max_tokens": 3,
            "stream": true
        }),
        "/v1/responses" => serde_json::json!({
            "model": model,
            "input": "ping",
            "store": false,
            "max_output_tokens": 3,
            "stream": true
        }),
        "/v1/messages" => serde_json::json!({
            "model": model,
            "messages": [{"role": "user", "content": "ping"}],
            "max_tokens": 3,
            "stream": true
        }),
        _ => panic!("unsupported test path: {path}"),
    };
    let client = loopback_client();
    let request = client
        .post(format!("http://127.0.0.1:{port}{path}"))
        .json(&body);
    let request = if path == "/v1/messages" {
        request.header("x-api-key", "gw-test")
    } else {
        request.header(reqwest::header::AUTHORIZATION, "Bearer gw-test")
    };
    let response = request.send().await.unwrap();
    let status = response.status();
    assert!(
        response
            .headers()
            .get("x-ocg-request-id")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.starts_with("ocg-")),
        "{path} should return a request id"
    );
    let body = response.text().await.unwrap();
    (status, body)
}

fn chat_stream_text(body: &str) -> String {
    body.split("\n\n")
        .filter_map(|frame| frame.lines().find_map(|line| line.strip_prefix("data: ")))
        .filter(|payload| *payload != "[DONE]")
        .filter_map(|payload| serde_json::from_str::<serde_json::Value>(payload).ok())
        .filter_map(|value| {
            value
                .pointer("/choices/0/delta/content")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        })
        .collect()
}

fn assert_local_openai_alias_list(body: &str) {
    let payload: serde_json::Value = serde_json::from_str(body).unwrap();
    assert_eq!(payload["object"], "list", "{body}");
    let expected = ocg_core::alias::published_routeable_aliases();
    let data = payload["data"].as_array().expect("OpenAI list data");
    assert_eq!(data.len(), expected.len(), "{body}");
    for (item, published) in data.iter().zip(&expected) {
        assert_eq!(item["id"], published.alias);
        assert_eq!(item["object"], "model");
        assert_eq!(item["owned_by"], published.owned_by);
        assert!(!published.alias.contains('/'));
    }
    assert!(!body.contains(COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM));
}

#[tokio::test]
async fn model_discovery_returns_local_list_with_zero_accounts() {
    let replies = HashMap::from([(
        "key-1".to_string(),
        VecDeque::from([MockReply {
            status: 200,
            body: r#"{"object":"list","data":[{"id":"deepseek/deepseek-v4-flash"}]}"#,
        }]),
    )]);
    let (base_url, calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state(base_url, &[]);
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    let unauthorized = loopback_client()
        .get(format!("http://127.0.0.1:{port}/v1/models"))
        .send()
        .await
        .unwrap();
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

    let (status, body) = models(port).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_local_openai_alias_list(&body);
    assert!(
        calls.lock().unwrap().is_empty(),
        "GET /v1/models must not call upstream: {:?}",
        calls.lock().unwrap()
    );
    assert!(state.db.lock().list_forward_logs(10).unwrap().is_empty());

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn model_discovery_does_not_create_inference_logs() {
    let replies = HashMap::from([(
        "key-1".to_string(),
        VecDeque::from([MockReply {
            status: 200,
            body: r#"{"object":"list","data":[{"id":"deepseek/deepseek-v4-flash"},{"id":"vendor-raw-not-an-alias"}]}"#,
        }]),
    )]);
    let (base_url, calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state(base_url, &["key-1"]);
    let before = state.db.lock().get_account("acct-1").unwrap().unwrap();
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    let (status, body) = models(port).await;
    assert_eq!(status, StatusCode::OK);
    assert_local_openai_alias_list(&body);
    assert!(
        body.contains("mimo-v2.5-free") && body.contains("nemotron-3-ultra"),
        "saved Zen Free models and stripped aliases must appear in the local Alias list: {body}"
    );
    assert!(
        calls.lock().unwrap().is_empty(),
        "GET /v1/models must not call upstream: {:?}",
        calls.lock().unwrap()
    );
    let logs = state
        .db
        .lock()
        .query_forward_logs(ForwardLogQueryOptions {
            limit: 10,
            offset: 0,
            status: None,
            account_id: None,
            provider_id: None,
            offering_id: None,
            route_account_id: None,
            credential_account_id: None,
            model: None,
            key_id: None,
            request_id: None,
            start_time: None,
            end_time: None,
            sort_by: None,
            sort_order: None,
        })
        .unwrap();
    assert!(logs.items.is_empty());
    assert_eq!(logs.summary.total_requests, 0);
    let after = state.db.lock().get_account("acct-1").unwrap().unwrap();
    assert_eq!(after.cooldown_until, before.cooldown_until);
    assert_eq!(after.last_error, before.last_error);
    assert_eq!(after.auth_error, before.auth_error);
    assert_eq!(after.updated_at, before.updated_at);

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

fn expected_local_application_models(state: &Arc<CoreStateInner>) -> Vec<String> {
    let priced = state
        .pricing_snapshot()
        .models
        .iter()
        .map(|model| model.model_id.clone())
        .collect::<HashSet<_>>();
    alias::routeable_aliases_for(OPENCODE_PROVIDER_ID, GO_OFFERING_ID)
        .into_iter()
        .filter(|alias| {
            priced.contains(alias)
                || alias
                    .strip_suffix("-highspeed")
                    .is_some_and(|base| priced.contains(base))
        })
        .collect()
}

async fn get_application_models(port: u16) -> (StatusCode, serde_json::Value) {
    let response = loopback_client()
        .get(format!(
            "http://127.0.0.1:{port}/dashboard/api/v3/application-models"
        ))
        .send()
        .await
        .unwrap();
    let status = response.status();
    let body = response.json::<serde_json::Value>().await.unwrap();
    let models = body.get("models").cloned().unwrap_or(body);
    (status, models)
}

fn assert_no_application_model_side_effects(
    state: &Arc<CoreStateInner>,
    calls: &Arc<Mutex<Vec<MockCall>>>,
    before: Option<&Account>,
    routing_before: &str,
) {
    assert!(
        calls.lock().unwrap().is_empty(),
        "GET /application-models must not call upstream: {:?}",
        calls.lock().unwrap()
    );
    assert!(state.db.lock().list_forward_logs(10).unwrap().is_empty());
    assert_eq!(format!("{:?}", state.routing), routing_before);
    if let Some(before) = before {
        let after = state.db.lock().get_account(&before.id).unwrap().unwrap();
        assert_eq!(after.cooldown_until, before.cooldown_until);
        assert_eq!(after.last_error, before.last_error);
        assert_eq!(after.auth_error, before.auth_error);
        assert_eq!(after.updated_at, before.updated_at);
    }
}

#[tokio::test]
async fn application_models_is_local_with_zero_accounts() {
    let replies = HashMap::from([(
        "key-1".to_string(),
        VecDeque::from([MockReply {
            status: 200,
            body: r#"{"object":"list","data":[{"id":"deepseek/deepseek-v4-flash"},{"id":"vendor-raw-not-an-alias"}]}"#,
        }]),
    )]);
    let (base_url, calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state(base_url, &[]);
    let routing_before = format!("{:?}", state.routing);
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    let (status, body) = get_application_models(port).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let ids = body
        .as_array()
        .expect("application-models must be a JSON array")
        .iter()
        .map(|item| item.as_str().expect("alias string").to_string())
        .collect::<Vec<_>>();
    assert_eq!(ids, expected_local_application_models(&state));
    assert!(ids.contains(&"deepseek-v4-flash".to_string()));
    assert!(ids.contains(&"minimax-m2.7-highspeed".to_string()));
    assert!(!ids.iter().any(|id| id.contains('/')));
    assert!(
        !ids.iter()
            .any(|id| *id == COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM)
    );
    assert!(!ids.iter().any(|id| id.ends_with("-free")));
    assert_no_application_model_side_effects(&state, &calls, None, &routing_before);

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn application_models_does_not_select_accounts_or_hit_upstream() {
    let replies = HashMap::from([(
        "key-1".to_string(),
        VecDeque::from([MockReply {
            status: 429,
            body: LIMITED_BODY,
        }]),
    )]);
    let (base_url, calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state(base_url, &["key-1"]);
    let before = state.db.lock().get_account("acct-1").unwrap().unwrap();
    let routing_before = format!("{:?}", state.routing);
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    let (status, body) = get_application_models(port).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body,
        serde_json::to_value(expected_local_application_models(&state)).unwrap()
    );
    assert_no_application_model_side_effects(&state, &calls, Some(&before), &routing_before);

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn application_models_intersects_priced_go_aliases_in_registry_order() {
    let replies = HashMap::from([(
        "key-1".to_string(),
        VecDeque::from([MockReply {
            status: 200,
            body: r#"{"object":"list","data":[{"id":"unknown"},{"id":"grok-4.5"},{"id":"kimi-k3"},{"id":"glm-5.1"},{"id":"minimax-m2.7-highspeed"},{"id":"minimax-m2.7"},{"id":"deepseek-v4-flash"},{"id":"minimax-m2.7"},{"id":"qwen3.7-plus"}]}"#,
        }]),
    )]);
    let (base_url, calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state(base_url, &["key-1"]);
    let mut pricing = state.pricing_snapshot().as_ref().clone();
    pricing.models.retain(|model| {
        matches!(
            model.model_id.as_str(),
            "grok-4.5" | "kimi-k3" | "minimax-m2.7" | "glm-5.1"
        )
    });
    pricing.revision = format!("test-priced-models-{}", Utc::now().timestamp_micros());
    pricing.activated_at = Utc::now().to_rfc3339();
    state.activate_pricing_snapshot(pricing).unwrap();
    let before = state.db.lock().get_account("acct-1").unwrap().unwrap();
    let routing_before = format!("{:?}", state.routing);
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    let (status, body) = get_application_models(port).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body,
        serde_json::json!([
            "glm-5.1",
            "grok-4.5",
            "kimi-k3",
            "minimax-m2.7",
            "minimax-m2.7-highspeed"
        ])
    );
    assert_eq!(
        body.as_array()
            .unwrap()
            .iter()
            .filter_map(|item| item.as_str())
            .collect::<Vec<_>>(),
        expected_local_application_models(&state)
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
    );
    assert_no_application_model_side_effects(&state, &calls, Some(&before), &routing_before);

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn application_models_empty_intersection_returns_empty_list() {
    let replies = HashMap::from([(
        "key-1".to_string(),
        VecDeque::from([MockReply {
            status: 500,
            body: r#"{"error":"upstream unavailable"}"#,
        }]),
    )]);
    let (base_url, calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state(base_url, &["key-1"]);
    let mut empty = state.pricing_snapshot().as_ref().clone();
    let mut raw_row = empty.models[0].clone();
    raw_row.model_id = "vendor-raw-not-an-alias".into();
    empty.models.clear();
    empty.revision = format!("test-empty-pricing-{}", Utc::now().timestamp_micros());
    empty.activated_at = Utc::now().to_rfc3339();
    state.activate_pricing_snapshot(empty).unwrap();
    let before = state.db.lock().get_account("acct-1").unwrap().unwrap();
    let routing_before = format!("{:?}", state.routing);
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    let (status, body) = get_application_models(port).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body, serde_json::json!([]));
    assert_no_application_model_side_effects(&state, &calls, Some(&before), &routing_before);

    let mut disjoint = state.pricing_snapshot().as_ref().clone();
    disjoint.models = vec![raw_row];
    disjoint.revision = format!("test-disjoint-pricing-{}", Utc::now().timestamp_micros());
    disjoint.activated_at = Utc::now().to_rfc3339();
    state.activate_pricing_snapshot(disjoint).unwrap();

    let (status, body) = get_application_models(port).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body, serde_json::json!([]));
    assert!(calls.lock().unwrap().is_empty());

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn routes_all_client_formats_to_each_models_native_protocol() {
    struct Case {
        client_path: &'static str,
        model: &'static str,
        upstream_path: &'static str,
        upstream_body: &'static str,
    }

    let cases = [
        Case {
            client_path: "/v1/chat/completions",
            model: "deepseek-v4-flash",
            upstream_path: "/v1/chat/completions",
            upstream_body: SUCCESS_BODY,
        },
        Case {
            client_path: "/v1/chat/completions",
            model: "minimax-m2.7",
            upstream_path: "/v1/chat/completions",
            upstream_body: SUCCESS_BODY,
        },
        Case {
            client_path: "/v1/responses",
            model: "deepseek-v4-flash",
            upstream_path: "/v1/responses",
            upstream_body: RESPONSES_SUCCESS_BODY,
        },
        Case {
            client_path: "/v1/responses",
            model: "hy3",
            upstream_path: "/v1/chat/completions",
            upstream_body: SUCCESS_BODY,
        },
        Case {
            client_path: "/v1/responses",
            model: "glm-5.2",
            upstream_path: "/v1/responses",
            upstream_body: RESPONSES_SUCCESS_BODY,
        },
        Case {
            client_path: "/v1/responses",
            model: "minimax-m2.7",
            upstream_path: "/v1/messages",
            upstream_body: MESSAGES_SUCCESS_BODY,
        },
        Case {
            client_path: "/v1/messages",
            model: "deepseek-v4-flash",
            upstream_path: "/v1/messages",
            upstream_body: MESSAGES_SUCCESS_BODY,
        },
        Case {
            client_path: "/v1/messages",
            model: "hy3",
            upstream_path: "/v1/chat/completions",
            upstream_body: SUCCESS_BODY,
        },
        Case {
            client_path: "/v1/messages",
            model: "minimax-m2.7",
            upstream_path: "/v1/messages",
            upstream_body: MESSAGES_SUCCESS_BODY,
        },
        Case {
            client_path: "/v1/messages",
            model: "glm-5.2",
            upstream_path: "/v1/messages",
            upstream_body: MESSAGES_SUCCESS_BODY,
        },
    ];

    for case in cases {
        let replies = HashMap::from([(
            "key-1".to_string(),
            VecDeque::from([MockReply {
                status: 200,
                body: case.upstream_body,
            }]),
        )]);
        let (base_url, calls, stop_mock) = start_mock_upstream(replies).await;
        let (state, dir) = build_state(base_url, &["key-1"]);
        let (port, gateway_handle) = start_gateway(state.clone()).await;

        let (status, response) = protocol_call(port, case.client_path, case.model).await;
        assert_eq!(
            status,
            StatusCode::OK,
            "{} {}",
            case.client_path,
            case.model
        );

        let call = calls.lock().unwrap()[0].clone();
        assert_eq!(call.path, case.upstream_path);
        if case.upstream_path == "/v1/messages" {
            assert_eq!(call.x_api_key.as_deref(), Some("key-1"));
            assert!(call.authorization.is_none());
            assert_eq!(call.anthropic_version.as_deref(), Some("2023-06-01"));
        } else {
            assert_eq!(call.authorization.as_deref(), Some("Bearer key-1"));
            assert!(call.x_api_key.is_none());
            assert!(call.anthropic_version.is_none());
        }
        let upstream_request: serde_json::Value = serde_json::from_str(&call.body).unwrap();
        assert_eq!(upstream_request["model"], case.model);
        match case.upstream_path {
            "/v1/responses" => {
                assert!(
                    upstream_request.get("input").is_some(),
                    "Responses upstream should keep input: {}",
                    call.body
                );
                assert!(upstream_request.get("messages").is_none());
            }
            _ => assert!(upstream_request["messages"].is_array()),
        }

        match case.client_path {
            "/v1/chat/completions" => {
                assert_eq!(response["object"], "chat.completion");
                assert_eq!(response["choices"][0]["message"]["content"], "ok");
            }
            "/v1/responses" => {
                assert_eq!(response["object"], "response");
                assert_eq!(response["output"][0]["content"][0]["text"], "ok");
            }
            "/v1/messages" => {
                assert_eq!(response["type"], "message");
                assert_eq!(response["content"][0]["text"], "ok");
            }
            _ => unreachable!(),
        }
        let log = state.db.lock().list_forward_logs(1).unwrap().remove(0);
        assert_eq!((log.prompt_tokens, log.completion_tokens), (10, 2));
        assert_eq!(log.status, "success");
        assert_eq!(log.cost_state, "priced");
        assert!(log.cost.is_some());
        assert!(log.pricing_revision_id.is_some());
        assert!(
            log.request_id
                .as_deref()
                .is_some_and(|id| id.starts_with("ocg-"))
        );
        assert_eq!(log.attempt, Some(1));
        assert!(log.error_source.is_none());
        assert!(log.error_stage.is_none());
        assert!(log.diagnostic.is_none());

        gateway::stop_gateway(gateway_handle);
        let _ = stop_mock.send(());
        let _ = fs::remove_dir_all(dir);
    }
}

#[tokio::test]
async fn successful_inference_never_echoes_the_selected_account_key() {
    for client_path in ["/v1/chat/completions", "/v1/responses", "/v1/messages"] {
        let replies = HashMap::from([(
            OPAQUE_ACCOUNT_KEY.to_string(),
            VecDeque::from([MockReply {
                status: 200,
                body: SUCCESS_BODY_WITH_ECHOED_KEY,
            }]),
        )]);
        let (base_url, _calls, stop_mock) = start_mock_upstream(replies).await;
        let (state, dir) = build_state(base_url, &[OPAQUE_ACCOUNT_KEY]);
        let (port, gateway_handle) = start_gateway(state).await;

        let (status, response) = protocol_call(port, client_path, "hy3").await;
        assert_eq!(status, StatusCode::OK, "{client_path}: {response}");
        assert!(
            !response.to_string().contains(OPAQUE_ACCOUNT_KEY),
            "{client_path} leaked the selected account Key: {response}"
        );

        gateway::stop_gateway(gateway_handle);
        let _ = stop_mock.send(());
        let _ = fs::remove_dir_all(dir);
    }
}

#[tokio::test]
async fn common_short_key_redaction_preserves_non_stream_protocol_discriminators() {
    for client_path in ["/v1/chat/completions", "/v1/responses", "/v1/messages"] {
        let replies = HashMap::from([(
            "text".to_string(),
            VecDeque::from([MockReply {
                status: 200,
                body: SUCCESS_BODY_WITH_COMMON_KEY,
            }]),
        )]);
        let (base_url, _calls, stop_mock) = start_mock_upstream(replies).await;
        let (state, dir) = build_state(base_url, &["text"]);
        let (port, gateway_handle) = start_gateway(state).await;

        let (status, response) = protocol_call(port, client_path, "hy3").await;
        assert_eq!(status, StatusCode::OK, "{client_path}: {response}");
        let content = match client_path {
            "/v1/chat/completions" => {
                assert_eq!(response["object"], "chat.completion");
                response["choices"][0]["message"]["content"].as_str()
            }
            "/v1/responses" => {
                assert_eq!(response["object"], "response");
                assert_eq!(response["output"][0]["type"], "message");
                assert_eq!(response["output"][0]["content"][0]["type"], "output_text");
                response["output"][0]["content"][0]["text"].as_str()
            }
            "/v1/messages" => {
                assert_eq!(response["type"], "message");
                assert_eq!(response["content"][0]["type"], "text");
                response["content"][0]["text"].as_str()
            }
            _ => unreachable!(),
        };
        assert_eq!(content, Some("before <redacted> after"), "{response}");

        gateway::stop_gateway(gateway_handle);
        let _ = stop_mock.send(());
        let _ = fs::remove_dir_all(dir);
    }
}

#[tokio::test]
async fn non_stream_tool_argument_redaction_preserves_nested_json_keys() {
    let replies = HashMap::from([(
        "data".to_string(),
        VecDeque::from([MockReply {
            status: 200,
            body: SUCCESS_BODY_WITH_NESTED_ARGUMENT_KEY,
        }]),
    )]);
    let (base_url, _calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state(base_url, &["data"]);
    let (port, gateway_handle) = start_gateway(state).await;

    let (status, response) = protocol_call(port, "/v1/chat/completions", "deepseek-v4-flash").await;
    assert_eq!(status, StatusCode::OK, "{response}");
    let arguments = response["choices"][0]["message"]["tool_calls"][0]["function"]["arguments"]
        .as_str()
        .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(arguments).unwrap(),
        serde_json::json!({"data":"safe","token":"<redacted>"})
    );

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn successful_conversion_redacts_a_key_before_opaque_reasoning_replay_encoding() {
    let replies = HashMap::from([(
        OPAQUE_ACCOUNT_KEY.to_string(),
        VecDeque::from([MockReply {
            status: 200,
            body: MESSAGES_SUCCESS_BODY_WITH_ECHOED_KEY_IN_THINKING,
        }]),
    )]);
    let (base_url, _calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state(base_url, &[OPAQUE_ACCOUNT_KEY]);
    let (port, gateway_handle) = start_gateway(state).await;

    let (status, response) = protocol_call(port, "/v1/responses", "minimax-m2.7").await;
    assert_eq!(status, StatusCode::OK, "{response}");
    let encrypted = response["output"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["type"] == "reasoning")
        .and_then(|item| item["encrypted_content"].as_str())
        .expect("converted response should retain a safe reasoning replay block");
    let encoded = encrypted
        .strip_prefix("ocg-anthropic-thinking-v1:")
        .expect("reasoning replay should use the Anthropic envelope");
    let decoded = String::from_utf8(URL_SAFE_NO_PAD.decode(encoded).unwrap()).unwrap();
    assert!(
        !decoded.contains(OPAQUE_ACCOUNT_KEY),
        "opaque replay leaked the selected account Key: {decoded}"
    );
    assert!(decoded.contains("<redacted>"), "{decoded}");

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn streamed_inference_redacts_a_selected_key_split_across_events() {
    for client_path in ["/v1/chat/completions", "/v1/responses", "/v1/messages"] {
        let replies = HashMap::from([(
            OPAQUE_ACCOUNT_KEY.to_string(),
            VecDeque::from([MockReply {
                status: 200,
                body: CHAT_STREAM_WITH_SPLIT_ECHOED_KEY,
            }]),
        )]);
        let (base_url, _calls, stop_mock) = start_mock_upstream(replies).await;
        let (state, dir) = build_state(base_url, &[OPAQUE_ACCOUNT_KEY]);
        let (port, gateway_handle) = start_gateway(state).await;

        let (status, body) = protocol_stream_call(port, client_path, "hy3").await;
        assert_eq!(status, StatusCode::OK, "{client_path}: {body}");
        assert!(
            !body.contains(OPAQUE_ACCOUNT_KEY),
            "{client_path} leaked a split selected account Key: {body}"
        );
        assert!(body.contains("before "), "{client_path}: {body}");
        assert!(body.contains(" after"), "{client_path}: {body}");

        gateway::stop_gateway(gateway_handle);
        let _ = stop_mock.send(());
        let _ = fs::remove_dir_all(dir);
    }
}

#[tokio::test]
async fn common_short_key_redaction_preserves_stream_protocol_discriminators() {
    for client_path in ["/v1/chat/completions", "/v1/responses", "/v1/messages"] {
        let replies = HashMap::from([(
            "text".to_string(),
            VecDeque::from([MockReply {
                status: 200,
                body: CHAT_STREAM_WITH_COMMON_KEY,
            }]),
        )]);
        let (base_url, _calls, stop_mock) = start_mock_upstream(replies).await;
        let (state, dir) = build_state(base_url, &["text"]);
        let (port, gateway_handle) = start_gateway(state).await;

        let (status, body) = protocol_stream_call(port, client_path, "hy3").await;
        assert_eq!(status, StatusCode::OK, "{client_path}: {body}");
        assert!(!body.contains("before text after"), "{client_path}: {body}");
        assert!(body.contains("before "), "{client_path}: {body}");
        assert!(body.contains(" after"), "{client_path}: {body}");
        match client_path {
            "/v1/chat/completions" => {
                assert!(body.contains("chat.completion.chunk"), "{body}")
            }
            "/v1/responses" => {
                assert!(body.contains("response.output_text.delta"), "{body}")
            }
            "/v1/messages" => assert!(body.contains("text_delta"), "{body}"),
            _ => unreachable!(),
        }

        gateway::stop_gateway(gateway_handle);
        let _ = stop_mock.send(());
        let _ = fs::remove_dir_all(dir);
    }
}

#[tokio::test]
async fn inference_skips_accounts_with_unusable_stored_credentials() {
    struct Case {
        client_path: &'static str,
        model: &'static str,
        upstream_path: &'static str,
        upstream_body: &'static str,
    }

    for case in [
        Case {
            client_path: "/v1/chat/completions",
            model: "deepseek-v4-flash",
            upstream_path: "/v1/chat/completions",
            upstream_body: SUCCESS_BODY,
        },
        Case {
            client_path: "/v1/responses",
            model: "deepseek-v4-flash",
            upstream_path: "/v1/responses",
            upstream_body: RESPONSES_SUCCESS_BODY,
        },
        Case {
            client_path: "/v1/messages",
            model: "minimax-m2.7",
            upstream_path: "/v1/messages",
            upstream_body: MESSAGES_SUCCESS_BODY,
        },
    ] {
        let replies = HashMap::from([(
            "key-good".to_string(),
            VecDeque::from([MockReply {
                status: 200,
                body: case.upstream_body,
            }]),
        )]);
        let (base_url, calls, stop_mock) = start_mock_upstream(replies).await;
        let (state, dir) = build_state(base_url, &["placeholder", "bad\nheader", "key-good"]);
        state
            .db
            .lock()
            .update_account(
                "acct-1",
                &AccountUpdate {
                    name: None,
                    username: None,
                    password: None,
                    key: None,
                    enabled: None,
                    referral_code: None,
                    purchase_date: None,
                    notes: None,
                },
                Some("!!!not-base64!!!"),
                None,
            )
            .unwrap();
        let (port, gateway_handle) = start_gateway(state.clone()).await;

        let (status, _) = protocol_call(port, case.client_path, case.model).await;
        assert_eq!(status, StatusCode::OK, "{}", case.client_path);
        let calls = calls.lock().unwrap();
        assert_eq!(calls.len(), 1, "{}", case.client_path);
        assert_eq!(calls[0].key, "key-good", "{}", case.client_path);
        assert_eq!(calls[0].path, case.upstream_path, "{}", case.client_path);
        drop(calls);
        let logs = state.db.lock().list_forward_logs(10).unwrap();
        assert_eq!(logs.len(), 3, "{}", case.client_path);
        let success = logs
            .iter()
            .find(|log| log.status == "success")
            .expect("successful fallback attempt should be logged");
        assert_eq!(success.account_id, "acct-3", "{}", case.client_path);
        let request_id = success.request_id.as_deref().unwrap();
        assert!(
            logs.iter()
                .all(|log| log.request_id.as_deref() == Some(request_id))
        );
        let mut attempts = logs
            .iter()
            .filter_map(|log| log.attempt)
            .collect::<Vec<_>>();
        attempts.sort_unstable();
        assert_eq!(attempts, [1, 2, 3]);
        let credential_failures = logs
            .iter()
            .filter(|log| log.error_stage.as_deref() == Some("credential"))
            .collect::<Vec<_>>();
        assert_eq!(credential_failures.len(), 2);
        assert!(
            credential_failures
                .iter()
                .all(|log| log.diagnostic.is_some())
        );

        gateway::stop_gateway(gateway_handle);
        let _ = stop_mock.send(());
        let _ = fs::remove_dir_all(dir);
    }
}

#[tokio::test]
async fn converts_streams_across_chat_messages_and_responses() {
    struct Case {
        client_path: &'static str,
        model: &'static str,
        upstream_path: &'static str,
        upstream_body: &'static str,
        expected_events: &'static [&'static str],
    }

    let cases = [
        Case {
            client_path: "/v1/messages",
            model: "hy3",
            upstream_path: "/v1/chat/completions",
            upstream_body: CHAT_STREAM_BODY,
            expected_events: &["event: message_start", "text_delta", "event: message_stop"],
        },
        Case {
            client_path: "/v1/responses",
            model: "hy3",
            upstream_path: "/v1/chat/completions",
            upstream_body: CHAT_STREAM_BODY,
            expected_events: &[
                "event: response.created",
                "response.output_text.delta",
                "event: response.completed",
            ],
        },
        Case {
            client_path: "/v1/responses",
            model: "glm-5.2",
            upstream_path: "/v1/responses",
            upstream_body: RESPONSES_STREAM_BODY,
            expected_events: &[
                "event: response.created",
                "response.output_text.delta",
                "event: response.completed",
            ],
        },
        Case {
            client_path: "/v1/chat/completions",
            model: "minimax-m2.7",
            upstream_path: "/v1/chat/completions",
            upstream_body: CHAT_STREAM_BODY,
            expected_events: &["finish_reason", "data: [DONE]"],
        },
        Case {
            client_path: "/v1/responses",
            model: "minimax-m2.7",
            upstream_path: "/v1/messages",
            upstream_body: MESSAGES_STREAM_BODY,
            expected_events: &[
                "event: response.created",
                "response.output_text.delta",
                "event: response.completed",
            ],
        },
    ];

    for case in cases {
        let replies = HashMap::from([(
            "key-1".to_string(),
            VecDeque::from([MockReply {
                status: 200,
                body: case.upstream_body,
            }]),
        )]);
        let (base_url, calls, stop_mock) = start_mock_upstream(replies).await;
        let (state, dir) = build_state(base_url, &["key-1"]);
        let (port, gateway_handle) = start_gateway(state.clone()).await;

        let (status, body) = protocol_stream_call(port, case.client_path, case.model).await;
        assert_eq!(status, StatusCode::OK);
        for expected in case.expected_events {
            assert!(
                body.contains(expected),
                "{} {} missing {expected}: {body}",
                case.client_path,
                case.model
            );
        }
        if case.client_path == "/v1/chat/completions" {
            assert_eq!(chat_stream_text(&body), "ok", "{body}");
        }
        assert_eq!(calls.lock().unwrap()[0].path, case.upstream_path);
        let log = state.db.lock().list_forward_logs(1).unwrap().remove(0);
        assert_eq!((log.prompt_tokens, log.completion_tokens), (10, 2));
        assert_eq!(log.status, "success");

        gateway::stop_gateway(gateway_handle);
        let _ = stop_mock.send(());
        let _ = fs::remove_dir_all(dir);
    }
}

#[tokio::test]
async fn stream_can_outlive_non_stream_timeout() {
    let (base_url, calls, stop_mock) = start_delayed_messages_upstream(
        "text/event-stream",
        vec![
            (StdDuration::ZERO, MESSAGES_STREAM_HEAD),
            (StdDuration::from_millis(1_200), MESSAGES_STREAM_TAIL),
        ],
    )
    .await;
    let (state, dir) = build_state(base_url, &["key-1"]);
    let mut config = state.config();
    config.non_stream_timeout_secs = 1;
    config.stream_idle_timeout_secs = 2;
    state.set_config(config).unwrap();
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    let (status, body) = tokio::time::timeout(
        StdDuration::from_secs(4),
        protocol_stream_call(port, "/v1/messages", "minimax-m2.7"),
    )
    .await
    .expect("stream should finish before the test watchdog");
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("event: message_stop"), "{body}");
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    let log = state.db.lock().list_forward_logs(1).unwrap().remove(0);
    assert_eq!(log.status, "success");
    assert_eq!((log.prompt_tokens, log.completion_tokens), (10, 2));

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn non_stream_uses_non_stream_timeout_not_stream_idle_timeout() {
    let (base_url, calls, stop_mock) = start_delayed_messages_upstream(
        "application/json",
        vec![(StdDuration::from_millis(1_200), MESSAGES_SUCCESS_BODY)],
    )
    .await;
    let (state, dir) = build_state(base_url, &["key-1"]);
    let mut config = state.config();
    config.non_stream_timeout_secs = 3;
    config.stream_idle_timeout_secs = 1;
    state.set_config(config).unwrap();
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    let (status, body) = tokio::time::timeout(
        StdDuration::from_secs(5),
        protocol_call(port, "/v1/messages", "minimax-m2.7"),
    )
    .await
    .expect("non-stream response should finish before the test watchdog");
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["content"][0]["text"], serde_json::json!("ok"));
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    let log = state.db.lock().list_forward_logs(1).unwrap().remove(0);
    assert_eq!(log.status, "success");

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn streamed_request_with_non_sse_success_body_timeout_is_not_replayed() {
    let (base_url, calls, stop_mock) = start_delayed_upstream(
        StatusCode::OK,
        "application/json",
        vec![(StdDuration::from_secs(10), MESSAGES_SUCCESS_BODY)],
    )
    .await;
    let (state, dir) = build_state(base_url, &["key-1", "key-2"]);
    let mut config = state.config();
    config.stream_idle_timeout_secs = 1;
    state.set_config(config).unwrap();
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    let (status, body) = tokio::time::timeout(
        StdDuration::from_secs(5),
        protocol_stream_call(port, "/v1/messages", "minimax-m2.7"),
    )
    .await
    .expect("non-SSE stream response should honor the idle timeout");
    assert_eq!(status, StatusCode::GATEWAY_TIMEOUT, "{body}");
    assert!(body.contains("upstream_outcome_unknown"), "{body}");
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    let log = state.db.lock().list_forward_logs(1).unwrap().remove(0);
    assert_eq!(log.status, "outcome_unknown");
    assert_eq!(log.http_status, Some(200));

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn streamed_request_with_stalled_error_body_returns_status_without_replay() {
    let (base_url, calls, stop_mock) = start_delayed_upstream(
        StatusCode::INTERNAL_SERVER_ERROR,
        "application/json",
        vec![(
            StdDuration::from_secs(10),
            r#"{"error":"late failure details"}"#,
        )],
    )
    .await;
    let (state, dir) = build_state(base_url, &["key-1", "key-2"]);
    let mut config = state.config();
    config.stream_idle_timeout_secs = 1;
    state.set_config(config).unwrap();
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    let (status, body) = tokio::time::timeout(
        StdDuration::from_secs(5),
        protocol_stream_call(port, "/v1/messages", "minimax-m2.7"),
    )
    .await
    .expect("error response body should honor the idle timeout");
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
    assert!(body.to_ascii_lowercase().contains("timed out"), "{body}");
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    let log = state.db.lock().list_forward_logs(1).unwrap().remove(0);
    assert_eq!(log.status, "error");

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn stream_idle_timeout_emits_protocol_error_and_updates_log() {
    let (base_url, calls, stop_mock) = start_delayed_messages_upstream(
        "text/event-stream",
        vec![
            (StdDuration::ZERO, MESSAGES_STREAM_HEAD),
            // Keep the tail well beyond the configured idle timeout so a loaded
            // Windows runner cannot race delivery against the timeout itself.
            (StdDuration::from_secs(10), MESSAGES_STREAM_TAIL),
        ],
    )
    .await;
    let (state, dir) = build_state(base_url, &["key-1"]);
    let mut config = state.config();
    config.stream_idle_timeout_secs = 1;
    state.set_config(config).unwrap();
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    let (status, body) = tokio::time::timeout(
        StdDuration::from_secs(8),
        protocol_stream_call(port, "/v1/messages", "minimax-m2.7"),
    )
    .await
    .expect("idle timeout should finish before the test watchdog");
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("event: error"), "{body}");
    assert!(body.contains("upstream_outcome_unknown"), "{body}");
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    let log = state.db.lock().list_forward_logs(1).unwrap().remove(0);
    assert_eq!(log.status, "outcome_unknown");
    assert_eq!(log.cost_state, "outcome_unknown");
    assert_eq!(log.cost, None);
    assert!(log.error_message.is_some());

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn non_stream_body_timeout_is_outcome_unknown_and_is_not_replayed() {
    let (base_url, calls, stop_mock) = start_delayed_messages_upstream(
        "application/json",
        vec![(StdDuration::from_millis(1_200), MESSAGES_SUCCESS_BODY)],
    )
    .await;
    let (state, dir) = build_state(base_url, &["key-1"]);
    let mut config = state.config();
    config.non_stream_timeout_secs = 1;
    state.set_config(config).unwrap();
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    let (status, body) = tokio::time::timeout(
        StdDuration::from_secs(5),
        protocol_call(port, "/v1/messages", "minimax-m2.7"),
    )
    .await
    .expect("non-stream timeout should finish before the test watchdog");
    assert_eq!(status, StatusCode::GATEWAY_TIMEOUT, "{body}");
    assert_eq!(
        body["error"]["type"],
        serde_json::json!("upstream_outcome_unknown")
    );
    let message = body["error"]["message"].as_str().unwrap_or_default();
    let message = message.to_ascii_lowercase();
    assert!(
        message.contains("timeout") || message.contains("timed out"),
        "{body}"
    );
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    let log = state.db.lock().list_forward_logs(1).unwrap().remove(0);
    assert_eq!(log.status, "outcome_unknown");
    assert_eq!(log.cost_state, "outcome_unknown");
    assert_eq!(log.cost, None);

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn truncated_non_stream_success_body_is_outcome_unknown_and_not_replayed() {
    let raw_response = concat!(
        "HTTP/1.1 200 OK\r\n",
        "content-type: application/json\r\n",
        "content-length: 4096\r\n",
        "connection: close\r\n",
        "\r\n",
        "{\"id\":\"partial"
    )
    .as_bytes()
    .to_vec();
    let (base_url, calls, stop_mock) = start_raw_disconnect_upstream(raw_response).await;
    let (state, dir) = build_state(base_url, &["key-1", "key-2"]);
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    let (status, body) = tokio::time::timeout(
        StdDuration::from_secs(5),
        protocol_call(port, "/v1/messages", "minimax-m2.7"),
    )
    .await
    .expect("truncated body should fail before the watchdog");
    assert_eq!(status, StatusCode::BAD_GATEWAY, "{body}");
    assert_eq!(
        body["error"]["type"],
        serde_json::json!("upstream_outcome_unknown")
    );
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    let log = state.db.lock().list_forward_logs(1).unwrap().remove(0);
    assert_eq!(log.status, "outcome_unknown");

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn interrupted_stream_is_outcome_unknown_and_not_replayed() {
    let payload = MESSAGES_STREAM_HEAD;
    let raw_response = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ntransfer-encoding: chunked\r\nconnection: close\r\n\r\n{:X}\r\n{}\r\n",
        payload.len(),
        payload
    )
    .into_bytes();
    let (base_url, calls, stop_mock) = start_raw_disconnect_upstream(raw_response).await;
    let (state, dir) = build_state(base_url, &["key-1", "key-2"]);
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    let (status, body) = tokio::time::timeout(
        StdDuration::from_secs(5),
        protocol_stream_call(port, "/v1/messages", "minimax-m2.7"),
    )
    .await
    .expect("interrupted stream should fail before the watchdog");
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains("upstream_outcome_unknown"), "{body}");
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    let log = state.db.lock().list_forward_logs(1).unwrap().remove(0);
    assert_eq!(log.status, "outcome_unknown");

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn stream_ending_before_downstream_output_retries_same_account_once() {
    let (base_url, calls, stop_mock) = start_sequenced_delayed_upstream(
        StatusCode::OK,
        "text/event-stream",
        vec![Vec::new(), vec![(StdDuration::ZERO, CHAT_STREAM_BODY)]],
    )
    .await;
    let (state, dir) = build_state(base_url, &["key-1", "key-2"]);
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    let (status, body) = tokio::time::timeout(
        StdDuration::from_secs(5),
        protocol_stream_call(port, "/v1/chat/completions", "deepseek-v4-flash"),
    )
    .await
    .expect("the zero-output retry should complete before the watchdog");
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(chat_stream_text(&body), "ok", "{body}");
    assert!(!body.contains("upstream_outcome_unknown"), "{body}");
    assert_eq!(calls.load(Ordering::Relaxed), 2);

    let mut logs = state.db.lock().list_forward_logs(10).unwrap();
    logs.sort_by_key(|log| log.attempt);
    assert_eq!(logs.len(), 2);
    assert!(logs.iter().all(|log| log.account_id == "acct-1"));
    assert_eq!(logs[0].status, "outcome_unknown");
    assert!(
        logs[1].status.starts_with("success"),
        "unexpected successful retry status: {}",
        logs[1].status
    );
    assert_eq!(logs[0].request_id, logs[1].request_id);
    assert_eq!(
        logs[0]
            .diagnostic
            .as_ref()
            .and_then(|value| value.get("retry_action"))
            .and_then(serde_json::Value::as_str),
        Some("retry_same_account")
    );

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn stream_ending_twice_before_downstream_output_stops_after_one_retry() {
    let (base_url, calls, stop_mock) = start_sequenced_delayed_upstream(
        StatusCode::OK,
        "text/event-stream",
        vec![Vec::new(), Vec::new()],
    )
    .await;
    let (state, dir) = build_state(base_url, &["key-1", "key-2"]);
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    let (status, body) = tokio::time::timeout(
        StdDuration::from_secs(5),
        protocol_stream_call(port, "/v1/chat/completions", "deepseek-v4-flash"),
    )
    .await
    .expect("the bounded retry should finish before the watchdog");
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains("upstream_outcome_unknown"), "{body}");
    assert_eq!(calls.load(Ordering::Relaxed), 2);

    let mut logs = state.db.lock().list_forward_logs(10).unwrap();
    logs.sort_by_key(|log| log.attempt);
    assert_eq!(logs.len(), 2);
    assert!(logs.iter().all(|log| log.account_id == "acct-1"));
    let retry_actions = logs
        .iter()
        .map(|log| {
            log.diagnostic
                .as_ref()
                .and_then(|value| value.get("retry_action"))
                .and_then(serde_json::Value::as_str)
        })
        .collect::<Vec<_>>();
    assert_eq!(retry_actions, [Some("retry_same_account"), Some("return")]);

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn upstream_408_is_outcome_unknown_and_does_not_fail_over() {
    let replies = HashMap::from([
        (
            "key-1".to_string(),
            VecDeque::from([MockReply {
                status: 408,
                body: r#"{"error":{"message":"request timed out"}}"#,
            }]),
        ),
        (
            "key-2".to_string(),
            VecDeque::from([MockReply {
                status: 200,
                body: MESSAGES_SUCCESS_BODY,
            }]),
        ),
    ]);
    let (base_url, calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state(base_url, &["key-1", "key-2"]);
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    let (status, body) = protocol_call(port, "/v1/messages", "minimax-m2.7").await;
    assert_eq!(status, StatusCode::GATEWAY_TIMEOUT, "{body}");
    assert_eq!(
        body["error"]["type"],
        serde_json::json!("upstream_outcome_unknown")
    );
    assert_eq!(calls.lock().unwrap().len(), 1);
    assert_eq!(
        state.db.lock().list_forward_logs(1).unwrap()[0].status,
        "outcome_unknown"
    );

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn connect_failure_retries_once_without_account_fallback() {
    let upstream_port = free_port();
    let (state, dir) = build_state(
        format!("http://127.0.0.1:{upstream_port}"),
        &["key-1", "key-2"],
    );
    let mut config = state.config();
    config.connect_timeout_secs = 1;
    state.set_config(config).unwrap();
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    let response = loopback_client()
        .post(format!("http://127.0.0.1:{port}/v1/messages"))
        .header("x-api-key", "gw-test")
        .json(&serde_json::json!({
            "model": "minimax-m2.7",
            "messages": [{"role": "user", "content": "ping"}],
            "max_tokens": 3,
            "stream": false
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    let response_request_id = response
        .headers()
        .get("x-ocg-request-id")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let logs = state.db.lock().list_forward_logs(10).unwrap();
    assert_eq!(logs.len(), 2);
    assert!(logs.iter().all(|log| log.account_id == "acct-1"));
    assert!(logs.iter().all(|log| log.status == "error"));
    assert!(
        logs.iter()
            .all(|log| log.request_id.as_deref() == Some(&response_request_id))
    );
    let mut attempts = logs
        .iter()
        .filter_map(|log| log.attempt)
        .collect::<Vec<_>>();
    attempts.sort_unstable();
    assert_eq!(attempts, [1, 2]);
    assert!(logs.iter().all(|log| {
        log.diagnostic
            .as_ref()
            .and_then(|value| value.get("request_id"))
            .and_then(serde_json::Value::as_str)
            == Some(response_request_id.as_str())
    }));
    let mut retry_actions = logs
        .iter()
        .filter_map(|log| {
            Some((
                log.attempt?,
                log.diagnostic
                    .as_ref()?
                    .get("retry_action")?
                    .as_str()?
                    .to_string(),
            ))
        })
        .collect::<Vec<_>>();
    retry_actions.sort_by_key(|(attempt, _)| *attempt);
    assert_eq!(
        retry_actions,
        [
            (1, "retry_same_account".to_string()),
            (2, "return".to_string())
        ]
    );

    gateway::stop_gateway(gateway_handle);
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn streaming_connect_failure_is_safe_to_retry_once() {
    let upstream_port = free_port();
    let (state, dir) = build_state(
        format!("http://127.0.0.1:{upstream_port}"),
        &["key-1", "key-2"],
    );
    let mut config = state.config();
    config.connect_timeout_secs = 1;
    state.set_config(config).unwrap();
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    let (status, _) = protocol_stream_call(port, "/v1/messages", "minimax-m2.7").await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    let logs = state.db.lock().list_forward_logs(10).unwrap();
    assert_eq!(logs.len(), 2);
    assert!(logs.iter().all(|log| log.account_id == "acct-1"));

    gateway::stop_gateway(gateway_handle);
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn messages_forwards_account_key_as_x_api_key() {
    let replies = HashMap::from([(
        "key-1".to_string(),
        VecDeque::from([MockReply {
            status: 200,
            body: MESSAGES_SUCCESS_BODY,
        }]),
    )]);
    let (base_url, calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state(base_url, &["key-1"]);
    let (port, gateway_handle) = start_gateway(state).await;

    let response = loopback_client()
        .post(format!("http://127.0.0.1:{}/v1/messages", port))
        .header("x-api-key", "gw-test")
        .header("anthropic-version", "2023-06-01")
        .json(&serde_json::json!({
            "model": "minimax-m2.7",
            "messages": [{"role": "user", "content": "ping"}],
            "max_tokens": 3,
            "stream": false
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let calls = calls.lock().unwrap();
    assert_eq!(calls[0].x_api_key.as_deref(), Some("key-1"));
    assert!(calls[0].authorization.is_none());

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn converted_messages_request_does_not_replay_upstream_5xx() {
    let replies = HashMap::from([
        (
            "key-1".to_string(),
            VecDeque::from([MockReply {
                status: 500,
                body: r#"{"error":"temporary"}"#,
            }]),
        ),
        (
            "key-2".to_string(),
            VecDeque::from([MockReply {
                status: 200,
                body: SUCCESS_BODY,
            }]),
        ),
    ]);
    let (base_url, calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state(base_url, &["key-1", "key-2"]);
    let (port, gateway_handle) = start_gateway(state).await;

    let (status, body) = protocol_call(port, "/v1/messages", "hy3").await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body["type"], "error");
    let calls = calls.lock().unwrap();
    assert_eq!(
        calls
            .iter()
            .map(|call| call.key.as_str())
            .collect::<Vec<_>>(),
        ["key-1"]
    );
    assert!(calls.iter().all(|call| call.path == "/v1/chat/completions"));
    drop(calls);

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn manual_order_drives_fallback_while_ineligible_accounts_are_skipped() {
    let replies = HashMap::from([
        (
            "key-2".to_string(),
            VecDeque::from([MockReply {
                status: 403,
                body: r#"{"error":{"message":"forbidden key"}}"#,
            }]),
        ),
        (
            "key-1".to_string(),
            VecDeque::from([MockReply {
                status: 200,
                body: SUCCESS_BODY,
            }]),
        ),
    ]);
    let (base_url, calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state(base_url, &["key-1", "key-2", "key-3", "key-4"]);
    {
        let db = state.db.lock();
        db.reorder_accounts(&[
            "acct-4".into(),
            "acct-3".into(),
            "acct-2".into(),
            "acct-1".into(),
            ZEN_FREE_ACCOUNT_ID.into(),
        ])
        .unwrap();
        db.update_account(
            "acct-4",
            &AccountUpdate {
                name: None,
                username: None,
                password: None,
                key: None,
                enabled: Some(false),
                referral_code: None,
                purchase_date: None,
                notes: None,
            },
            None,
            None,
        )
        .unwrap();
        db.set_account_cooldown(
            "acct-3",
            Some(Utc::now() + Duration::hours(1)),
            Some("test cooldown"),
        )
        .unwrap();
    }
    let (port, gateway_handle) = start_gateway(state).await;

    let (status, _) = chat(port).await;
    assert_eq!(status, 200);
    assert_eq!(
        calls
            .lock()
            .unwrap()
            .iter()
            .map(|call| call.key.as_str())
            .collect::<Vec<_>>(),
        ["key-2", "key-1"]
    );

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn converted_request_error_uses_callers_envelope_without_fallback() {
    let replies = HashMap::from([
        (
            OPAQUE_ACCOUNT_KEY.to_string(),
            VecDeque::from([MockReply {
                status: 400,
                body: ERROR_BODY_WITH_ECHOED_KEY,
            }]),
        ),
        (
            "key-2".to_string(),
            VecDeque::from([MockReply {
                status: 200,
                body: SUCCESS_BODY,
            }]),
        ),
    ]);
    let (base_url, calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state(base_url, &[OPAQUE_ACCOUNT_KEY, "key-2"]);
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    let (status, body) = protocol_call(port, "/v1/messages", "deepseek-v4-flash").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["type"], "error");
    assert!(!body.to_string().contains(OPAQUE_ACCOUNT_KEY));
    assert_eq!(calls.lock().unwrap().len(), 1);

    let log = state.db.lock().list_forward_logs(1).unwrap().remove(0);
    let persisted = format!("{:?}{:?}", log.error_message, log.diagnostic);
    assert!(
        !persisted.contains(OPAQUE_ACCOUNT_KEY),
        "forward log leaked key: {persisted}"
    );
    assert!(log.diagnostic.is_some());

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn unterminated_stream_tail_never_echoes_the_selected_account_key() {
    let replies = HashMap::from([(
        OPAQUE_ACCOUNT_KEY.to_string(),
        VecDeque::from([MockReply {
            status: 200,
            body: CHAT_STREAM_WITH_UNTERMINATED_KEY_TAIL,
        }]),
    )]);
    let (base_url, _calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state(base_url, &[OPAQUE_ACCOUNT_KEY]);
    let (port, gateway_handle) = start_gateway(state).await;

    let (status, body) =
        protocol_stream_call(port, "/v1/chat/completions", "deepseek-v4-flash").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        !body.contains(OPAQUE_ACCOUNT_KEY),
        "stream leaked key: {body}"
    );

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn upstream_payload_too_large_is_not_mislabeled_as_client_body_limit() {
    let replies = HashMap::from([
        (
            "key-1".to_string(),
            VecDeque::from([MockReply {
                status: 413,
                body: r#"{"error":{"message":"provider input too large"}}"#,
            }]),
        ),
        (
            "key-2".to_string(),
            VecDeque::from([MockReply {
                status: 200,
                body: SUCCESS_BODY,
            }]),
        ),
    ]);
    let (base_url, calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state(base_url, &["key-1", "key-2"]);
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    let response = loopback_client()
        .post(format!("http://127.0.0.1:{port}/v1/messages"))
        .header("x-api-key", "gw-test")
        .json(&serde_json::json!({
            "model": "deepseek-v4-flash",
            "messages": [{"role": "user", "content": "ping"}],
            "max_tokens": 3,
            "stream": false
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    let request_id = response
        .headers()
        .get("x-ocg-request-id")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert_eq!(calls.lock().unwrap().len(), 1);

    let forward_logs = state.db.lock().list_forward_logs(10).unwrap();
    assert_eq!(forward_logs.len(), 1);
    assert_eq!(
        forward_logs[0].request_id.as_deref(),
        Some(request_id.as_str())
    );
    assert_eq!(forward_logs[0].error_source.as_deref(), Some("upstream"));
    assert_eq!(
        forward_logs[0].error_stage.as_deref(),
        Some("upstream_http")
    );
    assert!(
        state
            .db
            .lock()
            .query_gateway_logs(10, Some(&request_id))
            .unwrap()
            .is_empty(),
        "upstream 413 must not create a second client/body_limit diagnostic"
    );

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn falls_back_past_five_limited_accounts_to_sixth_success() {
    let replies = (1..=6)
        .map(|i| {
            let reply = if i == 6 {
                MockReply {
                    status: 200,
                    body: SUCCESS_BODY,
                }
            } else {
                MockReply {
                    status: 429,
                    body: LIMITED_BODY,
                }
            };
            (format!("key-{}", i), VecDeque::from([reply]))
        })
        .collect();
    let (base_url, calls, stop_mock) = start_mock_upstream(replies).await;
    let keys = ["key-1", "key-2", "key-3", "key-4", "key-5", "key-6"];
    let (state, dir) = build_state(base_url, &keys);
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    let (status, _) = chat(port).await;
    assert_eq!(status, 200);

    let call_keys = calls
        .lock()
        .unwrap()
        .iter()
        .map(|c| c.key.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        call_keys,
        keys.iter().map(|k| k.to_string()).collect::<Vec<_>>()
    );
    assert!(
        calls
            .lock()
            .unwrap()
            .iter()
            .all(|c| c.accept_encoding.as_deref() == Some("identity"))
    );

    let db = state.db.lock();
    let accounts = db.list_accounts().unwrap();
    assert_eq!(
        accounts
            .iter()
            .filter(|a| a.cooldown_until.is_some())
            .count(),
        5
    );
    let logs = db.list_forward_logs(20).unwrap();
    assert!(
        logs.iter()
            .any(|l| l.account_name == "acct-6" && l.status == "success")
    );

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn upstream_5xx_is_returned_without_same_account_retry_or_fallback() {
    let replies = HashMap::from([
        (
            "key-1".to_string(),
            VecDeque::from([MockReply {
                status: 500,
                body: r#"{"error":"temporary"}"#,
            }]),
        ),
        (
            "key-2".to_string(),
            VecDeque::from([MockReply {
                status: 200,
                body: SUCCESS_BODY,
            }]),
        ),
    ]);
    let (base_url, calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state(base_url, &["key-1", "key-2"]);
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    let (status, _) = chat(port).await;
    assert_eq!(status, 500);

    let call_keys = calls
        .lock()
        .unwrap()
        .iter()
        .map(|c| c.key.clone())
        .collect::<Vec<_>>();
    assert_eq!(call_keys, ["key-1"].map(str::to_string));

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn inference_403_fails_over_without_persisting_an_auth_breaker() {
    let replies = HashMap::from([
        (
            "key-1".to_string(),
            VecDeque::from([MockReply {
                status: 403,
                body: r#"{"error":{"message":"forbidden key"}}"#,
            }]),
        ),
        (
            "key-2".to_string(),
            VecDeque::from([MockReply {
                status: 200,
                body: SUCCESS_BODY,
            }]),
        ),
    ]);
    let (base_url, calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state(base_url, &["key-1", "key-2"]);
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    for _ in 0..2 {
        let (status, body) = chat(port).await;
        assert_eq!(status, 200, "{body}");
    }

    let call_keys = calls
        .lock()
        .unwrap()
        .iter()
        .map(|c| c.key.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        call_keys,
        ["key-1", "key-2", "key-1", "key-2"].map(str::to_string)
    );
    assert!(
        state
            .db
            .lock()
            .get_account("acct-1")
            .unwrap()
            .unwrap()
            .auth_error
            .is_none(),
        "inference 403 must not permanently break an account"
    );

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn inference_401_is_returned_without_failover_or_breaker() {
    let replies = HashMap::from([
        (
            "key-1".to_string(),
            VecDeque::from([MockReply {
                status: 401,
                body: r#"{"error":{"message":"expired key"}}"#,
            }]),
        ),
        (
            "key-2".to_string(),
            VecDeque::from([MockReply {
                status: 200,
                body: SUCCESS_BODY,
            }]),
        ),
    ]);
    let (base_url, calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state(base_url, &["key-1", "key-2"]);
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    for _ in 0..2 {
        let (status, body) = chat(port).await;
        assert_eq!(status, 401, "{body}");
        assert!(
            body.contains("expired key") || body.contains("401"),
            "{body}"
        );
    }
    assert_eq!(
        calls
            .lock()
            .unwrap()
            .iter()
            .map(|call| call.key.as_str())
            .collect::<Vec<_>>(),
        ["key-1", "key-1"]
    );
    assert!(
        state
            .db
            .lock()
            .get_account("acct-1")
            .unwrap()
            .unwrap()
            .auth_error
            .is_none(),
        "inference 401 must not permanently break an account"
    );

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn unknown_model_is_rejected_before_any_upstream_attempt() {
    let replies = HashMap::from([
        (
            "key-1".to_string(),
            VecDeque::from([MockReply {
                status: 401,
                body: r#"{"error":{"message":"model does not exist"}}"#,
            }]),
        ),
        (
            "key-2".to_string(),
            VecDeque::from([MockReply {
                status: 200,
                body: SUCCESS_BODY,
            }]),
        ),
    ]);
    let (base_url, calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state(base_url, &["key-1", "key-2"]);
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    let (status, body) = protocol_call(port, "/v1/chat/completions", "totally-made-up-xyz").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(body.to_string().contains("unknown model"), "{body}");
    assert!(calls.lock().unwrap().is_empty());
    assert!(
        state.db.lock().list_forward_logs(10).unwrap().is_empty(),
        "unknown model must not insert a forward row"
    );
    assert!(
        state
            .db
            .lock()
            .get_account("acct-1")
            .unwrap()
            .unwrap()
            .auth_error
            .is_none(),
        "a rejected unknown model must not touch account state"
    );

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn corrupt_selectable_credential_writes_a_preflight_row_without_upstream_call() {
    let replies = HashMap::from([(
        "key-2".to_string(),
        VecDeque::from([MockReply {
            status: 200,
            body: SUCCESS_BODY,
        }]),
    )]);
    let (base_url, calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state(base_url, &["key-1", "key-2"]);
    state
        .db
        .lock()
        .update_account(
            "acct-1",
            &AccountUpdate {
                name: None,
                username: None,
                password: None,
                key: None,
                enabled: None,
                referral_code: None,
                purchase_date: None,
                notes: None,
            },
            Some("not-a-valid-cipher"),
            None,
        )
        .unwrap();
    let corrupted = state.db.lock().get_account("acct-1").unwrap().unwrap();
    assert!(corrupted.enabled, "{corrupted:?}");
    assert_eq!(corrupted.key_cipher, "not-a-valid-cipher");

    let (port, gateway_handle) = start_gateway(state.clone()).await;
    let (status, body) = chat(port).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let recorded = calls.lock().unwrap().clone();
    assert_eq!(recorded.len(), 1, "{recorded:?}");
    assert_eq!(recorded[0].key, "key-2");

    let mut logs = state.db.lock().list_forward_logs(10).unwrap();
    logs.sort_by_key(|log| log.attempt);
    assert_eq!(logs.len(), 2, "{logs:?}");
    assert_eq!(logs[0].account_id, "acct-1");
    assert_eq!(logs[0].status, "error");
    assert_eq!(logs[0].http_status, None);
    assert_eq!(logs[0].error_source.as_deref(), Some("gateway"));
    assert_eq!(logs[0].error_stage.as_deref(), Some("credential"));
    assert!(
        logs[0]
            .error_message
            .as_deref()
            .is_some_and(|message| message.contains("failed to decrypt account credentials")),
        "{:?}",
        logs[0].error_message
    );
    assert!(
        logs[1].account_id == "acct-2" && logs[1].status.starts_with("success"),
        "{logs:?}"
    );

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn registered_zen_promo_routes_to_zen_not_go() {
    let replies = HashMap::from([(
        String::new(),
        VecDeque::from([
            MockReply {
                status: 200,
                body: SUCCESS_BODY,
            },
            MockReply {
                status: 200,
                body: SUCCESS_BODY,
            },
        ]),
    )]);
    let (mock_base, calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state(format!("{mock_base}/zen/go"), &["key-1"]);
    let (port, gateway_handle) = start_gateway(state).await;

    for model in ["mimo-v2.5-free", "mimo-v2.5"] {
        let (status, body) = protocol_call(port, "/v1/chat/completions", model).await;
        assert_eq!(status, StatusCode::OK, "{model} {body}");
    }
    assert_eq!(
        calls
            .lock()
            .unwrap()
            .iter()
            .map(|call| call.path.as_str())
            .collect::<Vec<_>>(),
        ["/zen/v1/chat/completions", "/zen/v1/chat/completions"]
    );
    assert!(calls.lock().unwrap().iter().all(|call| call.key.is_empty()));

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn go_named_free_stays_on_go() {
    let replies = HashMap::from([(
        "key-1".to_string(),
        VecDeque::from([MockReply {
            status: 200,
            body: SUCCESS_BODY,
        }]),
    )]);
    let (mock_base, calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state(format!("{mock_base}/zen/go"), &["key-1"]);
    let (port, gateway_handle) = start_gateway(state).await;

    let (status, body) = protocol_call(port, "/v1/chat/completions", "ox-alpha-free").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = protocol_call(port, "/v1/chat/completions", "brand-new-promo-free").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(
        calls
            .lock()
            .unwrap()
            .iter()
            .map(|call| call.path.as_str())
            .collect::<Vec<_>>(),
        ["/zen/go/v1/chat/completions"]
    );

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn registered_zen_model_401_is_returned_without_credential_fallback_or_breaker() {
    let replies = HashMap::from([
        (
            String::new(),
            VecDeque::from([MockReply {
                status: 401,
                body: r#"{"error":{"message":"expired key"}}"#,
            }]),
        ),
        (
            "key-2".to_string(),
            VecDeque::from([MockReply {
                status: 200,
                body: SUCCESS_BODY,
            }]),
        ),
    ]);
    let (mock_base, calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state(format!("{mock_base}/zen/go"), &["key-1", "key-2"]);
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    let (status, body) = protocol_call(port, "/v1/chat/completions", "mimo-v2.5-free").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    assert_eq!(
        calls
            .lock()
            .unwrap()
            .iter()
            .map(|call| call.key.as_str())
            .collect::<Vec<_>>(),
        [""]
    );
    assert!(
        state
            .db
            .lock()
            .get_account("acct-1")
            .unwrap()
            .unwrap()
            .auth_error
            .is_none(),
        "inference 401 must not permanently break an account"
    );

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn all_limited_accounts_return_429_with_soonest_reset() {
    let replies = HashMap::from([
        (
            "key-1".to_string(),
            VecDeque::from([MockReply {
                status: 429,
                body: LIMITED_BODY,
            }]),
        ),
        (
            "key-2".to_string(),
            VecDeque::from([MockReply {
                status: 429,
                body: LIMITED_BODY,
            }]),
        ),
    ]);
    let (base_url, _calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state(base_url, &["key-1", "key-2"]);
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    let (status, body) = chat(port).await;
    assert_eq!(status, 429);
    assert!(body.contains("resets_at"));
    assert_eq!(
        state
            .db
            .lock()
            .list_accounts()
            .unwrap()
            .iter()
            .filter(|a| a.cooldown_until.is_some())
            .count(),
        2
    );

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn zen_free_429_is_anonymous_and_cools_the_singleton_egress_route() {
    let replies = HashMap::from([
        (
            String::new(),
            VecDeque::from([MockReply {
                status: 429,
                // Free endpoints may reuse Go quota wording. The endpoint is
                // authoritative and must prevent a probe with key-2.
                body: LIMITED_BODY,
            }]),
        ),
        (
            "key-2".to_string(),
            VecDeque::from([MockReply {
                status: 200,
                body: SUCCESS_BODY,
            }]),
        ),
    ]);
    let (mock_base, calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state(format!("{mock_base}/zen/go"), &["key-1", "key-2"]);
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    let (status, _) = protocol_call(port, "/v1/chat/completions", "mimo-v2.5-free").await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(
        calls
            .lock()
            .unwrap()
            .iter()
            .map(|call| call.key.as_str())
            .collect::<Vec<_>>(),
        [""],
        "Zen Free must not borrow or rotate an account key"
    );
    {
        let db = state.db.lock();
        let source = db.get_account(ZEN_FREE_ACCOUNT_ID).unwrap().unwrap();
        assert!(source.cooldown_free_until.is_some());
        assert!(source.cooldown_5h_until.is_none());
        assert!(source.cooldown_week_until.is_none());
        assert!(source.cooldown_month_until.is_none());
        assert!(db.free_channel_cooldown_until().unwrap().is_some());
        assert!(
            db.get_account("acct-1")
                .unwrap()
                .unwrap()
                .cooldown_until
                .is_none()
        );
    }
    let captured = calls.lock().unwrap()[0].clone();
    assert!(captured.authorization.is_none());
    assert!(captured.x_api_key.is_none());
    assert!(captured.x_goog_api_key.is_none());

    set_account_enabled(&state, "acct-1", false);
    let (status, _) = protocol_call(port, "/v1/chat/completions", "mimo-v2.5-free").await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(calls.lock().unwrap().len(), 1);

    state.db.lock().delete_account("acct-1").unwrap();
    let (status, _) = protocol_call(port, "/v1/chat/completions", "mimo-v2.5-free").await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(calls.lock().unwrap().len(), 1);

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn zen_free_is_anonymous_across_all_client_formats_and_logs_route_identity() {
    let replies = HashMap::from([(
        String::new(),
        VecDeque::from([
            MockReply {
                status: 200,
                body: SUCCESS_BODY,
            },
            MockReply {
                status: 200,
                body: SUCCESS_BODY,
            },
            MockReply {
                status: 200,
                body: SUCCESS_BODY,
            },
            MockReply {
                status: 200,
                body: SUCCESS_BODY,
            },
        ]),
    )]);
    let (mock_base, calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state(format!("{mock_base}/zen/go"), &["normal-key"]);
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    for path in ["/v1/chat/completions", "/v1/responses", "/v1/messages"] {
        let (status, body) = protocol_call(port, path, "mimo-v2.5-free").await;
        assert_eq!(status, StatusCode::OK, "{path}: {body}");
    }
    let (status, body) = gemini_call(port, "mimo-v2.5-free").await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let captured = calls.lock().unwrap().clone();
    assert_eq!(captured.len(), 4);
    assert!(captured.iter().all(|call| {
        call.authorization.is_none() && call.x_api_key.is_none() && call.x_goog_api_key.is_none()
    }));
    assert!(
        captured
            .iter()
            .all(|call| call.path.ends_with("/v1/chat/completions"))
    );
    let logs = state.db.lock().list_forward_logs(10).unwrap();
    assert_eq!(logs.len(), 4);
    assert!(logs.iter().all(|log| {
        log.route_account_id.as_deref() == Some(ZEN_FREE_ACCOUNT_ID)
            && log.provider_id.as_deref() == Some("opencode-zen-free")
            && log.offering_id.as_deref() == Some("anonymous-free")
            && log.credential_account_id.is_none()
            && log.account_id == ZEN_FREE_ACCOUNT_ID
    }));
    assert!(logs.iter().all(|log| {
        log.status == "success"
            && log.cost_state == "free"
            && log.raw_cost_usd == Some(0.0)
            && log.quota_debit == Some(0.0)
            && log.effective_paid_cost_usd == Some(0.0)
            && log.pricing_revision_id.is_none()
            && log.quota_multiplier.is_none()
            && log.local_adjustment_multiplier.is_none()
    }));

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn zen_free_non_stream_success_without_usage_is_still_zero_cost_free() {
    let replies = HashMap::from([(
        String::new(),
        VecDeque::from([MockReply {
            status: 200,
            body: SUCCESS_BODY_WITHOUT_USAGE,
        }]),
    )]);
    let (mock_base, _calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state(format!("{mock_base}/zen/go"), &["normal-key"]);
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    let (status, body) = protocol_call(port, "/v1/chat/completions", "mimo-v2.5-free").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let log = state.db.lock().list_forward_logs(1).unwrap().remove(0);
    assert_eq!(log.status, "success");
    assert_eq!(log.cost_state, "free");
    assert_eq!(log.raw_cost_usd, Some(0.0));
    assert_eq!(log.quota_debit, Some(0.0));
    assert_eq!(log.effective_paid_cost_usd, Some(0.0));
    assert_eq!((log.prompt_tokens, log.completion_tokens), (0, 0));

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn zen_free_stream_success_without_usage_is_still_zero_cost_free() {
    let replies = HashMap::from([(
        String::new(),
        VecDeque::from([MockReply {
            status: 200,
            body: CHAT_STREAM_WITHOUT_USAGE,
        }]),
    )]);
    let (mock_base, _calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state(format!("{mock_base}/zen/go"), &["normal-key"]);
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    let (status, body) = protocol_stream_call(port, "/v1/chat/completions", "mimo-v2.5-free").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let log = state.db.lock().list_forward_logs(1).unwrap().remove(0);
    assert_eq!(log.status, "success");
    assert_eq!(log.cost_state, "free");
    assert_eq!(log.raw_cost_usd, Some(0.0));
    assert_eq!(log.quota_debit, Some(0.0));
    assert_eq!(log.effective_paid_cost_usd, Some(0.0));
    assert_eq!((log.prompt_tokens, log.completion_tokens), (0, 0));

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn zen_free_401_and_403_stop_without_touching_a_normal_credential() {
    let replies = HashMap::from([
        (
            String::new(),
            VecDeque::from([
                MockReply {
                    status: 401,
                    body: r#"{"error":{"message":"anonymous route disabled"}}"#,
                },
                MockReply {
                    status: 403,
                    body: r#"{"error":{"message":"anonymous route forbidden"}}"#,
                },
            ]),
        ),
        (
            "normal-key".to_string(),
            VecDeque::from([MockReply {
                status: 200,
                body: SUCCESS_BODY,
            }]),
        ),
    ]);
    let (mock_base, calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state(format!("{mock_base}/zen/go"), &["normal-key"]);
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    let (status, body) = protocol_call(port, "/v1/chat/completions", "mimo-v2.5-free").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    assert!(
        body.to_string().contains("anonymous route disabled"),
        "{body}"
    );

    let (status, body) = protocol_call(port, "/v1/chat/completions", "mimo-v2.5-free").await;
    assert_eq!(status, StatusCode::BAD_GATEWAY, "{body}");
    assert!(body.to_string().contains("403"), "{body}");
    let captured = calls.lock().unwrap().clone();
    assert_eq!(captured.len(), 2);
    assert!(captured.iter().all(|call| call.key.is_empty()));
    assert!(
        state
            .db
            .lock()
            .get_account("acct-1")
            .unwrap()
            .unwrap()
            .auth_error
            .is_none()
    );

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn ordered_zen_candidate_429_falls_through_to_the_next_normal_card() {
    let replies = HashMap::from([
        (
            String::new(),
            VecDeque::from([MockReply {
                status: 429,
                body: LIMITED_BODY,
            }]),
        ),
        (
            "normal-key".to_string(),
            VecDeque::from([MockReply {
                status: 200,
                body: SUCCESS_BODY,
            }]),
        ),
    ]);
    let (mock_base, calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state(format!("{mock_base}/zen/go"), &["normal-key"]);
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    let (status, body) = protocol_call(port, "/v1/chat/completions", "mimo-v2.5").await;
    assert_eq!(status, 200, "{body}");
    let captured = calls.lock().unwrap().clone();
    assert_eq!(
        captured
            .iter()
            .map(|call| call.key.as_str())
            .collect::<Vec<_>>(),
        ["", "normal-key"]
    );
    assert!(captured[0].body.contains("mimo-v2.5-free"));
    assert!(captured[1].body.contains("mimo-v2.5"));
    assert!(!captured[1].body.contains("mimo-v2.5-free"));
    let logs = state.db.lock().list_forward_logs(10).unwrap();
    assert_eq!(logs.len(), 2);
    assert!(logs.iter().any(|log| {
        log.route_account_id.as_deref() == Some(ZEN_FREE_ACCOUNT_ID) && log.http_status == Some(429)
    }));
    assert!(logs.iter().any(|log| {
        log.route_account_id.as_deref() == Some("acct-1") && log.status == "success"
    }));

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn shared_alias_strict_priority_follows_the_persisted_card_order() {
    let replies = HashMap::from([
        (
            String::new(),
            VecDeque::from([MockReply {
                status: 200,
                body: SUCCESS_BODY,
            }]),
        ),
        (
            "normal-key".to_string(),
            VecDeque::from([MockReply {
                status: 200,
                body: SUCCESS_BODY,
            }]),
        ),
    ]);
    let (mock_base, calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state(format!("{mock_base}/zen/go"), &["normal-key"]);
    state
        .db
        .lock()
        .reorder_accounts(&["acct-1".into(), ZEN_FREE_ACCOUNT_ID.into()])
        .unwrap();
    let (port, gateway_handle) = start_gateway(state.clone()).await;
    let (status, body) = protocol_call(port, "/v1/chat/completions", "mimo-v2.5").await;
    assert_eq!(status, 200, "{body}");
    state
        .db
        .lock()
        .reorder_accounts(&[ZEN_FREE_ACCOUNT_ID.into(), "acct-1".into()])
        .unwrap();
    let (status, body) = protocol_call(port, "/v1/chat/completions", "mimo-v2.5").await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        calls
            .lock()
            .unwrap()
            .iter()
            .map(|call| call.key.as_str())
            .collect::<Vec<_>>(),
        ["normal-key", ""]
    );

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn goat_loopback_adapter_routes_all_client_formats_with_its_own_auth_contract() {
    let replies = HashMap::from([(
        "goat-key".to_string(),
        VecDeque::from([
            MockReply {
                status: 200,
                body: SUCCESS_BODY,
            },
            MockReply {
                status: 200,
                body: SUCCESS_BODY,
            },
            MockReply {
                status: 200,
                body: SUCCESS_BODY,
            },
            MockReply {
                status: 200,
                body: SUCCESS_BODY,
            },
        ]),
    )]);
    let (base_url, calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state(base_url.clone(), &["open-key"]);
    let goat_id = format!("goat-{}", uuid::Uuid::new_v4());
    create_goat_account(&state, "acct-1", &goat_id, "goat-key");
    persist_goat_verified_catalog(
        &state,
        &goat_id,
        &[COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM],
    );
    state
        .db
        .lock()
        .reorder_accounts(&[goat_id.clone(), "acct-1".into(), ZEN_FREE_ACCOUNT_ID.into()])
        .unwrap();
    let _goat_route = install_goat_loopback_route_for_test(goat_id.clone(), base_url).unwrap();
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    for path in ["/v1/chat/completions", "/v1/responses", "/v1/messages"] {
        let (status, body) =
            protocol_call(port, path, COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM).await;
        assert_eq!(status, StatusCode::OK, "{path}: {body}");
        assert_eq!(
            body["model"].as_str(),
            Some(COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM),
            "{path}: {body}"
        );
    }
    let (status, body) = gemini_call(port, COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let captured = calls.lock().unwrap().clone();
    assert_eq!(captured.len(), 4);
    assert!(
        captured
            .iter()
            .all(|call| call.authorization.as_deref() == Some("Bearer goat-key"))
    );
    assert!(captured.iter().all(|call| call.x_api_key.is_none()));
    assert!(
        captured
            .iter()
            .all(|call| call.path == "/provider/v1/chat/completions"),
        "{:?}",
        captured
            .iter()
            .map(|call| call.path.as_str())
            .collect::<Vec<_>>()
    );
    assert!(
        captured
            .iter()
            .all(|call| !call.path.contains("/responses") && !call.path.contains("/messages")),
        "GOAT must not emit /responses or /messages: {:?}",
        captured
            .iter()
            .map(|call| call.path.as_str())
            .collect::<Vec<_>>()
    );
    assert!(captured.iter().all(|call| {
        call.body
            .contains(COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM)
            && !call.body.contains("\"x-cmdc-zdr\"")
    }));
    let logs = state.db.lock().list_forward_logs(10).unwrap();
    assert!(logs.iter().all(|log| {
        log.route_account_id.as_deref() == Some(goat_id.as_str())
            && log.provider_id.as_deref() == Some(COMMAND_CODE_PROVIDER_ID)
            && log.offering_id.as_deref() == Some(GOAT_OFFERING_ID)
            && log.credential_account_id.as_deref() == Some(goat_id.as_str())
            && log.model == COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM
    }));
    assert!(logs.iter().all(|log| {
        log.status == "success_unpriced"
            && log.cost_state == "unpriced"
            && log.cost.is_none()
            && log.raw_cost_usd.is_none()
            && log.quota_debit.is_none()
            && log.effective_paid_cost_usd.is_none()
            && log.pricing_revision_id.is_none()
            && log.quota_multiplier.is_none()
            && log.local_adjustment_multiplier.is_none()
    }));

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn disabled_goat_protocol_fails_locally_without_upstream() {
    let replies = HashMap::from([(
        "goat-key".to_string(),
        VecDeque::from([MockReply {
            status: 200,
            body: SUCCESS_BODY,
        }]),
    )]);
    let (base_url, calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state(base_url.clone(), &["open-key"]);
    let goat_id = format!("goat-{}", uuid::Uuid::new_v4());
    create_goat_account(&state, "acct-1", &goat_id, "goat-key");
    persist_goat_verified_catalog(
        &state,
        &goat_id,
        &[COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM],
    );
    state
        .db
        .lock()
        .reorder_accounts(&[goat_id.clone(), "acct-1".into(), ZEN_FREE_ACCOUNT_ID.into()])
        .unwrap();
    state
        .db
        .lock()
        .set_protocol_switch(
            &ocg_core::provider_contracts::ContractScope::provider(COMMAND_CODE_PROVIDER_ID),
            ocg_core::provider::UpstreamProtocolKind::ChatCompletions,
            false,
            Utc::now(),
        )
        .unwrap();
    state.reload_provider_contracts().unwrap();
    let _goat_route = install_goat_loopback_route_for_test(goat_id, base_url).unwrap();
    let (port, gateway_handle) = start_gateway(state).await;

    let (status, body) = protocol_call(
        port,
        "/v1/chat/completions",
        COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        calls.lock().unwrap().is_empty(),
        "disabled GOAT protocol must fail before sending its stored Key upstream"
    );

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn unsupported_goat_model_is_skipped_before_any_upstream_attempt() {
    let replies = HashMap::from([(
        "open-key".to_string(),
        VecDeque::from([MockReply {
            status: 200,
            body: SUCCESS_BODY,
        }]),
    )]);
    let (base_url, calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state(base_url.clone(), &["open-key"]);
    let goat_id = format!("goat-{}", uuid::Uuid::new_v4());
    create_goat_account(&state, "acct-1", &goat_id, "goat-key");
    state
        .db
        .lock()
        .reorder_accounts(&[goat_id.clone(), "acct-1".into(), ZEN_FREE_ACCOUNT_ID.into()])
        .unwrap();
    let _goat_route = install_goat_loopback_route_for_test(goat_id, base_url).unwrap();
    let (port, gateway_handle) = start_gateway(state).await;

    let (status, body) = chat(port).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        calls
            .lock()
            .unwrap()
            .iter()
            .map(|call| call.key.as_str())
            .collect::<Vec<_>>(),
        ["open-key"]
    );

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn mixed_goat_cooldown_and_sticky_state_are_independent() {
    let replies = HashMap::from([
        (
            "goat-key".to_string(),
            VecDeque::from([MockReply {
                status: 429,
                body: LIMITED_BODY,
            }]),
        ),
        (
            "open-key".to_string(),
            VecDeque::from([MockReply {
                status: 200,
                body: SUCCESS_BODY,
            }]),
        ),
    ]);
    let (base_url, calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state_with_routing(
        base_url.clone(),
        &["open-key"],
        RoutingMode::StickyGlobal,
        false,
    );
    let goat_id = format!("goat-{}", uuid::Uuid::new_v4());
    create_goat_account(&state, "acct-1", &goat_id, "goat-key");
    persist_goat_verified_catalog(
        &state,
        &goat_id,
        &[COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM],
    );
    state
        .db
        .lock()
        .reorder_accounts(&[goat_id.clone(), "acct-1".into(), ZEN_FREE_ACCOUNT_ID.into()])
        .unwrap();
    let _goat_route = install_goat_loopback_route_for_test(goat_id.clone(), base_url).unwrap();
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    let (status, body) = protocol_call(
        port,
        "/v1/chat/completions",
        COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM,
    )
    .await;
    assert_ne!(
        status,
        StatusCode::OK,
        "pinned GOAT 429 must not fall through to Go: {body}"
    );
    assert_eq!(
        calls
            .lock()
            .unwrap()
            .iter()
            .map(|call| call.key.as_str())
            .collect::<Vec<_>>(),
        ["goat-key"]
    );
    assert!(
        calls
            .lock()
            .unwrap()
            .iter()
            .all(|call| call.path == "/provider/v1/chat/completions")
    );
    let goat = state.db.lock().get_account(&goat_id).unwrap().unwrap();
    let open = state.db.lock().get_account("acct-1").unwrap().unwrap();
    assert!(goat.cooldown_until.is_some());
    assert!(goat.cooldown_generic_until.is_some());
    assert!(goat.cooldown_5h_until.is_none());
    assert!(goat.cooldown_week_until.is_none());
    assert!(goat.cooldown_month_until.is_none());
    assert!(open.cooldown_until.is_none());
    let sync = state.db.lock().account_usage_sync_state(&goat_id).unwrap();
    assert!(
        sync.as_ref()
            .is_none_or(|state| state.next_eligible_at.is_none()),
        "GOAT 429 must not schedule OpenCode Go usage sync: {sync:?}"
    );

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn goat_loopback_does_not_steal_go_alias_requests() {
    let replies = HashMap::from([(
        "open-key".to_string(),
        VecDeque::from([MockReply {
            status: 200,
            body: SUCCESS_BODY,
        }]),
    )]);
    let (base_url, calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state(base_url.clone(), &["open-key"]);
    let goat_id = format!("goat-{}", uuid::Uuid::new_v4());
    create_goat_account(&state, "acct-1", &goat_id, "goat-key");
    state
        .db
        .lock()
        .reorder_accounts(&[goat_id.clone(), "acct-1".into(), ZEN_FREE_ACCOUNT_ID.into()])
        .unwrap();
    let _goat_route = install_goat_loopback_route_for_test(goat_id, base_url).unwrap();
    let (port, gateway_handle) = start_gateway(state).await;

    let (status, body) = protocol_call(
        port,
        "/v1/chat/completions",
        COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_ALIAS,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        calls
            .lock()
            .unwrap()
            .iter()
            .map(|call| call.key.as_str())
            .collect::<Vec<_>>(),
        ["open-key"]
    );
    assert!(
        calls
            .lock()
            .unwrap()
            .iter()
            .all(|call| call.path == "/v1/chat/completions")
    );

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn enabled_goat_without_loopback_is_not_selected() {
    let replies = HashMap::from([(
        "open-key".to_string(),
        VecDeque::from([MockReply {
            status: 200,
            body: SUCCESS_BODY,
        }]),
    )]);
    let (base_url, calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state(base_url, &["open-key"]);
    let goat_id = format!("goat-{}", uuid::Uuid::new_v4());
    create_goat_account(&state, "acct-1", &goat_id, "goat-key");
    state
        .db
        .lock()
        .reorder_accounts(&[goat_id, "acct-1".into(), ZEN_FREE_ACCOUNT_ID.into()])
        .unwrap();
    let (port, gateway_handle) = start_gateway(state).await;

    let (status, body) = protocol_call(
        port,
        "/v1/chat/completions",
        COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM,
    )
    .await;
    assert_ne!(
        status,
        StatusCode::OK,
        "enabled but unverified GOAT must stay unselected: {body}"
    );
    assert!(
        calls.lock().unwrap().is_empty(),
        "pinned GOAT raw id must not fall through to Go: {:?}",
        calls.lock().unwrap()
    );

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn eligible_goat_anthropic_catalog_uses_messages_and_converts_client_responses() {
    let replies = HashMap::from([(
        "goat-key".to_string(),
        VecDeque::from([
            MockReply {
                status: 200,
                body: MESSAGES_SUCCESS_BODY,
            },
            MockReply {
                status: 200,
                body: MESSAGES_SUCCESS_BODY,
            },
        ]),
    )]);
    let (base_url, calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state(base_url.clone(), &["open-key"]);
    let goat_id = format!("goat-{}", uuid::Uuid::new_v4());
    create_goat_account(&state, "acct-1", &goat_id, "goat-key");
    state
        .db
        .lock()
        .set_goat_model_access(&goat_id, ocg_core::provider::GoatModelAccess::All)
        .unwrap();
    persist_goat_verified_catalog(&state, &goat_id, &["claude-sonnet-4-6"]);
    state
        .db
        .lock()
        .reorder_accounts(&[goat_id.clone(), "acct-1".into(), ZEN_FREE_ACCOUNT_ID.into()])
        .unwrap();
    let _goat_route = install_goat_loopback_route_for_test(goat_id.clone(), base_url).unwrap();
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    let models = loopback_client()
        .get(format!("http://127.0.0.1:{port}/v1/models"))
        .bearer_auth("gw-test")
        .send()
        .await
        .unwrap();
    assert_eq!(models.status(), StatusCode::OK);
    let models: serde_json::Value = models.json().await.unwrap();
    let ids = models["data"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|item| item["id"].as_str().map(str::to_string))
        .collect::<Vec<_>>();
    assert!(
        ids.iter().any(|id| id == "claude-sonnet-4-6"),
        "eligible unique GOAT kebab id must be published: {ids:?}"
    );
    assert!(
        !ids.iter()
            .any(|id| id == COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM),
        "GOAT raw ids must stay unpublished: {ids:?}"
    );

    for path in ["/v1/messages", "/v1/responses"] {
        let (status, body) = protocol_call(port, path, "claude-sonnet-4-6").await;
        assert_eq!(status, StatusCode::OK, "{path}: {body}");
    }
    let captured = calls.lock().unwrap().clone();
    assert_eq!(captured.len(), 2, "{captured:?}");
    assert!(
        captured
            .iter()
            .all(|call| call.authorization.as_deref() == Some("Bearer goat-key"))
    );
    state
        .db
        .lock()
        .set_protocol_switch(
            &ocg_core::provider_contracts::ContractScope::provider(COMMAND_CODE_PROVIDER_ID),
            ocg_core::provider::UpstreamProtocolKind::Messages,
            false,
            Utc::now(),
        )
        .unwrap();
    state.reload_provider_contracts().unwrap();
    let models = loopback_client()
        .get(format!("http://127.0.0.1:{port}/v1/models"))
        .bearer_auth("gw-test")
        .send()
        .await
        .unwrap();
    assert_eq!(models.status(), StatusCode::OK);
    let models: serde_json::Value = models.json().await.unwrap();
    let ids = models["data"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|item| item["id"].as_str())
        .collect::<Vec<_>>();
    assert!(
        !ids.contains(&"claude-sonnet-4-6"),
        "GOAT model must leave /v1/models when its only protocol is disabled: {ids:?}"
    );
    assert!(
        captured
            .iter()
            .all(|call| call.path == "/provider/v1/messages"),
        "{:?}",
        captured
            .iter()
            .map(|call| call.path.as_str())
            .collect::<Vec<_>>()
    );
    assert!(captured.iter().all(|call| call.x_api_key.is_none()));

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn sticky_global_keeps_failover_account_after_higher_priority_recovers() {
    let replies = HashMap::from([
        (
            "key-1".to_string(),
            VecDeque::from([MockReply {
                status: 200,
                body: SUCCESS_BODY,
            }]),
        ),
        (
            "key-2".to_string(),
            VecDeque::from([MockReply {
                status: 200,
                body: SUCCESS_BODY,
            }]),
        ),
    ]);
    let (base_url, calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state_with_routing(
        base_url,
        &["key-1", "key-2"],
        RoutingMode::StickyGlobal,
        false,
    );
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    assert_eq!(chat(port).await.0, 200);
    set_account_enabled(&state, "acct-1", false);
    assert_eq!(chat(port).await.0, 200);
    set_account_enabled(&state, "acct-1", true);
    assert_eq!(chat(port).await.0, 200);

    assert_eq!(
        calls
            .lock()
            .unwrap()
            .iter()
            .map(|call| call.key.as_str())
            .collect::<Vec<_>>(),
        ["key-1", "key-2", "key-2"]
    );

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn round_robin_cycles_and_skips_a_disabled_account() {
    let replies = HashMap::from([
        (
            "key-1".to_string(),
            VecDeque::from([MockReply {
                status: 200,
                body: SUCCESS_BODY,
            }]),
        ),
        (
            "key-2".to_string(),
            VecDeque::from([MockReply {
                status: 200,
                body: SUCCESS_BODY,
            }]),
        ),
    ]);
    let (base_url, calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state_with_routing(
        base_url,
        &["key-1", "key-2"],
        RoutingMode::RoundRobin,
        false,
    );
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    assert_eq!(chat(port).await.0, 200);
    set_account_enabled(&state, "acct-2", false);
    assert_eq!(chat(port).await.0, 200);
    set_account_enabled(&state, "acct-2", true);
    assert_eq!(chat(port).await.0, 200);
    assert_eq!(chat(port).await.0, 200);

    assert_eq!(
        calls
            .lock()
            .unwrap()
            .iter()
            .map(|call| call.key.as_str())
            .collect::<Vec<_>>(),
        ["key-1", "key-1", "key-2", "key-1"]
    );

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn explicit_conversation_bindings_are_sticky_and_private() {
    let replies = HashMap::from([
        (
            "key-1".to_string(),
            VecDeque::from([MockReply {
                status: 200,
                body: SUCCESS_BODY,
            }]),
        ),
        (
            "key-2".to_string(),
            VecDeque::from([MockReply {
                status: 200,
                body: SUCCESS_BODY,
            }]),
        ),
    ]);
    let (base_url, calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) =
        build_state_with_routing(base_url, &["key-1", "key-2"], RoutingMode::RoundRobin, true);
    let (port, gateway_handle) = start_gateway(state).await;

    for (conversation, user) in [
        ("conversation-a", "a1"),
        ("conversation-b", "b1"),
        ("conversation-a", "a2"),
        ("conversation-b", "b2"),
    ] {
        assert_eq!(
            chat_with_conversation(port, Some(conversation), user)
                .await
                .0,
            200
        );
    }

    let calls = calls.lock().unwrap();
    assert_eq!(
        calls
            .iter()
            .map(|call| call.key.as_str())
            .collect::<Vec<_>>(),
        ["key-1", "key-2", "key-1", "key-2"]
    );
    assert!(calls.iter().all(|call| call.conversation_header.is_none()));
    drop(calls);

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn conversation_failover_rebinds_to_the_successful_account() {
    let replies = HashMap::from([
        (
            "key-1".to_string(),
            VecDeque::from([MockReply {
                status: 403,
                body: r#"{"error":{"message":"forbidden key"}}"#,
            }]),
        ),
        (
            "key-2".to_string(),
            VecDeque::from([MockReply {
                status: 200,
                body: SUCCESS_BODY,
            }]),
        ),
    ]);
    let (base_url, calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state_with_routing(
        base_url,
        &["key-1", "key-2"],
        RoutingMode::StrictPriority,
        true,
    );
    let (port, gateway_handle) = start_gateway(state).await;

    assert_eq!(
        chat_with_conversation(port, Some("conversation-rebind"), "first")
            .await
            .0,
        200
    );
    assert_eq!(
        chat_with_conversation(port, Some("conversation-rebind"), "second")
            .await
            .0,
        200
    );

    assert_eq!(
        calls
            .lock()
            .unwrap()
            .iter()
            .map(|call| call.key.as_str())
            .collect::<Vec<_>>(),
        ["key-1", "key-2", "key-2"]
    );

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn model_discovery_does_not_advance_round_robin_generation_cursor() {
    let replies = HashMap::from([
        (
            "key-1".to_string(),
            VecDeque::from([MockReply {
                status: 200,
                body: SUCCESS_BODY,
            }]),
        ),
        (
            "key-2".to_string(),
            VecDeque::from([MockReply {
                status: 200,
                body: SUCCESS_BODY,
            }]),
        ),
    ]);
    let (base_url, calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state_with_routing(
        base_url,
        &["key-1", "key-2"],
        RoutingMode::RoundRobin,
        false,
    );
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    assert_eq!(chat(port).await.0, 200);
    let (status, body) = models(port).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_local_openai_alias_list(&body);
    assert_eq!(chat(port).await.0, 200);

    let calls = calls.lock().unwrap();
    assert_eq!(
        calls
            .iter()
            .map(|call| (call.key.as_str(), call.path.as_str()))
            .collect::<Vec<_>>(),
        [
            ("key-1", "/v1/chat/completions"),
            ("key-2", "/v1/chat/completions"),
        ]
    );
    drop(calls);
    assert_eq!(state.db.lock().list_forward_logs(10).unwrap().len(), 2);

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn concurrent_round_robin_requests_are_evenly_distributed() {
    let replies = HashMap::from([
        (
            "key-1".to_string(),
            VecDeque::from([MockReply {
                status: 200,
                body: SUCCESS_BODY,
            }]),
        ),
        (
            "key-2".to_string(),
            VecDeque::from([MockReply {
                status: 200,
                body: SUCCESS_BODY,
            }]),
        ),
    ]);
    let (base_url, calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state_with_routing(
        base_url,
        &["key-1", "key-2"],
        RoutingMode::RoundRobin,
        false,
    );
    let (port, gateway_handle) = start_gateway(state).await;

    let requests = (0..20)
        .map(|_| tokio::spawn(chat(port)))
        .collect::<Vec<_>>();
    for request in requests {
        assert_eq!(request.await.unwrap().0, 200);
    }

    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 20);
    assert_eq!(calls.iter().filter(|call| call.key == "key-1").count(), 10);
    assert_eq!(calls.iter().filter(|call| call.key == "key-2").count(), 10);
    drop(calls);

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn dashboard_port_change_rebinds_and_persists_across_restart() {
    let (state, dir) = build_state("http://127.0.0.1:1".into(), &[]);
    let handle = gateway::start_gateway(state.clone(), free_port())
        .await
        .unwrap();
    let current_port = handle.port;
    *state.gateway.lock() = Some(handle);
    {
        let mut config = state.config();
        config.gateway_port = current_port;
        state.set_config(config).unwrap();
    }
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let requested_port = free_port();
    assert_ne!(
        requested_port, current_port,
        "the settings write must request a different port than the live listener"
    );
    let mut config = state.config();
    config.gateway_port = requested_port;
    let settings_payload = serde_json::json!({
        "expectedRevision": state.settings_revision(),
        "processGeneration": state.process_generation(),
        "gatewayPort": requested_port
    });
    let client = loopback_client();
    let response = client
        .put(format!(
            "http://127.0.0.1:{}/dashboard/api/v3/settings",
            current_port
        ))
        .json(&settings_payload)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let result: serde_json::Value = response.json().await.unwrap();
    assert_eq!(result["revision"].as_u64(), Some(state.settings_revision()));
    assert_eq!(state.config().gateway_port, requested_port);
    assert_eq!(
        state.active_gateway_port(),
        requested_port,
        "successful HTTP port mutation rebinds the managed listener"
    );
    let stored = state.db.lock().get_setting("config").unwrap().unwrap();
    let stored: AppConfig = serde_json::from_str(&stored).unwrap();
    assert_eq!(stored.gateway_port, requested_port);

    let status_response = client
        .get(format!(
            "http://127.0.0.1:{}/dashboard/api/v3/gateway/status",
            requested_port
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(status_response.status(), StatusCode::OK);
    let status: serde_json::Value = status_response.json().await.unwrap();
    assert_eq!(status["running"], true);
    assert_eq!(status["port"].as_u64(), Some(u64::from(requested_port)));

    let occupied = StdTcpListener::bind(("127.0.0.1", 0)).unwrap();
    let occupied_port = occupied.local_addr().unwrap().port();
    let mut fail_config = state.config();
    fail_config.gateway_port = occupied_port;
    let fail_payload = serde_json::json!({
        "expectedRevision": state.settings_revision(),
        "processGeneration": state.process_generation(),
        "gatewayPort": occupied_port
    });
    let fail = client
        .put(format!(
            "http://127.0.0.1:{}/dashboard/api/v3/settings",
            requested_port
        ))
        .json(&fail_payload)
        .send()
        .await
        .unwrap();
    assert_eq!(fail.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(
        state.active_gateway_port(),
        requested_port,
        "failed rebind must keep the live listener"
    );
    assert_eq!(
        state.config().gateway_port,
        requested_port,
        "failed rebind compensation must restore the last successful port"
    );

    let handle = state.gateway.lock().take().unwrap();
    gateway::stop_gateway_and_wait(handle).await;
    drop(state);

    let cipher: Arc<dyn KeyCipher + Send + Sync> = Arc::new(StaticKeyCipher::new("test"));
    let restarted =
        CoreStateInner::new(Database::open(dir.clone()).unwrap(), dir.clone(), cipher).unwrap();
    assert_eq!(
        restarted.config().gateway_port,
        requested_port,
        "the last successful port must load on the next process start"
    );
    drop(occupied);
    drop(restarted);
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn forwarded_requests_are_attributed_to_the_authenticating_key() {
    let replies = HashMap::from([(
        "key-1".to_string(),
        VecDeque::from([
            MockReply {
                status: 200,
                body: r#"{"id":"x","choices":[{"message":{"role":"assistant","content":"hi"}}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}"#,
            },
            MockReply {
                status: 200,
                body: r#"{"id":"y","choices":[{"message":{"role":"assistant","content":"yo"}}],"usage":{"prompt_tokens":2,"completion_tokens":2,"total_tokens":4}}"#,
            },
        ]),
    )]);
    let (base_url, calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state(base_url, &["key-1"]);
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    // A sub key shares the same upstream account; usage written under it
    // must be attributable per key.
    let secondary = ocg_core::gateway_keys::create_sub_key(&state, "Laptop").unwrap();

    let client = loopback_client();
    let body = serde_json::json!({
        "model": "deepseek-v4-flash",
        "messages": [{"role": "user", "content": "hello"}],
        "max_tokens": 3,
        "stream": false
    });
    let secondary_status = client
        .post(format!("http://127.0.0.1:{}/v1/chat/completions", port))
        .header(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {}", secondary.key),
        )
        .json(&body)
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(secondary_status, StatusCode::OK);

    let primary_status = chat(port).await.0;
    assert_eq!(primary_status, StatusCode::OK);

    let unauthorized_status = client
        .post(format!("http://127.0.0.1:{}/v1/chat/completions", port))
        .header(reqwest::header::AUTHORIZATION, "Bearer unknown-key")
        .json(&body)
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(unauthorized_status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        calls.lock().unwrap().len(),
        2,
        "only authenticated requests forward"
    );

    let primary_id = ocg_core::gateway_keys::PRIMARY_KEY_ID;
    let logs = state.db.lock().list_forward_logs(10).unwrap();
    assert_eq!(
        logs.len(),
        2,
        "unauthenticated requests write no forward rows"
    );
    let secondary_rows = logs
        .iter()
        .filter(|log| log.client_key_id.as_deref() == Some(secondary.id.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(secondary_rows.len(), 1);
    assert_eq!(
        secondary_rows[0].client_key_name.as_deref(),
        Some("Laptop"),
        "the write-time name snapshot rides along for later renames"
    );
    let primary_rows = logs
        .iter()
        .filter(|log| log.client_key_id.as_deref() == Some(primary_id))
        .collect::<Vec<_>>();
    assert_eq!(primary_rows.len(), 1);

    // Key-scoped queries return only that key's rows plus its summary slice.
    let page = state
        .db
        .lock()
        .query_forward_logs(ForwardLogQueryOptions {
            limit: 10,
            offset: 0,
            status: None,
            account_id: None,
            provider_id: None,
            offering_id: None,
            route_account_id: None,
            credential_account_id: None,
            model: None,
            key_id: Some(secondary.id.as_str()),
            request_id: None,
            start_time: None,
            end_time: None,
            sort_by: None,
            sort_order: None,
        })
        .unwrap();
    assert_eq!(page.summary.total_requests, 1);
    assert_eq!(page.summary.prompt_tokens, 1);
    assert!(
        page.items
            .iter()
            .all(|log| log.client_key_id == Some(secondary.id.clone()))
    );

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn gateway_stays_available_while_large_backfill_runs() {
    let replies = HashMap::from([(
        "key-1".to_string(),
        VecDeque::from([MockReply {
            status: 200,
            body: r#"{"id":"x","choices":[{"message":{"role":"assistant","content":"hi"}}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}"#,
        }]),
    )]);
    let (base_url, _calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state(base_url, &["key-1"]);

    // Seed more rows than one backfill chunk so the background thread takes
    // over after the inline first step.
    {
        let seed_rows = vec![ForwardLog {
            id: 0,
            timestamp: chrono::Utc::now(),
            model: "legacy".into(),
            account_id: "acct".into(),
            account_name: "acct".into(),
            route_account_id: None,
            provider_id: None,
            offering_id: None,
            credential_account_id: None,
            client_key_id: None,
            client_key_name: None,
            status: "success".into(),
            http_status: Some(200),
            route: String::new(),
            prompt_tokens: 0,
            completion_tokens: 0,
            cached_tokens: 0,
            cache_creation_tokens: 0,
            cost: Some(0.0),
            raw_cost_usd: None,
            quota_debit: None,
            effective_paid_cost_usd: None,
            pricing_revision_id: None,
            quota_multiplier: None,
            local_adjustment_multiplier: None,
            service_tier: None,
            cost_state: "legacy_estimate".into(),
            error_message: None,
            request_id: None,
            attempt: None,
            error_source: None,
            error_stage: None,
            duration_ms: None,
            diagnostic: None,
        };
        // More rows than one chunk so the background thread takes over after
        // the inline first step at gateway start.
        (ocg_core::db::FORWARD_LOG_BACKFILL_CHUNK_ROWS + 5_000) as usize];
        let db = state.db.lock();
        db.log_forward_batch(&seed_rows).unwrap();
        assert_eq!(
            db.forward_log_backfill_marker().unwrap(),
            None,
            "seeding must not run the backfill"
        );
    }

    let (port, gateway_handle) = start_gateway(state.clone()).await;

    // Both request classes complete while the backfill thread is still
    // chunking: unauthenticated traffic is untouched, and authenticated
    // logging only ever queues behind one short chunk transaction.
    let (status, _body) = chat(port).await;
    assert_eq!(status, StatusCode::OK);
    let client = loopback_client();
    let unauthorized = client
        .post(format!("http://127.0.0.1:{}/v1/chat/completions", port))
        .header(reqwest::header::AUTHORIZATION, "Bearer wrong-key")
        .json(&serde_json::json!({
            "model": "deepseek-v4-flash",
            "messages": [{"role": "user", "content": "x"}],
            "max_tokens": 1
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

    // The backfill converges to the completion marker.
    let mut marker = None;
    for _ in 0..600 {
        marker = state.db.lock().forward_log_backfill_marker().unwrap();
        if marker.as_deref() == Some(ocg_core::db::BACKFILL_DONE) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert_eq!(
        marker.as_deref(),
        Some(ocg_core::db::BACKFILL_DONE),
        "backfill must complete after the seeded rows"
    );

    // Every row is attributed; the request served mid-backfill carried its
    // key id from the write path.
    let unattributed: i64 = state
        .db
        .lock()
        .query_forward_logs(ForwardLogQueryOptions {
            limit: 1,
            offset: 0,
            status: None,
            account_id: None,
            provider_id: None,
            offering_id: None,
            route_account_id: None,
            credential_account_id: None,
            model: None,
            key_id: Some(ocg_core::models::UNATTRIBUTED_KEY_FILTER),
            request_id: None,
            start_time: None,
            end_time: None,
            sort_by: None,
            sort_order: None,
        })
        .unwrap()
        .summary
        .total_requests;
    assert_eq!(unattributed, 0);
    let attributed_chat: i64 = state
        .db
        .lock()
        .query_forward_logs(ForwardLogQueryOptions {
            limit: 1,
            offset: 0,
            status: None,
            account_id: None,
            provider_id: None,
            offering_id: None,
            route_account_id: None,
            credential_account_id: None,
            model: Some("deepseek-v4-flash"),
            key_id: None,
            request_id: None,
            start_time: None,
            end_time: None,
            sort_by: None,
            sort_order: None,
        })
        .unwrap()
        .summary
        .total_requests;
    assert_eq!(attributed_chat, 1);

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

/// Applies a list-mode whitelist config pointing the exception (proxy) leg at
/// `proxy_base` and returns the mutated config for further tweaks.
fn apply_list_whitelist_config(
    state: &Arc<CoreStateInner>,
    upstream_base: String,
    proxy_base: &str,
    listed: &[&str],
) {
    let mut config = state.config();
    config.upstream_base_url = upstream_base;
    config.proxy_mode = ProxyMode::List;
    config.proxy_url = proxy_base.to_string();
    config.proxy_list_direction = ProxyListDirection::Whitelist;
    config.proxy_list_models = listed.iter().map(|model| model.to_string()).collect();
    state.set_config(config).unwrap();
}

async fn forward_log_rows(state: &Arc<CoreStateInner>) -> Vec<ForwardLog> {
    state.db.lock().list_forward_logs(50).unwrap()
}

#[tokio::test]
async fn list_mode_routes_listed_models_through_the_proxy_leg_and_labels_logs() {
    // Direct-leg upstream answers anything with success.
    let (upstream_base, upstream_calls, stop_upstream) = start_mock_upstream(HashMap::from([(
        "key-1".to_string(),
        VecDeque::from([MockReply {
            status: 200,
            body: SUCCESS_BODY,
        }]),
    )]))
    .await;
    // Proxy-leg server: distinct listener so we can tell legs apart.
    let (proxy_base, proxy_calls, stop_proxy) = start_mock_upstream(HashMap::from([(
        "key-1".to_string(),
        VecDeque::from([MockReply {
            status: 200,
            body: SUCCESS_BODY,
        }]),
    )]))
    .await;

    let (state, dir) = build_state(upstream_base.clone(), &["key-1"]);
    apply_list_whitelist_config(
        &state,
        upstream_base.clone(),
        &proxy_base,
        &["gpt-5.6-luna"],
    );
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    let (status, _) = protocol_call(port, "/v1/chat/completions", "gpt-5.6-luna").await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = protocol_call(port, "/v1/chat/completions", "glm-5.2").await;
    assert_eq!(status, StatusCode::OK);

    let proxy_call_count = proxy_calls.lock().unwrap().len();
    let upstream_call_count = upstream_calls.lock().unwrap().len();
    assert_eq!(
        proxy_call_count, 1,
        "listed model must traverse the proxy leg"
    );
    assert_eq!(
        upstream_call_count, 1,
        "unlisted model must connect directly"
    );

    let logs = forward_log_rows(&state).await;
    let luna = logs
        .iter()
        .find(|log| log.model == "gpt-5.6-luna")
        .expect("listed model row");
    assert_eq!(luna.route, "proxy");
    let glm = logs
        .iter()
        .find(|log| log.model == "glm-5.2")
        .expect("unlisted model row");
    assert_eq!(glm.route, "direct");

    gateway::stop_gateway(gateway_handle);
    let _ = stop_upstream.send(());
    let _ = stop_proxy.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn list_mode_free_fallback_reroutes_to_the_default_leg_mid_request() {
    // Card order puts Zen first: the request starts on the listed free twin
    // (proxy leg, exhausted) and falls back to the unlisted Go model (direct leg).
    let (upstream_base, upstream_calls, stop_upstream) = start_mock_upstream(HashMap::from([(
        "key-1".to_string(),
        VecDeque::from([MockReply {
            status: 200,
            body: SUCCESS_BODY,
        }]),
    )]))
    .await;
    let (proxy_base, proxy_calls, stop_proxy) = start_mock_upstream(HashMap::from([(
        String::new(),
        VecDeque::from([MockReply {
            status: 429,
            body: LIMITED_BODY,
        }]),
    )]))
    .await;

    let (state, dir) = build_state(format!("{upstream_base}/zen/go"), &["key-1"]);
    apply_list_whitelist_config(
        &state,
        format!("{upstream_base}/zen/go"),
        &proxy_base,
        &["mimo-v2.5-free"],
    );
    state
        .db
        .lock()
        .reorder_accounts(&[ZEN_FREE_ACCOUNT_ID.into(), "acct-1".into()])
        .unwrap();
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    let (status, _) = protocol_call(port, "/v1/chat/completions", "mimo-v2.5").await;
    assert_eq!(status, StatusCode::OK);

    assert_eq!(
        proxy_calls.lock().unwrap().len(),
        1,
        "the free twin attempt must use the listed proxy leg"
    );
    assert_eq!(
        upstream_calls.lock().unwrap().len(),
        1,
        "the Go fallback must use the direct default leg"
    );

    let logs = forward_log_rows(&state).await;
    let free_row = logs
        .iter()
        .find(|log| log.model == "mimo-v2.5-free")
        .expect("free attempt row");
    assert_eq!(
        free_row.route, "proxy",
        "free failure rows carry the leg too"
    );
    let go_row = logs
        .iter()
        .find(|log| log.model == "mimo-v2.5" && log.status == "success")
        .expect("Go fallback success row");
    assert_eq!(go_row.route, "direct");

    gateway::stop_gateway(gateway_handle);
    let _ = stop_upstream.send(());
    let _ = stop_proxy.send(());
    let _ = fs::remove_dir_all(dir);
}

#[derive(Clone)]
struct SwitchingProxyState {
    state: Arc<CoreStateInner>,
    replies: Arc<Mutex<VecDeque<MockReply>>>,
    switched: Arc<AtomicBool>,
}

/// Proxy-leg server that flips the process config to Direct while the first
/// attempt is still in flight, then keeps replying from a fixed queue.
async fn switching_proxy_chat(
    axum::extract::State(server): axum::extract::State<SwitchingProxyState>,
) -> impl IntoResponse {
    if !server.switched.swap(true, Ordering::SeqCst) {
        let mut config = server.state.config();
        config.proxy_mode = ProxyMode::Direct;
        server.state.set_config(config).unwrap();
    }
    let reply = server
        .replies
        .lock()
        .unwrap()
        .pop_front()
        .expect("switching proxy replies must be pre-seeded");
    (
        StatusCode::from_u16(reply.status).unwrap(),
        [("content-type", "application/json")],
        reply.body,
    )
}

#[tokio::test]
async fn list_mode_midflight_config_switch_keeps_the_entry_snapshot() {
    // Direct-leg upstream must observe zero calls: the retry after the
    // in-flight switch still resolves from the entry snapshot's proxy leg.
    let (upstream_base, upstream_calls, stop_upstream) = start_mock_upstream(HashMap::new()).await;
    let replies = Arc::new(Mutex::new(VecDeque::from([
        MockReply {
            // 403 still rotates accounts after upstream made inference 401 a
            // hard return (Go uses 401 for ModelError).
            status: 403,
            body: r#"{"error":"first attempt rejected, rotate to next account"}"#,
        },
        MockReply {
            status: 200,
            body: SUCCESS_BODY,
        },
    ])));
    let switched = Arc::new(AtomicBool::new(false));

    let (state, dir) = build_state(upstream_base.clone(), &["key-1", "key-2"]);
    let proxy_listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let proxy_base = format!("http://{}", proxy_listener.local_addr().unwrap());
    let (proxy_shutdown_tx, proxy_shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let proxy_app = Router::new()
        .fallback(switching_proxy_chat)
        .with_state(SwitchingProxyState {
            state: state.clone(),
            replies: replies.clone(),
            switched: switched.clone(),
        });
    tokio::spawn(async move {
        let server = axum::serve(proxy_listener, proxy_app).with_graceful_shutdown(async move {
            let _ = proxy_shutdown_rx.await;
        });
        let _ = server.await;
    });

    apply_list_whitelist_config(
        &state,
        upstream_base.clone(),
        &proxy_base,
        &["gpt-5.6-luna"],
    );
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    let (status, _) = protocol_call(port, "/v1/chat/completions", "gpt-5.6-luna").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        switched.load(Ordering::SeqCst),
        "config must have flipped mid-flight"
    );
    assert_eq!(
        replies.lock().unwrap().len(),
        0,
        "both attempts must have hit the proxy leg of the entry snapshot"
    );
    assert_eq!(
        upstream_calls.lock().unwrap().len(),
        0,
        "the in-flight request must not observe the Direct switch"
    );
    assert_eq!(state.config().proxy_mode, ProxyMode::Direct);

    let logs = forward_log_rows(&state).await;
    assert!(
        logs.iter()
            .filter(|log| log.model == "gpt-5.6-luna")
            .all(|log| log.route == "proxy")
    );

    gateway::stop_gateway(gateway_handle);
    let _ = stop_upstream.send(());
    let _ = proxy_shutdown_tx.send(());
    let _ = fs::remove_dir_all(dir);
}

fn disable_go_protocols(state: &Arc<CoreStateInner>, chat: bool, responses: bool, messages: bool) {
    let now = Utc::now();
    let scope = ocg_core::provider_contracts::ContractScope::provider(OPENCODE_PROVIDER_ID);
    let db = state.db.lock();
    if !chat {
        db.set_protocol_switch(
            &scope,
            ocg_core::provider::UpstreamProtocolKind::ChatCompletions,
            false,
            now,
        )
        .unwrap();
    }
    if !responses {
        db.set_protocol_switch(
            &scope,
            ocg_core::provider::UpstreamProtocolKind::Responses,
            false,
            now,
        )
        .unwrap();
    }
    if !messages {
        db.set_protocol_switch(
            &scope,
            ocg_core::provider::UpstreamProtocolKind::Messages,
            false,
            now,
        )
        .unwrap();
    }
    drop(db);
    state.reload_provider_contracts().unwrap();
}

#[tokio::test]
async fn disabled_protocols_fail_locally_without_upstream() {
    let replies = HashMap::from([(
        "key-1".to_string(),
        VecDeque::from([MockReply {
            status: 200,
            body: SUCCESS_BODY,
        }]),
    )]);
    let (base_url, calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state(base_url, &["key-1"]);
    disable_go_protocols(&state, false, false, false);
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    let (status, body) = protocol_call(port, "/v1/chat/completions", "glm-5.3").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        body.to_string()
            .contains(ocg_core::provider_contracts::NO_ENABLED_UPSTREAM_PROTOCOL)
            || body.to_string().contains("no enabled upstream"),
        "{body}"
    );
    assert!(
        calls.lock().unwrap().is_empty(),
        "disabled protocols must fail before upstream: {:?}",
        calls.lock().unwrap()
    );

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn protocol_switch_filters_v1_models_and_application_models() {
    let replies = HashMap::from([(
        "key-1".to_string(),
        VecDeque::from([MockReply {
            status: 200,
            body: SUCCESS_BODY,
        }]),
    )]);
    let (base_url, calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state(base_url, &["key-1"]);
    disable_go_protocols(&state, false, true, true);
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    let (status, body) = models(port).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        !body.contains("\"glm-5.3\""),
        "chat-only glm-5.3 must leave /v1/models when Chat is disabled: {body}"
    );
    assert!(
        body.contains("\"grok-4.5\""),
        "responses-only grok-4.5 must remain: {body}"
    );

    let (status, app_body) = get_application_models(port).await;
    assert_eq!(status, StatusCode::OK, "{app_body}");
    let ids = app_body
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|item| item.as_str())
        .collect::<Vec<_>>();
    assert!(!ids.contains(&"glm-5.3"));
    assert!(ids.contains(&"grok-4.5"));
    assert!(
        calls.lock().unwrap().is_empty(),
        "listing endpoints must stay local: {:?}",
        calls.lock().unwrap()
    );

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn reenabling_a_protocol_restores_routing_without_a_new_probe() {
    let replies = HashMap::from([(
        "key-1".to_string(),
        VecDeque::from([MockReply {
            status: 200,
            body: SUCCESS_BODY,
        }]),
    )]);
    let (base_url, calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state(base_url, &["key-1"]);
    disable_go_protocols(&state, false, true, true);
    let glm = state
        .provider_contracts()
        .scope(&ocg_core::provider_contracts::ContractScope::provider(
            OPENCODE_PROVIDER_ID,
        ))
        .unwrap()
        .model("glm-5.3")
        .unwrap()
        .clone();
    assert!(glm.protocols.get("chat_completions").unwrap().available);
    assert!(!glm.protocols.get("chat_completions").unwrap().enabled);

    let now = Utc::now();
    state
        .db
        .lock()
        .set_protocol_switch(
            &ocg_core::provider_contracts::ContractScope::provider(OPENCODE_PROVIDER_ID),
            ocg_core::provider::UpstreamProtocolKind::ChatCompletions,
            true,
            now,
        )
        .unwrap();
    state.reload_provider_contracts().unwrap();
    let (port, gateway_handle) = start_gateway(state.clone()).await;
    let (status, body) = protocol_call(port, "/v1/chat/completions", "glm-5.3").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(calls.lock().unwrap().len(), 1);

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

async fn dashboard_protocol_probe(
    port: u16,
    state: &Arc<CoreStateInner>,
    account_id: &str,
    model_id: &str,
    protocols: &[&str],
) -> (StatusCode, serde_json::Value) {
    let response = loopback_client()
        .post(format!(
            "http://127.0.0.1:{port}/dashboard/api/v3/providers/{OPENCODE_PROVIDER_ID}/protocol-probes"
        ))
        .json(&serde_json::json!({
            "expectedRevision": state.settings_revision(),
            "processGeneration": state.process_generation(),
            "accountId": account_id,
            "modelId": model_id,
            "protocols": protocols,
        }))
        .send()
        .await
        .unwrap();
    let status = response.status();
    let body = response.json().await.unwrap();
    (status, body)
}

#[tokio::test]
async fn duplicate_protocol_probes_fail_locally_without_upstream() {
    let replies = HashMap::from([(
        "key-1".to_string(),
        VecDeque::from([MockReply {
            status: 200,
            body: SUCCESS_BODY,
        }]),
    )]);
    let (base_url, calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state(base_url, &["key-1"]);
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    let (status, body) = dashboard_protocol_probe(
        port,
        &state,
        "acct-1",
        "glm-5.2",
        &["chat_completions", "responses", "chat_completions"],
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        body.to_string().contains("duplicate"),
        "duplicate protocols must 400: {body}"
    );
    assert!(
        calls.lock().unwrap().is_empty(),
        "a duplicated protocol must not run a billable probe: {:?}",
        calls.lock().unwrap()
    );

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn explicit_probe_can_add_ceiling_protocol_and_failure_does_not() {
    let replies = HashMap::from([(
        "key-1".to_string(),
        VecDeque::from([
            MockReply {
                status: 500,
                body: r#"{"error":"nope"}"#,
            },
            MockReply {
                status: 200,
                body: SUCCESS_BODY,
            },
            MockReply {
                status: 200,
                body: SUCCESS_BODY,
            },
        ]),
    )]);
    let (base_url, calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state(base_url, &["key-1"]);
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    let before = state
        .provider_contracts()
        .scope(&ocg_core::provider_contracts::ContractScope::provider(
            OPENCODE_PROVIDER_ID,
        ))
        .unwrap()
        .model("grok-4.5")
        .unwrap()
        .clone();
    assert!(!before.protocols.get("chat_completions").unwrap().available);
    assert!(before.protocols.get("responses").unwrap().available);

    let (status, body) =
        dashboard_protocol_probe(port, &state, "acct-1", "grok-4.5", &["chat_completions"]).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["results"][0]["success"], false);
    assert_eq!(body["results"][0]["skipped"], false);
    let after_failure = state
        .provider_contracts()
        .scope(&ocg_core::provider_contracts::ContractScope::provider(
            OPENCODE_PROVIDER_ID,
        ))
        .unwrap()
        .model("grok-4.5")
        .unwrap()
        .clone();
    assert!(
        !after_failure
            .protocols
            .get("chat_completions")
            .unwrap()
            .available
    );
    assert!(after_failure.protocols.get("responses").unwrap().available);
    assert_eq!(
        after_failure
            .protocols
            .get("chat_completions")
            .unwrap()
            .last_probe_result,
        Some(ocg_core::provider_contracts::ProbeResultKind::Failure)
    );

    let (status, body) =
        dashboard_protocol_probe(port, &state, "acct-1", "grok-4.5", &["chat_completions"]).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["results"][0]["success"], true);
    let after_success = state
        .provider_contracts()
        .scope(&ocg_core::provider_contracts::ContractScope::provider(
            OPENCODE_PROVIDER_ID,
        ))
        .unwrap()
        .model("grok-4.5")
        .unwrap()
        .clone();
    assert!(
        after_success
            .protocols
            .get("chat_completions")
            .unwrap()
            .available
    );
    assert!(
        after_success
            .protocols
            .get("chat_completions")
            .unwrap()
            .enabled
    );

    let (status, body) = protocol_call(port, "/v1/chat/completions", "grok-4.5").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let recorded = calls.lock().unwrap();
    assert!(
        recorded
            .iter()
            .any(|call| call.path == "/v1/chat/completions" && call.body.contains("grok-4.5")),
        "probed Chat must become the selected production path: {recorded:?}"
    );

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}
