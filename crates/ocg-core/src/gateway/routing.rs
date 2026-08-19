use crate::gateway::protocol::ApiFormat;
use crate::gateway::selector::AccountSelector;
use crate::models::{Account, RoutingMode, UpstreamChannel};
use axum::http::HeaderMap;
use bytes::Bytes;
use parking_lot::Mutex;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

pub const CONVERSATION_HEADER: &str = "x-ocg-conversation-id";
pub const CONVERSATION_TTL: Duration = Duration::from_secs(30 * 60);
pub const MAX_CONVERSATIONS: usize = 4096;
const MAX_EXPLICIT_ID_LEN: usize = 256;

#[derive(Debug, Clone)]
struct ConversationBinding {
    account_id: String,
    channel: UpstreamChannel,
    resolved_model: String,
    last_seen: Instant,
}

#[derive(Debug, Default)]
struct RoutingRuntimeState {
    global_account_id: Option<String>,
    round_robin_after: Option<String>,
    conversations: ConversationMap,
}

#[derive(Debug, Default)]
struct ConversationMap {
    entries: HashMap<String, ConversationBinding>,
    order: VecDeque<String>,
}

impl ConversationMap {
    fn get_fresh(&mut self, key: &str, now: Instant) -> Option<&ConversationBinding> {
        self.purge_expired(now);
        let expired = self
            .entries
            .get(key)
            .is_some_and(|binding| now.duration_since(binding.last_seen) >= CONVERSATION_TTL);
        if expired {
            self.remove(key);
            return None;
        }
        if let Some(binding) = self.entries.get_mut(key) {
            binding.last_seen = now;
            self.touch_order(key);
            self.entries.get(key)
        } else {
            None
        }
    }

    fn insert(
        &mut self,
        key: String,
        account_id: String,
        channel: UpstreamChannel,
        resolved_model: String,
        now: Instant,
    ) {
        self.purge_expired(now);
        if let Some(existing) = self.entries.get_mut(&key) {
            existing.account_id = account_id;
            existing.channel = channel;
            existing.resolved_model = resolved_model;
            existing.last_seen = now;
            self.touch_order(&key);
            return;
        }
        while self.entries.len() >= MAX_CONVERSATIONS {
            if let Some(oldest) = self.order.pop_front() {
                self.entries.remove(&oldest);
            } else {
                break;
            }
        }
        self.entries.insert(
            key.clone(),
            ConversationBinding {
                account_id,
                channel,
                resolved_model,
                last_seen: now,
            },
        );
        self.order.push_back(key);
    }

    fn remove(&mut self, key: &str) {
        self.entries.remove(key);
        if let Some(index) = self.order.iter().position(|item| item == key) {
            self.order.remove(index);
        }
    }

    fn touch_order(&mut self, key: &str) {
        if let Some(index) = self.order.iter().position(|item| item == key) {
            self.order.remove(index);
        }
        self.order.push_back(key.to_string());
    }

    fn purge_expired(&mut self, now: Instant) {
        let expired = self
            .entries
            .iter()
            .filter(|(_, binding)| now.duration_since(binding.last_seen) >= CONVERSATION_TTL)
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();
        for key in expired {
            self.remove(&key);
        }
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries.len()
    }
}

#[derive(Debug, Default)]
pub struct RoutingRuntime {
    inner: Mutex<RoutingRuntimeState>,
}

#[derive(Debug, Clone)]
pub struct RoutingCandidate {
    pub account: Account,
    pub channel: UpstreamChannel,
    pub resolved_model: String,
}

