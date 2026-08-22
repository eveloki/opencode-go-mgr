//! HTTP helpers for Dashboard V3 contract kernel tests.

#![allow(dead_code)]

use ocg_core::crypto::{KeyCipher, StaticKeyCipher};
use ocg_core::db::Database;
use ocg_core::gateway;
use ocg_core::state::{CoreStateInner, GatewayHandle};
use reqwest::StatusCode;
use serde_json::Value;
use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

pub struct V3Harness {
    pub state: Arc<CoreStateInner>,
    pub dir: PathBuf,
    pub handle: GatewayHandle,
    pub client: reqwest::Client,
    pub v2_base: String,
    pub v3_base: String,
}

pub fn temp_data_dir(label: &str) -> PathBuf {
    let mut dir = std::env::temp_dir();
    dir.push(format!(
        "ocg-dashboard-v3-{}-{}",
        label,
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

pub fn loopback_client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("dashboard v3 test client should build")
}

pub fn state(label: &str) -> Arc<CoreStateInner> {
    let dir = temp_data_dir(label);
    let db = Database::open(dir.clone()).unwrap();
    let cipher: Arc<dyn KeyCipher + Send + Sync> = Arc::new(StaticKeyCipher::new("v3-contract"));
    Arc::new(CoreStateInner::new(db, dir, cipher).unwrap())
}

pub async fn start_loopback(label: &str) -> V3Harness {
    start_on(label, SocketAddr::from(([127, 0, 0, 1], 0))).await
}

pub async fn start_public(label: &str) -> V3Harness {
    start_on(label, SocketAddr::from(([0, 0, 0, 0], 0))).await
}

async fn start_on(label: &str, addr: SocketAddr) -> V3Harness {
    let state = state(label);
    let dir = state.data_dir();
    let handle = gateway::start_gateway_on(state.clone(), addr)
        .await
        .unwrap();
    let host = format!("http://127.0.0.1:{}", handle.port);
    V3Harness {
        state,
        dir,
        handle,
        client: loopback_client(),
        v2_base: format!("{host}/dashboard/api"),
        v3_base: format!("{host}/dashboard/api/v3"),
    }
}

impl V3Harness {
    pub async fn get_json(&self, url: &str) -> (StatusCode, Value) {
        let response = self.client.get(url).send().await.unwrap();
        let status = response.status();
        let body = response.json().await.unwrap_or(Value::Null);
        (status, body)
    }

    pub fn stop(self) {
        gateway::stop_gateway(self.handle);
        let _ = fs::remove_dir_all(self.dir);
    }
}