impl RoutingRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&self) {
        let mut state = self.inner.lock();
        *state = RoutingRuntimeState::default();
    }

    /// Select an account for Go channel requests (test and legacy callers).
    pub fn select_account(
        &self,
        accounts: &[Account],
        mode: RoutingMode,
        conversation_sticky: bool,
        conversation_key: Option<&str>,
        exclude_ids: &[&str],
    ) -> Option<Account> {
        self.select_account_for(
            accounts,
            mode,
            conversation_sticky,
            conversation_key,
            UpstreamChannel::Go,
            "",
            exclude_ids,
        )
    }

    /// Select an account for a generation request and update sticky/round-robin state.
    #[allow(clippy::too_many_arguments)]
    pub fn select_account_for(
        &self,
        accounts: &[Account],
        mode: RoutingMode,
        conversation_sticky: bool,
        conversation_key: Option<&str>,
        channel: UpstreamChannel,
        resolved_model: &str,
        exclude_ids: &[&str],
    ) -> Option<Account> {
        let candidates = accounts
            .iter()
            .cloned()
            .map(|account| RoutingCandidate {
                account,
                channel,
                resolved_model: resolved_model.to_string(),
            })
            .collect::<Vec<_>>();
        self.select_candidate(
            &candidates,
            mode,
            conversation_sticky,
            conversation_key,
            exclude_ids,
        )
        .map(|candidate| candidate.account)
    }

    /// Select one already capability-filtered route target. Candidates retain
    /// database order, while each carries its own provider channel and resolved
    /// model (for example, a Zen mapped model beside later paid accounts).
    pub fn select_candidate(
        &self,
        candidates: &[RoutingCandidate],
        mode: RoutingMode,
        conversation_sticky: bool,
        conversation_key: Option<&str>,
        exclude_ids: &[&str],
    ) -> Option<RoutingCandidate> {
        let now = Instant::now();
        let mut state = self.inner.lock();

        if conversation_sticky
            && let Some(key) = conversation_key
            && let Some(binding) = state.conversations.get_fresh(key, now)
        {
            // Sticky locks account + provider channel + resolved model for the session.
            if let Some(candidate) = find_available_candidate(
                candidates,
                &binding.account_id,
                binding.channel,
                &binding.resolved_model,
                exclude_ids,
            ) {
                return Some(candidate);
            }
        }

        let selected = match mode {
            RoutingMode::StrictPriority => first_available_candidate(candidates, exclude_ids),
            RoutingMode::StickyGlobal => {
                select_sticky_global_candidate(&mut state, candidates, exclude_ids)
            }
            RoutingMode::RoundRobin => {
                select_round_robin_candidate(&mut state, candidates, exclude_ids)
            }
        }?;

        if conversation_sticky && let Some(key) = conversation_key {
            state.conversations.insert(
                key.to_string(),
                selected.account.id.clone(),
                selected.channel,
                selected.resolved_model.clone(),
                now,
            );
        }

        Some(selected)
    }

    /// Read sticky binding for a conversation if still fresh.
    pub fn sticky_binding(
        &self,
        conversation_key: &str,
    ) -> Option<(String, UpstreamChannel, String)> {
        let now = Instant::now();
        let mut state = self.inner.lock();
        state
            .conversations
            .get_fresh(conversation_key, now)
            .map(|binding| {
                (
                    binding.account_id.clone(),
                    binding.channel,
                    binding.resolved_model.clone(),
                )
            })
    }

    #[cfg(test)]
    fn snapshot(&self) -> (Option<String>, Option<String>, usize, Option<String>) {
        let state = self.inner.lock();
        let first_binding = state
            .conversations
            .order
            .front()
            .and_then(|key| state.conversations.entries.get(key))
            .map(|binding| binding.account_id.clone());
        (
            state.global_account_id.clone(),
            state.round_robin_after.clone(),
            state.conversations.len(),
            first_binding,
        )
    }

    #[cfg(test)]
    fn force_bind_age(&self, key: &str, account_id: &str, last_seen: Instant) {
        let mut state = self.inner.lock();
        state.conversations.entries.insert(
            key.to_string(),
            ConversationBinding {
                account_id: account_id.to_string(),
                channel: UpstreamChannel::Go,
                resolved_model: "test-model".to_string(),
                last_seen,
            },
        );
        if !state.conversations.order.iter().any(|item| item == key) {
            state.conversations.order.push_back(key.to_string());
        }
    }
}

fn candidate_is_available(candidate: &RoutingCandidate, exclude_ids: &[&str]) -> bool {
    AccountSelector::is_available_for(&candidate.account, candidate.channel, exclude_ids)
}

fn first_available_candidate(
    candidates: &[RoutingCandidate],
    exclude_ids: &[&str],
) -> Option<RoutingCandidate> {
    candidates
        .iter()
        .find(|candidate| candidate_is_available(candidate, exclude_ids))
        .cloned()
}

fn find_available_candidate(
    candidates: &[RoutingCandidate],
    account_id: &str,
    channel: UpstreamChannel,
    resolved_model: &str,
    exclude_ids: &[&str],
) -> Option<RoutingCandidate> {
    candidates
        .iter()
        .find(|candidate| {
            candidate.account.id == account_id
                && candidate.channel == channel
                && candidate.resolved_model == resolved_model
                && candidate_is_available(candidate, exclude_ids)
        })
        .cloned()
}

fn select_sticky_global_candidate(
    state: &mut RoutingRuntimeState,
    candidates: &[RoutingCandidate],
    exclude_ids: &[&str],
) -> Option<RoutingCandidate> {
    if let Some(current_id) = state.global_account_id.clone() {
        if let Some(candidate) = candidates.iter().find(|candidate| {
            candidate.account.id == current_id && candidate_is_available(candidate, exclude_ids)
        }) {
            return Some(candidate.clone());
        }
        let persistently_available = candidates.iter().any(|candidate| {
            candidate.account.id == current_id && candidate_is_available(candidate, &[])
        });
        let selected = first_available_candidate(candidates, exclude_ids)?;
        if !persistently_available {
            state.global_account_id = Some(selected.account.id.clone());
        }
        return Some(selected);
    }
    let selected = first_available_candidate(candidates, exclude_ids)?;
    state.global_account_id = Some(selected.account.id.clone());
    Some(selected)
}

fn select_round_robin_candidate(
    state: &mut RoutingRuntimeState,
    candidates: &[RoutingCandidate],
    exclude_ids: &[&str],
) -> Option<RoutingCandidate> {
    if candidates.is_empty() {
        return None;
    }
    let start = state
        .round_robin_after
        .as_ref()
        .and_then(|after| {
            candidates
                .iter()
                .position(|candidate| candidate.account.id == *after)
        })
        .map(|index| (index + 1) % candidates.len())
        .unwrap_or(0);
    for offset in 0..candidates.len() {
        let index = (start + offset) % candidates.len();
        let candidate = &candidates[index];
        if candidate_is_available(candidate, exclude_ids) {
            state.round_robin_after = Some(candidate.account.id.clone());
            return Some(candidate.clone());
        }
    }
    None
}

pub fn resolve_conversation_key(
    client_format: ApiFormat,
    model: &str,
    headers: &HeaderMap,
    client_body: &Bytes,
) -> Option<String> {
    if let Some(explicit) = explicit_conversation_id(headers) {
        return Some(namespaced_key(
            client_format,
            model,
            "explicit",
            explicit.as_bytes(),
        ));
    }
    let body: Value = serde_json::from_slice(client_body).ok()?;
    let seed = prompt_seed(client_format, &body)?;
    let canonical = serde_json::to_vec(&canonicalize_json(&seed)).ok()?;
    Some(namespaced_key(client_format, model, "prompt", &canonical))
}

fn explicit_conversation_id(headers: &HeaderMap) -> Option<String> {
    let raw = headers
        .get(CONVERSATION_HEADER)
        .and_then(|value| value.to_str().ok())?;
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_EXPLICIT_ID_LEN {
        return None;
    }
    if trimmed.chars().any(|ch| ch.is_control()) {
        return None;
    }
    Some(trimmed.to_string())
}

fn namespaced_key(format: ApiFormat, model: &str, kind: &str, payload: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(format.as_routing_label().as_bytes());
    hasher.update([0]);
    hasher.update(model.as_bytes());
    hasher.update([0]);
    hasher.update(kind.as_bytes());
    hasher.update([0]);
    hasher.update(payload);
    hex::encode(hasher.finalize())
}

// Avoid adding a hex crate; format digest bytes manually.
mod hex {
    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let bytes = bytes.as_ref();
        let mut out = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            out.push(HEX[(byte >> 4) as usize] as char);
            out.push(HEX[(byte & 0xf) as usize] as char);
        }
        out
    }
}

impl ApiFormat {
    fn as_routing_label(self) -> &'static str {
        match self {
            Self::ChatCompletions => "chat_completions",
            Self::Responses => "responses",
            Self::Messages => "messages",
            Self::Gemini => "gemini",
        }
    }
}

fn prompt_seed(format: ApiFormat, body: &Value) -> Option<Value> {
    let (system, tools, first_user) = match format {
        ApiFormat::ChatCompletions => (
            chat_system_seed(body),
            body.get("tools").cloned().filter(|value| !value.is_null()),
            chat_first_user(body),
        ),
        ApiFormat::Responses => (
            body.get("instructions")
                .cloned()
                .filter(|value| !is_empty_json(value)),
            body.get("tools").cloned().filter(|value| !value.is_null()),
            responses_first_user(body),
        ),
        ApiFormat::Messages => (
            body.get("system")
                .cloned()
                .filter(|value| !is_empty_json(value)),
            body.get("tools").cloned().filter(|value| !value.is_null()),
            messages_first_user(body),
        ),
        ApiFormat::Gemini => (
            body.get("systemInstruction")
                .cloned()
                .filter(|value| !is_empty_json(value)),
            body.get("tools").cloned().filter(|value| !value.is_null()),
            gemini_first_user(body),
        ),
    };

    let first_user = first_user.filter(|value| !is_empty_json(value))?;
    let mut seed = Map::new();
    if let Some(system) = system.filter(|value| !is_empty_json(value)) {
        seed.insert("system".into(), system);
    }
    if let Some(tools) = tools.filter(|value| !is_empty_json(value)) {
        seed.insert("tools".into(), tools);
    }
    seed.insert("first_user".into(), first_user);
    Some(Value::Object(seed))
}

fn chat_system_seed(body: &Value) -> Option<Value> {
    let mut parts = Vec::new();
    for message in json_array(body, "messages") {
        if let Some("system" | "developer") = message.get("role").and_then(Value::as_str)
            && let Some(content) = message.get("content")
        {
            parts.push(content.clone());
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(Value::Array(parts))
    }
}

fn chat_first_user(body: &Value) -> Option<Value> {
    for message in json_array(body, "messages") {
        if message.get("role").and_then(Value::as_str) == Some("user") {
            return message.get("content").cloned();
        }
    }
    None
}

fn messages_first_user(body: &Value) -> Option<Value> {
    chat_first_user(body)
}

fn responses_first_user(body: &Value) -> Option<Value> {
    match body.get("input") {
        Some(Value::String(text)) if !text.is_empty() => Some(Value::String(text.clone())),
        Some(Value::Array(items)) => {
            for item in items {
                let role = item.get("role").and_then(Value::as_str);
                let item_type = item.get("type").and_then(Value::as_str);
                if role == Some("user")
                    || (item_type == Some("message") && role.unwrap_or("user") == "user")
                {
                    return item.get("content").cloned().or_else(|| Some(item.clone()));
                }
            }
            None
        }
        _ => None,
    }
}

fn gemini_first_user(body: &Value) -> Option<Value> {
    for content in json_array(body, "contents") {
        match content.get("role").and_then(Value::as_str) {
            Some("user") | None => {
                return content
                    .get("parts")
                    .cloned()
                    .or_else(|| Some(content.clone()));
            }
            _ => {}
        }
    }
    None
}

fn json_array<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

fn is_empty_json(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::String(text) => text.is_empty(),
        Value::Array(items) => items.is_empty(),
        Value::Object(map) => map.is_empty(),
        _ => false,
    }
}

fn canonicalize_json(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut keys = map.keys().cloned().collect::<Vec<_>>();
            keys.sort();
            let mut out = Map::new();
            for key in keys {
                if let Some(child) = map.get(&key) {
                    out.insert(key, canonicalize_json(child));
                }
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(canonicalize_json).collect()),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::{KeyCipher, StaticKeyCipher};
    use chrono::Utc;
    use std::sync::Arc;

    fn account(id: &str, enabled: bool) -> Account {
        let cipher: Arc<dyn KeyCipher + Send + Sync> = Arc::new(StaticKeyCipher::new("test"));
        Account {
            id: id.into(),
            provider_id: crate::provider::default_provider_id(),
            offering_id: crate::provider::default_offering_id(),
            credential_kind: crate::provider::default_credential_kind(),
            quota_scope: crate::provider::default_quota_scope(),
            free_alias_enabled: false,
            name: id.into(),
            username: None,
            password_cipher: None,
            key_cipher: cipher.encrypt(id).unwrap(),
            enabled,
            account_type: crate::models::AccountType::Key,
            setup_step: crate::models::AccountSetupStep::Ready,
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
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn cooling(id: &str) -> Account {
        let mut item = account(id, true);
        item.cooldown_generic_until = Some(Utc::now() + chrono::Duration::hours(1));
        item.cooldown_until = item.cooldown_generic_until;
        item
    }

    #[test]
    fn strict_priority_picks_first_available() {
        let runtime = RoutingRuntime::new();
        let accounts = vec![account("a", false), account("b", true), account("c", true)];
        let selected = runtime
            .select_account(&accounts, RoutingMode::StrictPriority, false, None, &[])
            .unwrap();
        assert_eq!(selected.id, "b");
    }

    #[test]
    fn sticky_global_keeps_current_when_higher_priority_recovers() {
        let runtime = RoutingRuntime::new();
        let first = vec![cooling("a"), account("b", true)];
        assert_eq!(
            runtime
                .select_account(&first, RoutingMode::StickyGlobal, false, None, &[])
                .unwrap()
                .id,
            "b"
        );
        let recovered = vec![account("a", true), account("b", true)];
        assert_eq!(
            runtime
                .select_account(&recovered, RoutingMode::StickyGlobal, false, None, &[])
                .unwrap()
                .id,
            "b"
        );
    }

    #[test]
    fn sticky_global_transient_exclude_does_not_rewrite_global() {
        let runtime = RoutingRuntime::new();
        let accounts = vec![account("a", true), account("b", true)];
        assert_eq!(
            runtime
                .select_account(&accounts, RoutingMode::StickyGlobal, false, None, &[])
                .unwrap()
                .id,
            "a"
        );
        // Request-local exclude (e.g. 403/preflight failover): use next account now,
        // but keep the persistent global sticky on a.
        assert_eq!(
            runtime
                .select_account(&accounts, RoutingMode::StickyGlobal, false, None, &["a"])
                .unwrap()
                .id,
            "b"
        );
        let (global, _, _, _) = runtime.snapshot();
        assert_eq!(global.as_deref(), Some("a"));
        assert_eq!(
            runtime
                .select_account(&accounts, RoutingMode::StickyGlobal, false, None, &[])
                .unwrap()
                .id,
            "a"
        );
    }

    #[test]
    fn sticky_global_switches_when_current_persistently_unavailable() {
        let runtime = RoutingRuntime::new();
        let accounts = vec![account("a", true), account("b", true)];
        assert_eq!(
            runtime
                .select_account(&accounts, RoutingMode::StickyGlobal, false, None, &[])
                .unwrap()
                .id,
            "a"
        );
        let disabled = vec![account("a", false), account("b", true)];
        assert_eq!(
            runtime
                .select_account(&disabled, RoutingMode::StickyGlobal, false, None, &[])
                .unwrap()
                .id,
            "b"
        );
        let (global, _, _, _) = runtime.snapshot();
        assert_eq!(global.as_deref(), Some("b"));

        let runtime = RoutingRuntime::new();
        assert_eq!(
            runtime
                .select_account(&accounts, RoutingMode::StickyGlobal, false, None, &[])
                .unwrap()
                .id,
            "a"
        );
        let cooled = vec![cooling("a"), account("b", true)];
        assert_eq!(
            runtime
                .select_account(&cooled, RoutingMode::StickyGlobal, false, None, &[])
                .unwrap()
                .id,
            "b"
        );
        let (global, _, _, _) = runtime.snapshot();
        assert_eq!(global.as_deref(), Some("b"));
    }

    #[test]
    fn round_robin_cycles_and_skips_unavailable() {
        let runtime = RoutingRuntime::new();
        let accounts = vec![account("a", true), cooling("b"), account("c", true)];
        assert_eq!(
            runtime
                .select_account(&accounts, RoutingMode::RoundRobin, false, None, &[])
                .unwrap()
                .id,
            "a"
        );
        assert_eq!(
            runtime
                .select_account(&accounts, RoutingMode::RoundRobin, false, None, &[])
                .unwrap()
                .id,
            "c"
        );
        assert_eq!(
            runtime
                .select_account(&accounts, RoutingMode::RoundRobin, false, None, &[])
                .unwrap()
                .id,
            "a"
        );
    }

    #[test]
    fn round_robin_cursor_survives_reordering_and_missing_accounts_by_id() {
        let runtime = RoutingRuntime::new();
        let original = vec![account("a", true), account("b", true), account("c", true)];
        assert_eq!(
            runtime
                .select_account(&original, RoutingMode::RoundRobin, false, None, &[])
                .unwrap()
                .id,
            "a"
        );

        let reordered = vec![account("c", true), account("a", true), account("b", true)];
        assert_eq!(
            runtime
                .select_account(&reordered, RoutingMode::RoundRobin, false, None, &[])
                .unwrap()
                .id,
            "b"
        );

        let missing_cursor = vec![account("a", true), account("c", true)];
        assert_eq!(
            runtime
                .select_account(&missing_cursor, RoutingMode::RoundRobin, false, None, &[],)
                .unwrap()
                .id,
            "a"
        );
    }

    #[test]
    fn concurrent_round_robin_selection_updates_one_shared_cursor() {
        let runtime = Arc::new(RoutingRuntime::new());
        let accounts = Arc::new(vec![account("a", true), account("b", true)]);
        let workers = (0..100)
            .map(|_| {
                let runtime = runtime.clone();
                let accounts = accounts.clone();
                std::thread::spawn(move || {
                    runtime
                        .select_account(&accounts, RoutingMode::RoundRobin, false, None, &[])
                        .unwrap()
                        .id
                })
            })
            .collect::<Vec<_>>();
        let selected = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(selected.iter().filter(|id| id.as_str() == "a").count(), 50);
        assert_eq!(selected.iter().filter(|id| id.as_str() == "b").count(), 50);
        let (_, after, _, _) = runtime.snapshot();
        assert_eq!(after.as_deref(), Some("b"));
    }

    #[test]
    fn conversation_sticky_prefers_binding_without_advancing_round_robin() {
        let runtime = RoutingRuntime::new();
        let accounts = vec![account("a", true), account("b", true)];
        let key = "conv-1";
        assert_eq!(
            runtime
                .select_account(&accounts, RoutingMode::RoundRobin, true, Some(key), &[],)
                .unwrap()
                .id,
            "a"
        );
        let (_, after_first, _, _) = runtime.snapshot();
        assert_eq!(after_first.as_deref(), Some("a"));
        assert_eq!(
            runtime
                .select_account(&accounts, RoutingMode::RoundRobin, true, Some(key), &[],)
                .unwrap()
                .id,
            "a"
        );
        let (_, after_second, _, _) = runtime.snapshot();
        assert_eq!(after_second.as_deref(), Some("a"));
    }

    #[test]
    fn conversation_sticky_rebinds_when_bound_account_excluded() {
        let runtime = RoutingRuntime::new();
        let accounts = vec![account("a", true), account("b", true)];
        let key = "conv-2";
        assert_eq!(
            runtime
                .select_account(&accounts, RoutingMode::StrictPriority, true, Some(key), &[],)
                .unwrap()
                .id,
            "a"
        );
        assert_eq!(
            runtime
                .select_account(
                    &accounts,
                    RoutingMode::StrictPriority,
                    true,
                    Some(key),
                    &["a"],
                )
                .unwrap()
                .id,
            "b"
        );
        assert_eq!(
            runtime
                .select_account(&accounts, RoutingMode::StrictPriority, true, Some(key), &[],)
                .unwrap()
                .id,
            "b"
        );
    }

    #[test]
    fn conversation_ttl_expires_bindings() {
        let runtime = RoutingRuntime::new();
        let accounts = vec![account("a", true), account("b", true)];
        runtime.force_bind_age(
            "old",
            "b",
            Instant::now() - CONVERSATION_TTL - Duration::from_secs(1),
        );
        assert_eq!(
            runtime
                .select_account(
                    &accounts,
                    RoutingMode::StrictPriority,
                    true,
                    Some("old"),
                    &[],
                )
                .unwrap()
                .id,
            "a"
        );
    }

    #[test]
    fn conversation_capacity_evicts_least_recently_used() {
        let runtime = RoutingRuntime::new();
        let accounts = vec![account("a", true)];
        for index in 0..=MAX_CONVERSATIONS {
            let key = format!("k{index}");
            runtime
                .select_account(
                    &accounts,
                    RoutingMode::StrictPriority,
                    true,
                    Some(&key),
                    &[],
                )
                .unwrap();
        }
        let (_, _, len, _) = runtime.snapshot();
        assert_eq!(len, MAX_CONVERSATIONS);
    }

    #[test]
    fn conversation_hit_refreshes_lru_order_before_capacity_eviction() {
        let runtime = RoutingRuntime::new();
        let accounts = vec![account("a", true)];
        for index in 0..MAX_CONVERSATIONS {
            let key = format!("k{index}");
            runtime
                .select_account(
                    &accounts,
                    RoutingMode::StrictPriority,
                    true,
                    Some(&key),
                    &[],
                )
                .unwrap();
        }
        runtime
            .select_account(
                &accounts,
                RoutingMode::StrictPriority,
                true,
                Some("k0"),
                &[],
            )
            .unwrap();
        runtime
            .select_account(
                &accounts,
                RoutingMode::StrictPriority,
                true,
                Some("new"),
                &[],
            )
            .unwrap();

        let state = runtime.inner.lock();
        assert!(state.conversations.entries.contains_key("k0"));
        assert!(!state.conversations.entries.contains_key("k1"));
        assert!(state.conversations.entries.contains_key("new"));
        assert_eq!(state.conversations.entries.len(), MAX_CONVERSATIONS);
    }

    #[test]
    fn reset_clears_runtime_state() {
        let runtime = RoutingRuntime::new();
        let accounts = vec![account("a", true), account("b", true)];
        runtime
            .select_account(&accounts, RoutingMode::RoundRobin, true, Some("c1"), &[])
            .unwrap();
        runtime.reset();
        let (global, after, len, _) = runtime.snapshot();
        assert!(global.is_none());
        assert!(after.is_none());
        assert_eq!(len, 0);
        assert_eq!(
            runtime
                .select_account(&accounts, RoutingMode::RoundRobin, false, None, &[])
                .unwrap()
                .id,
            "a"
        );
    }

    #[test]
    fn disabling_conversation_sticky_ignores_existing_bindings() {
        let runtime = RoutingRuntime::new();
        let accounts = vec![account("a", true), account("b", true)];
        assert_eq!(
            runtime
                .select_account(&accounts, RoutingMode::RoundRobin, true, Some("bound"), &[],)
                .unwrap()
                .id,
            "a"
        );
        assert_eq!(
            runtime
                .select_account(
                    &accounts,
                    RoutingMode::RoundRobin,
                    false,
                    Some("bound"),
                    &[],
                )
                .unwrap()
                .id,
            "b"
        );
    }

    #[test]
    fn explicit_header_is_namespaced_by_format_and_model() {
        let mut headers = HeaderMap::new();
        headers.insert(CONVERSATION_HEADER, "  session-1  ".parse().unwrap());
        let body = Bytes::from_static(br#"{"messages":[{"role":"user","content":"hi"}]}"#);
        let chat =
            resolve_conversation_key(ApiFormat::ChatCompletions, "grok-4.5", &headers, &body)
                .unwrap();
        let messages =
            resolve_conversation_key(ApiFormat::Messages, "grok-4.5", &headers, &body).unwrap();
        let other_model =
            resolve_conversation_key(ApiFormat::ChatCompletions, "glm-5.1", &headers, &body)
                .unwrap();
        assert_ne!(chat, messages);
        assert_ne!(chat, other_model);
    }

    #[test]
    fn invalid_explicit_header_falls_back_to_prompt_seed() {
        let mut headers = HeaderMap::new();
        headers.insert(CONVERSATION_HEADER, "   ".parse().unwrap());
        let body = Bytes::from_static(
            br#"{"messages":[{"role":"system","content":"s"},{"role":"user","content":"hello"}]}"#,
        );
        let prompt_key =
            resolve_conversation_key(ApiFormat::ChatCompletions, "grok-4.5", &headers, &body)
                .unwrap();
        let mut good = HeaderMap::new();
        good.insert(CONVERSATION_HEADER, "ok".parse().unwrap());
        let explicit =
            resolve_conversation_key(ApiFormat::ChatCompletions, "grok-4.5", &good, &body).unwrap();
        assert_ne!(prompt_key, explicit);

        let overlong = "x".repeat(MAX_EXPLICIT_ID_LEN + 1);
        let mut long_headers = HeaderMap::new();
        long_headers.insert(CONVERSATION_HEADER, overlong.parse().unwrap());
        assert_eq!(
            resolve_conversation_key(ApiFormat::ChatCompletions, "grok-4.5", &long_headers, &body,)
                .unwrap(),
            prompt_key
        );

        let mut control_headers = HeaderMap::new();
        control_headers.insert(
            CONVERSATION_HEADER,
            axum::http::HeaderValue::from_bytes(b"bad\tid").unwrap(),
        );
        assert_eq!(
            resolve_conversation_key(
                ApiFormat::ChatCompletions,
                "grok-4.5",
                &control_headers,
                &body,
            )
            .unwrap(),
            prompt_key
        );
    }

    #[test]
    fn prompt_seed_ignores_later_history_and_sampling_params() {
        let headers = HeaderMap::new();
        let first = Bytes::from_static(
            br#"{"model":"grok-4.5","temperature":0.2,"messages":[{"role":"system","content":"sys"},{"role":"user","content":"hello"}]}"#,
        );
        let second = Bytes::from_static(
            br#"{"model":"grok-4.5","temperature":0.9,"stream":true,"messages":[{"role":"system","content":"sys"},{"role":"user","content":"hello"},{"role":"assistant","content":"hi"},{"role":"user","content":"again"}]}"#,
        );
        let a = resolve_conversation_key(ApiFormat::ChatCompletions, "grok-4.5", &headers, &first)
            .unwrap();
        let b = resolve_conversation_key(ApiFormat::ChatCompletions, "grok-4.5", &headers, &second)
            .unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn prompt_seed_is_stable_across_object_key_order() {
        let headers = HeaderMap::new();
        let a = Bytes::from_static(
            br#"{"tools":[{"type":"function","function":{"name":"x","parameters":{"b":1,"a":2}}}],"messages":[{"role":"user","content":"hi"}]}"#,
        );
        let b = Bytes::from_static(
            br#"{"messages":[{"content":"hi","role":"user"}],"tools":[{"function":{"parameters":{"a":2,"b":1},"name":"x"},"type":"function"}]}"#,
        );
        assert_eq!(
            resolve_conversation_key(ApiFormat::ChatCompletions, "m", &headers, &a).unwrap(),
            resolve_conversation_key(ApiFormat::ChatCompletions, "m", &headers, &b).unwrap()
        );
    }

    #[test]
    fn prompt_seed_requires_first_user() {
        let headers = HeaderMap::new();
        let body = Bytes::from_static(br#"{"messages":[{"role":"system","content":"only"}]}"#);
        assert!(
            resolve_conversation_key(ApiFormat::ChatCompletions, "m", &headers, &body).is_none()
        );
    }

    #[test]
    fn responses_and_gemini_prompt_seeds_extract_first_user() {
        let headers = HeaderMap::new();
        let responses = Bytes::from_static(
            br#"{"instructions":"sys","input":[{"role":"user","content":"hello"}]}"#,
        );
        assert!(
            resolve_conversation_key(ApiFormat::Responses, "grok-4.5", &headers, &responses)
                .is_some()
        );
        let gemini = Bytes::from_static(
            br#"{"systemInstruction":{"parts":[{"text":"sys"}]},"contents":[{"role":"user","parts":[{"text":"hi"}]}]}"#,
        );
        assert!(
            resolve_conversation_key(ApiFormat::Gemini, "grok-4.5", &headers, &gemini).is_some()
        );
    }
}
