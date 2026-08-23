//! Pure secret-free in-memory selection state machine.
//!
//! A later host adapter computes wall-clock, row, binding, auth, and Free-gate
//! eligibility once and supplies only [`BaseAvailability`]. This module never
//! sees host rows, stored secrets, or process I/O. [`std::time::Instant`] is
//! used only for conversation TTL and LRU recency, and must be supplied by
//! the caller as `now`.
//!
//! Candidate slice order is the authoritative card order. [`select_at`][SelectorState::select_at]
//! returns a [`Selection`] index into that slice; it never clones a candidate.
//! Duplicate account ids are rejected before any state mutation.
//!
//! Items are rust-public only as the cross-crate bridge; a later host facade
//! should keep historical routing-runtime paths crate-private.

use ocg_domain::account::UpstreamChannel;
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

/// Idle conversation bindings expire at this age, inclusive (`>=`).
pub const CONVERSATION_TTL: Duration = Duration::from_secs(30 * 60);

/// Maximum live conversation bindings. Hits refresh LRU recency.
pub const MAX_CONVERSATIONS: usize = 4096;

/// How the state machine walks base-available, non-excluded cards.
///
/// Public only as the cross-crate bridge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[doc(hidden)]
pub enum SelectionPolicy {
    StrictPriority,
    StickyGlobal,
    RoundRobin,
}

/// Adapter-precomputed eligibility for one card. Transient request excludes
/// are a separate `&[&str]` of account ids; they are not encoded here.
///
/// Public only as the cross-crate bridge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[doc(hidden)]
pub enum BaseAvailability {
    Available,
    Unavailable,
}

/// One already-filtered route target. Fields stay private so callers go
/// through [`Self::new`] and the accessors.
///
/// Public only as the cross-crate bridge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[doc(hidden)]
pub struct Candidate<'a> {
    account_id: &'a str,
    channel: UpstreamChannel,
    resolved_model: &'a str,
    base_availability: BaseAvailability,
}

impl<'a> Candidate<'a> {
    pub fn new(
        account_id: &'a str,
        channel: UpstreamChannel,
        resolved_model: &'a str,
        base_availability: BaseAvailability,
    ) -> Self {
        Self {
            account_id,
            channel,
            resolved_model,
            base_availability,
        }
    }

    pub fn account_id(&self) -> &'a str {
        self.account_id
    }

    pub fn channel(&self) -> UpstreamChannel {
        self.channel
    }

    pub fn resolved_model(&self) -> &'a str {
        self.resolved_model
    }

    pub fn base_availability(&self) -> BaseAvailability {
        self.base_availability
    }

    fn is_selectable(&self, exclude_ids: &[&str]) -> bool {
        self.base_availability == BaseAvailability::Available
            && !exclude_ids.contains(&self.account_id)
    }
}

/// Index of the chosen candidate in the caller-supplied slice.
///
/// Public only as the cross-crate bridge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[doc(hidden)]
pub struct Selection {
    candidate_index: usize,
}

impl Selection {
    pub fn candidate_index(&self) -> usize {
        self.candidate_index
    }
}

/// Recoverable selection failure. Duplicate ids are rejected with the first
/// and later slice indices; state is left unchanged.
///
/// Public only as the cross-crate bridge.
#[derive(Debug, Clone, PartialEq, Eq)]
#[doc(hidden)]
pub enum SelectionError {
    DuplicateAccountId { first: usize, duplicate: usize },
}

impl std::fmt::Display for SelectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateAccountId { first, duplicate } => write!(
                f,
                "duplicate account id at candidate index {duplicate} (first seen at {first})"
            ),
        }
    }
}

impl std::error::Error for SelectionError {}

/// Fresh conversation sticky binding. Fields stay private; use the accessors.
///
/// Public only as the cross-crate bridge.
#[derive(Debug, Clone, PartialEq, Eq)]
#[doc(hidden)]
pub struct BindingSnapshot {
    account_id: String,
    channel: UpstreamChannel,
    resolved_model: String,
}

impl BindingSnapshot {
    pub fn account_id(&self) -> &str {
        &self.account_id
    }

    pub fn channel(&self) -> UpstreamChannel {
        self.channel
    }

    pub fn resolved_model(&self) -> &str {
        &self.resolved_model
    }
}

#[derive(Debug, Clone)]
struct ConversationBinding {
    account_id: String,
    channel: UpstreamChannel,
    resolved_model: String,
    last_seen: Instant,
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
}

/// Owned sticky / round-robin / conversation LRU slot. Mutation is `&mut self`;
/// the caller supplies any sharing wrapper.
///
/// Public only as the cross-crate bridge.
#[derive(Debug, Default)]
#[doc(hidden)]
pub struct SelectorState {
    global_account_id: Option<String>,
    round_robin_after: Option<String>,
    conversations: ConversationMap,
}

impl SelectorState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Select one candidate index and update sticky / round-robin / conversation
    /// state. Duplicate account ids are rejected before any mutation.
    pub fn select_at(
        &mut self,
        candidates: &[Candidate<'_>],
        policy: SelectionPolicy,
        conversation_sticky: bool,
        conversation_key: Option<&str>,
        exclude_ids: &[&str],
        now: Instant,
    ) -> Result<Option<Selection>, SelectionError> {
        if let Some((first, duplicate)) = first_duplicate_account_id(candidates) {
            return Err(SelectionError::DuplicateAccountId { first, duplicate });
        }

        if conversation_sticky && let Some(key) = conversation_key {
            let bound = self.conversations.get_fresh(key, now).map(|binding| {
                (
                    binding.account_id.clone(),
                    binding.channel,
                    binding.resolved_model.clone(),
                )
            });
            if let Some((account_id, channel, resolved_model)) = bound
                && let Some(candidate_index) = find_available_index(
                    candidates,
                    &account_id,
                    channel,
                    &resolved_model,
                    exclude_ids,
                )
            {
                return Ok(Some(Selection { candidate_index }));
            }
        }

        let selected = match policy {
            SelectionPolicy::StrictPriority => first_available_index(candidates, exclude_ids),
            SelectionPolicy::StickyGlobal => self.select_sticky_global(candidates, exclude_ids),
            SelectionPolicy::RoundRobin => self.select_round_robin(candidates, exclude_ids),
        };

        if let Some(candidate_index) = selected
            && conversation_sticky
            && let Some(key) = conversation_key
        {
            let candidate = &candidates[candidate_index];
            self.conversations.insert(
                key.to_string(),
                candidate.account_id.to_string(),
                candidate.channel,
                candidate.resolved_model.to_string(),
                now,
            );
        }

        Ok(selected.map(|candidate_index| Selection { candidate_index }))
    }

    /// Read a conversation binding if it is still fresh. A hit refreshes LRU
    /// recency using `now`, matching lookup during [`Self::select_at`].
    pub fn binding_at(&mut self, conversation_key: &str, now: Instant) -> Option<BindingSnapshot> {
        self.conversations
            .get_fresh(conversation_key, now)
            .map(|binding| BindingSnapshot {
                account_id: binding.account_id.clone(),
                channel: binding.channel,
                resolved_model: binding.resolved_model.clone(),
            })
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    fn select_sticky_global(
        &mut self,
        candidates: &[Candidate<'_>],
        exclude_ids: &[&str],
    ) -> Option<usize> {
        if let Some(current_id) = self.global_account_id.clone() {
            if let Some(index) = candidates.iter().position(|candidate| {
                candidate.account_id == current_id && candidate.is_selectable(exclude_ids)
            }) {
                return Some(index);
            }
            let persistently_available = candidates.iter().any(|candidate| {
                candidate.account_id == current_id && candidate.is_selectable(&[])
            });
            let selected = first_available_index(candidates, exclude_ids)?;
            if !persistently_available {
                self.global_account_id = Some(candidates[selected].account_id.to_string());
            }
            return Some(selected);
        }
        let selected = first_available_index(candidates, exclude_ids)?;
        self.global_account_id = Some(candidates[selected].account_id.to_string());
        Some(selected)
    }

    fn select_round_robin(
        &mut self,
        candidates: &[Candidate<'_>],
        exclude_ids: &[&str],
    ) -> Option<usize> {
        if candidates.is_empty() {
            return None;
        }
        let start = self
            .round_robin_after
            .as_ref()
            .and_then(|after| {
                candidates
                    .iter()
                    .position(|candidate| candidate.account_id == *after)
            })
            .map(|index| (index + 1) % candidates.len())
            .unwrap_or(0);
        for offset in 0..candidates.len() {
            let index = (start + offset) % candidates.len();
            if candidates[index].is_selectable(exclude_ids) {
                self.round_robin_after = Some(candidates[index].account_id.to_string());
                return Some(index);
            }
        }
        None
    }
}

fn first_duplicate_account_id(candidates: &[Candidate<'_>]) -> Option<(usize, usize)> {
    let mut first_by_id = HashMap::with_capacity(candidates.len());
    for (index, candidate) in candidates.iter().enumerate() {
        if let Some(first) = first_by_id.insert(candidate.account_id, index) {
            return Some((first, index));
        }
    }
    None
}

fn first_available_index(candidates: &[Candidate<'_>], exclude_ids: &[&str]) -> Option<usize> {
    candidates
        .iter()
        .position(|candidate| candidate.is_selectable(exclude_ids))
}

fn find_available_index(
    candidates: &[Candidate<'_>],
    account_id: &str,
    channel: UpstreamChannel,
    resolved_model: &str,
    exclude_ids: &[&str],
) -> Option<usize> {
    candidates.iter().position(|candidate| {
        candidate.account_id == account_id
            && candidate.channel == channel
            && candidate.resolved_model == resolved_model
            && candidate.is_selectable(exclude_ids)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn availability(available: bool) -> BaseAvailability {
        if available {
            BaseAvailability::Available
        } else {
            BaseAvailability::Unavailable
        }
    }

    fn cand(id: &str, available: bool) -> Candidate<'_> {
        Candidate::new(
            id,
            UpstreamChannel::Go,
            "test-model",
            availability(available),
        )
    }

    fn cand_on<'a>(
        id: &'a str,
        channel: UpstreamChannel,
        model: &'a str,
        available: bool,
    ) -> Candidate<'a> {
        Candidate::new(id, channel, model, availability(available))
    }

    /// Far-future origin so accidental `Instant::now()` inside production
    /// code cannot satisfy conversation hits or TTL arithmetic.
    fn origin() -> Instant {
        Instant::now() + Duration::from_secs(86_400)
    }

    fn pick(
        state: &mut SelectorState,
        candidates: &[Candidate<'_>],
        policy: SelectionPolicy,
        conversation_sticky: bool,
        conversation_key: Option<&str>,
        exclude_ids: &[&str],
        now: Instant,
    ) -> Option<usize> {
        state
            .select_at(
                candidates,
                policy,
                conversation_sticky,
                conversation_key,
                exclude_ids,
                now,
            )
            .expect("candidates must not contain duplicate account ids")
            .map(|selection| selection.candidate_index())
    }

    fn id_at<'a>(candidates: &'a [Candidate<'a>], index: usize) -> &'a str {
        candidates[index].account_id()
    }

    fn production_source(source: &str) -> &str {
        source
            .split("#[cfg(test)]")
            .next()
            .expect("production source precedes tests")
    }

    #[test]
    fn strict_priority_picks_first_available_in_card_order() {
        let mut state = SelectorState::new();
        let now = origin();
        let candidates = [cand("a", false), cand("b", true), cand("c", true)];
        let index = pick(
            &mut state,
            &candidates,
            SelectionPolicy::StrictPriority,
            false,
            None,
            &[],
            now,
        )
        .unwrap();
        assert_eq!(index, 1);
        assert_eq!(id_at(&candidates, index), "b");
    }

    #[test]
    fn unavailable_and_excluded_cards_are_skipped() {
        let mut state = SelectorState::new();
        let now = origin();
        let candidates = [
            cand("disabled", false),
            cand("ready", true),
            cand("also-ready", true),
        ];
        let index = pick(
            &mut state,
            &candidates,
            SelectionPolicy::StrictPriority,
            false,
            None,
            &["ready"],
            now,
        )
        .unwrap();
        assert_eq!(id_at(&candidates, index), "also-ready");
    }

    #[test]
    fn empty_or_all_unselectable_returns_none() {
        let mut state = SelectorState::new();
        let now = origin();
        assert!(
            pick(
                &mut state,
                &[],
                SelectionPolicy::StrictPriority,
                false,
                None,
                &[],
                now,
            )
            .is_none()
        );
        let candidates = [cand("a", false), cand("b", true)];
        assert!(
            pick(
                &mut state,
                &candidates,
                SelectionPolicy::RoundRobin,
                false,
                None,
                &["b"],
                now,
            )
            .is_none()
        );
        assert!(state.round_robin_after.is_none());
    }

    #[test]
    fn sticky_global_keeps_current_when_higher_priority_recovers() {
        let mut state = SelectorState::new();
        let now = origin();
        let first = [cand("a", false), cand("b", true)];
        assert_eq!(
            id_at(
                &first,
                pick(
                    &mut state,
                    &first,
                    SelectionPolicy::StickyGlobal,
                    false,
                    None,
                    &[],
                    now,
                )
                .unwrap()
            ),
            "b"
        );
        let recovered = [cand("a", true), cand("b", true)];
        assert_eq!(
            id_at(
                &recovered,
                pick(
                    &mut state,
                    &recovered,
                    SelectionPolicy::StickyGlobal,
                    false,
                    None,
                    &[],
                    now,
                )
                .unwrap()
            ),
            "b"
        );
        assert_eq!(state.global_account_id.as_deref(), Some("b"));
    }

    #[test]
    fn sticky_global_transient_exclude_does_not_rewrite_global() {
        let mut state = SelectorState::new();
        let now = origin();
        let candidates = [cand("a", true), cand("b", true)];
        assert_eq!(
            id_at(
                &candidates,
                pick(
                    &mut state,
                    &candidates,
                    SelectionPolicy::StickyGlobal,
                    false,
                    None,
                    &[],
                    now,
                )
                .unwrap()
            ),
            "a"
        );
        assert_eq!(
            id_at(
                &candidates,
                pick(
                    &mut state,
                    &candidates,
                    SelectionPolicy::StickyGlobal,
                    false,
                    None,
                    &["a"],
                    now,
                )
                .unwrap()
            ),
            "b"
        );
        assert_eq!(state.global_account_id.as_deref(), Some("a"));
        assert_eq!(
            id_at(
                &candidates,
                pick(
                    &mut state,
                    &candidates,
                    SelectionPolicy::StickyGlobal,
                    false,
                    None,
                    &[],
                    now,
                )
                .unwrap()
            ),
            "a"
        );
    }

    #[test]
    fn sticky_global_switches_when_current_persistently_unavailable() {
        let mut state = SelectorState::new();
        let now = origin();
        let candidates = [cand("a", true), cand("b", true)];
        pick(
            &mut state,
            &candidates,
            SelectionPolicy::StickyGlobal,
            false,
            None,
            &[],
            now,
        );
        let disabled = [cand("a", false), cand("b", true)];
        assert_eq!(
            id_at(
                &disabled,
                pick(
                    &mut state,
                    &disabled,
                    SelectionPolicy::StickyGlobal,
                    false,
                    None,
                    &[],
                    now,
                )
                .unwrap()
            ),
            "b"
        );
        assert_eq!(state.global_account_id.as_deref(), Some("b"));
    }

    #[test]
    fn sticky_global_switches_when_bound_account_is_missing() {
        let mut state = SelectorState::new();
        let now = origin();
        let original = [cand("a", true), cand("b", true)];
        pick(
            &mut state,
            &original,
            SelectionPolicy::StickyGlobal,
            false,
            None,
            &[],
            now,
        );
        let missing = [cand("b", true), cand("c", true)];
        assert_eq!(
            id_at(
                &missing,
                pick(
                    &mut state,
                    &missing,
                    SelectionPolicy::StickyGlobal,
                    false,
                    None,
                    &[],
                    now,
                )
                .unwrap()
            ),
            "b"
        );
        assert_eq!(state.global_account_id.as_deref(), Some("b"));
    }

    #[test]
    fn sticky_global_stores_account_only() {
        let mut state = SelectorState::new();
        let now = origin();
        let first = [cand_on("a", UpstreamChannel::Free, "m1", true)];
        pick(
            &mut state,
            &first,
            SelectionPolicy::StickyGlobal,
            false,
            None,
            &[],
            now,
        );
        let switched_identity = [
            cand_on("a", UpstreamChannel::Go, "m2", true),
            cand("b", true),
        ];
        let index = pick(
            &mut state,
            &switched_identity,
            SelectionPolicy::StickyGlobal,
            false,
            None,
            &[],
            now,
        )
        .unwrap();
        assert_eq!(index, 0);
        assert_eq!(switched_identity[index].channel(), UpstreamChannel::Go);
        assert_eq!(switched_identity[index].resolved_model(), "m2");
        assert_eq!(state.global_account_id.as_deref(), Some("a"));
    }

    #[test]
    fn round_robin_cycles_and_skips_unavailable() {
        let mut state = SelectorState::new();
        let now = origin();
        let candidates = [cand("a", true), cand("b", false), cand("c", true)];
        assert_eq!(
            id_at(
                &candidates,
                pick(
                    &mut state,
                    &candidates,
                    SelectionPolicy::RoundRobin,
                    false,
                    None,
                    &[],
                    now,
                )
                .unwrap()
            ),
            "a"
        );
        assert_eq!(
            id_at(
                &candidates,
                pick(
                    &mut state,
                    &candidates,
                    SelectionPolicy::RoundRobin,
                    false,
                    None,
                    &[],
                    now,
                )
                .unwrap()
            ),
            "c"
        );
        assert_eq!(
            id_at(
                &candidates,
                pick(
                    &mut state,
                    &candidates,
                    SelectionPolicy::RoundRobin,
                    false,
                    None,
                    &[],
                    now,
                )
                .unwrap()
            ),
            "a"
        );
    }

    #[test]
    fn round_robin_cursor_survives_reordering_and_missing_cursor_by_account_id() {
        let mut state = SelectorState::new();
        let now = origin();
        let original = [cand("a", true), cand("b", true), cand("c", true)];
        assert_eq!(
            id_at(
                &original,
                pick(
                    &mut state,
                    &original,
                    SelectionPolicy::RoundRobin,
                    false,
                    None,
                    &[],
                    now,
                )
                .unwrap()
            ),
            "a"
        );

        let reordered = [cand("c", true), cand("a", true), cand("b", true)];
        assert_eq!(
            id_at(
                &reordered,
                pick(
                    &mut state,
                    &reordered,
                    SelectionPolicy::RoundRobin,
                    false,
                    None,
                    &[],
                    now,
                )
                .unwrap()
            ),
            "b"
        );

        let missing_cursor = [cand("a", true), cand("c", true)];
        assert_eq!(
            id_at(
                &missing_cursor,
                pick(
                    &mut state,
                    &missing_cursor,
                    SelectionPolicy::RoundRobin,
                    false,
                    None,
                    &[],
                    now,
                )
                .unwrap()
            ),
            "a"
        );
    }

    #[test]
    fn conversation_sticky_prefers_exact_binding_without_advancing_round_robin() {
        let mut state = SelectorState::new();
        let now = origin();
        let candidates = [cand("a", true), cand("b", true)];
        let key = "conv-1";
        assert_eq!(
            id_at(
                &candidates,
                pick(
                    &mut state,
                    &candidates,
                    SelectionPolicy::RoundRobin,
                    true,
                    Some(key),
                    &[],
                    now,
                )
                .unwrap()
            ),
            "a"
        );
        assert_eq!(state.round_robin_after.as_deref(), Some("a"));
        let later = now + Duration::from_secs(1);
        assert_eq!(
            id_at(
                &candidates,
                pick(
                    &mut state,
                    &candidates,
                    SelectionPolicy::RoundRobin,
                    true,
                    Some(key),
                    &[],
                    later,
                )
                .unwrap()
            ),
            "a"
        );
        assert_eq!(state.round_robin_after.as_deref(), Some("a"));
        let binding = state.binding_at(key, later).unwrap();
        assert_eq!(binding.account_id(), "a");
        assert_eq!(binding.channel(), UpstreamChannel::Go);
        assert_eq!(binding.resolved_model(), "test-model");
    }

    #[test]
    fn conversation_sticky_requires_account_channel_and_resolved_model() {
        let mut state = SelectorState::new();
        let t0 = origin();
        let first = [
            cand_on("a", UpstreamChannel::Free, "m1", true),
            cand("b", true),
        ];
        pick(
            &mut state,
            &first,
            SelectionPolicy::StrictPriority,
            true,
            Some("conv"),
            &[],
            t0,
        );

        let wrong_model = [
            cand_on("a", UpstreamChannel::Free, "m2", true),
            cand("b", true),
        ];
        assert_eq!(
            id_at(
                &wrong_model,
                pick(
                    &mut state,
                    &wrong_model,
                    SelectionPolicy::StrictPriority,
                    true,
                    Some("conv"),
                    &[],
                    t0,
                )
                .unwrap()
            ),
            "a"
        );
        assert_eq!(state.binding_at("conv", t0).unwrap().resolved_model(), "m2");

        let wrong_channel = [
            cand_on("a", UpstreamChannel::Go, "m2", true),
            cand("b", true),
        ];
        assert_eq!(
            id_at(
                &wrong_channel,
                pick(
                    &mut state,
                    &wrong_channel,
                    SelectionPolicy::StrictPriority,
                    true,
                    Some("conv"),
                    &[],
                    t0,
                )
                .unwrap()
            ),
            "a"
        );
        assert_eq!(
            state.binding_at("conv", t0).unwrap().channel(),
            UpstreamChannel::Go
        );
    }

    #[test]
    fn conversation_sticky_rebinds_when_bound_account_excluded() {
        let mut state = SelectorState::new();
        let now = origin();
        let candidates = [cand("a", true), cand("b", true)];
        let key = "conv-2";
        assert_eq!(
            id_at(
                &candidates,
                pick(
                    &mut state,
                    &candidates,
                    SelectionPolicy::StrictPriority,
                    true,
                    Some(key),
                    &[],
                    now,
                )
                .unwrap()
            ),
            "a"
        );
        assert_eq!(
            id_at(
                &candidates,
                pick(
                    &mut state,
                    &candidates,
                    SelectionPolicy::StrictPriority,
                    true,
                    Some(key),
                    &["a"],
                    now,
                )
                .unwrap()
            ),
            "b"
        );
        assert_eq!(
            id_at(
                &candidates,
                pick(
                    &mut state,
                    &candidates,
                    SelectionPolicy::StrictPriority,
                    true,
                    Some(key),
                    &[],
                    now,
                )
                .unwrap()
            ),
            "b"
        );
    }

    #[test]
    fn conversation_miss_falls_through_and_advances_round_robin() {
        let mut state = SelectorState::new();
        let now = origin();
        let candidates = [cand("a", true), cand("b", true)];
        pick(
            &mut state,
            &candidates,
            SelectionPolicy::RoundRobin,
            true,
            Some("conv"),
            &[],
            now,
        );
        assert_eq!(state.round_robin_after.as_deref(), Some("a"));
        let index = pick(
            &mut state,
            &candidates,
            SelectionPolicy::RoundRobin,
            true,
            Some("conv"),
            &["a"],
            now,
        )
        .unwrap();
        assert_eq!(id_at(&candidates, index), "b");
        assert_eq!(state.round_robin_after.as_deref(), Some("b"));
        assert_eq!(state.binding_at("conv", now).unwrap().account_id(), "b");
    }

    #[test]
    fn conversation_ttl_expires_at_inclusive_boundary() {
        let t0 = origin();
        let bind_b = [cand("a", false), cand("b", true)];
        let both = [cand("a", true), cand("b", true)];

        let mut still = SelectorState::new();
        assert_eq!(
            id_at(
                &bind_b,
                pick(
                    &mut still,
                    &bind_b,
                    SelectionPolicy::StrictPriority,
                    true,
                    Some("old"),
                    &[],
                    t0,
                )
                .unwrap()
            ),
            "b"
        );
        assert_eq!(
            id_at(
                &both,
                pick(
                    &mut still,
                    &both,
                    SelectionPolicy::StrictPriority,
                    true,
                    Some("old"),
                    &[],
                    t0 + CONVERSATION_TTL - Duration::from_secs(1),
                )
                .unwrap()
            ),
            "b"
        );

        let mut expired = SelectorState::new();
        pick(
            &mut expired,
            &bind_b,
            SelectionPolicy::StrictPriority,
            true,
            Some("old"),
            &[],
            t0,
        );
        assert!(expired.binding_at("old", t0 + CONVERSATION_TTL).is_none());
        assert_eq!(
            id_at(
                &both,
                pick(
                    &mut expired,
                    &both,
                    SelectionPolicy::StrictPriority,
                    true,
                    Some("old"),
                    &[],
                    t0 + CONVERSATION_TTL,
                )
                .unwrap()
            ),
            "a"
        );
    }

    #[test]
    fn conversation_capacity_evicts_least_recently_used() {
        let mut state = SelectorState::new();
        let now = origin();
        let candidates = [cand("a", true)];
        for index in 0..=MAX_CONVERSATIONS {
            let key = format!("k{index}");
            pick(
                &mut state,
                &candidates,
                SelectionPolicy::StrictPriority,
                true,
                Some(&key),
                &[],
                now,
            );
        }
        assert_eq!(state.conversations.entries.len(), MAX_CONVERSATIONS);
        assert!(state.binding_at("k0", now).is_none());
        assert!(
            state
                .binding_at(&format!("k{MAX_CONVERSATIONS}"), now)
                .is_some()
        );
    }

    #[test]
    fn conversation_lookup_refreshes_lru_before_availability_check() {
        let mut state = SelectorState::new();
        let now = origin();
        let candidates = [cand("a", true), cand("b", true)];
        for index in 0..MAX_CONVERSATIONS {
            let key = format!("k{index}");
            pick(
                &mut state,
                &candidates,
                SelectionPolicy::StrictPriority,
                true,
                Some(&key),
                &[],
                now,
            );
        }
        assert!(
            pick(
                &mut state,
                &candidates,
                SelectionPolicy::StrictPriority,
                true,
                Some("k0"),
                &["a", "b"],
                now,
            )
            .is_none()
        );
        pick(
            &mut state,
            &candidates,
            SelectionPolicy::StrictPriority,
            true,
            Some("new"),
            &[],
            now,
        );
        assert!(state.conversations.entries.contains_key("k0"));
        assert!(!state.conversations.entries.contains_key("k1"));
        assert!(state.conversations.entries.contains_key("new"));
    }

    #[test]
    fn conversation_hit_refreshes_lru_order_before_capacity_eviction() {
        let mut state = SelectorState::new();
        let now = origin();
        let candidates = [cand("a", true)];
        for index in 0..MAX_CONVERSATIONS {
            let key = format!("k{index}");
            pick(
                &mut state,
                &candidates,
                SelectionPolicy::StrictPriority,
                true,
                Some(&key),
                &[],
                now,
            );
        }
        pick(
            &mut state,
            &candidates,
            SelectionPolicy::StrictPriority,
            true,
            Some("k0"),
            &[],
            now,
        );
        pick(
            &mut state,
            &candidates,
            SelectionPolicy::StrictPriority,
            true,
            Some("new"),
            &[],
            now,
        );

        assert!(state.conversations.entries.contains_key("k0"));
        assert!(!state.conversations.entries.contains_key("k1"));
        assert!(state.conversations.entries.contains_key("new"));
        assert_eq!(state.conversations.entries.len(), MAX_CONVERSATIONS);
    }

    #[test]
    fn duplicate_account_ids_are_rejected_before_any_state_mutation() {
        let mut state = SelectorState::new();
        let t0 = origin();
        let unique = [cand("a", true), cand("b", true)];
        pick(
            &mut state,
            &unique,
            SelectionPolicy::RoundRobin,
            true,
            Some("alive"),
            &[],
            t0,
        );
        assert_eq!(state.round_robin_after.as_deref(), Some("a"));
        assert_eq!(state.binding_at("alive", t0).unwrap().account_id(), "a");

        let later = t0 + Duration::from_secs(20 * 60);
        let duplicates = [cand("a", true), cand("b", true), cand("a", true)];
        let error = state
            .select_at(
                &duplicates,
                SelectionPolicy::RoundRobin,
                true,
                Some("alive"),
                &[],
                later,
            )
            .unwrap_err();
        assert_eq!(
            error,
            SelectionError::DuplicateAccountId {
                first: 0,
                duplicate: 2
            }
        );

        assert_eq!(state.round_robin_after.as_deref(), Some("a"));
        assert_eq!(state.global_account_id.as_deref(), None);
        // Lookup was not refreshed at `later`; the original t0 last_seen still
        // expires at t0 + TTL rather than later + TTL.
        assert!(state.binding_at("alive", t0 + CONVERSATION_TTL).is_none());

        let mut sticky = SelectorState::new();
        pick(
            &mut sticky,
            &unique,
            SelectionPolicy::StickyGlobal,
            false,
            None,
            &[],
            t0,
        );
        assert_eq!(sticky.global_account_id.as_deref(), Some("a"));
        assert!(
            sticky
                .select_at(
                    &[cand("x", true), cand("x", true)],
                    SelectionPolicy::StickyGlobal,
                    false,
                    None,
                    &["a"],
                    t0,
                )
                .is_err()
        );
        assert_eq!(sticky.global_account_id.as_deref(), Some("a"));
    }

    #[test]
    fn reset_clears_runtime_state() {
        let mut state = SelectorState::new();
        let now = origin();
        let candidates = [cand("a", true), cand("b", true)];
        pick(
            &mut state,
            &candidates,
            SelectionPolicy::RoundRobin,
            true,
            Some("c1"),
            &[],
            now,
        );
        pick(
            &mut state,
            &candidates,
            SelectionPolicy::StickyGlobal,
            false,
            None,
            &[],
            now,
        );
        state.reset();
        assert!(state.global_account_id.is_none());
        assert!(state.round_robin_after.is_none());
        assert_eq!(state.conversations.entries.len(), 0);
        assert_eq!(
            id_at(
                &candidates,
                pick(
                    &mut state,
                    &candidates,
                    SelectionPolicy::RoundRobin,
                    false,
                    None,
                    &[],
                    now,
                )
                .unwrap()
            ),
            "a"
        );
    }

    #[test]
    fn disabling_conversation_sticky_ignores_existing_bindings() {
        let mut state = SelectorState::new();
        let now = origin();
        let candidates = [cand("a", true), cand("b", true)];
        assert_eq!(
            id_at(
                &candidates,
                pick(
                    &mut state,
                    &candidates,
                    SelectionPolicy::RoundRobin,
                    true,
                    Some("bound"),
                    &[],
                    now,
                )
                .unwrap()
            ),
            "a"
        );
        assert_eq!(
            id_at(
                &candidates,
                pick(
                    &mut state,
                    &candidates,
                    SelectionPolicy::RoundRobin,
                    false,
                    Some("bound"),
                    &[],
                    now,
                )
                .unwrap()
            ),
            "b"
        );
    }

    #[test]
    fn explicit_instant_drives_hits_and_expiry_not_wall_clock() {
        let t0 = origin();
        let bind_b = [cand("a", false), cand("b", true)];
        let both = [cand("a", true), cand("b", true)];

        let mut hit = SelectorState::new();
        pick(
            &mut hit,
            &bind_b,
            SelectionPolicy::StrictPriority,
            true,
            Some("timed"),
            &[],
            t0,
        );
        assert_eq!(
            id_at(
                &both,
                pick(
                    &mut hit,
                    &both,
                    SelectionPolicy::StrictPriority,
                    true,
                    Some("timed"),
                    &[],
                    t0 + Duration::from_nanos(1),
                )
                .unwrap()
            ),
            "b"
        );

        let mut expired = SelectorState::new();
        pick(
            &mut expired,
            &bind_b,
            SelectionPolicy::StrictPriority,
            true,
            Some("timed"),
            &[],
            t0,
        );
        assert_eq!(
            id_at(
                &both,
                pick(
                    &mut expired,
                    &both,
                    SelectionPolicy::StrictPriority,
                    true,
                    Some("timed"),
                    &[],
                    t0 + CONVERSATION_TTL,
                )
                .unwrap()
            ),
            "a"
        );
    }

    #[test]
    fn free_channel_identity_has_no_special_exhaust_gate() {
        let mut state = SelectorState::new();
        let now = origin();
        let candidates = [
            cand_on("free-1", UpstreamChannel::Free, "m-free", true),
            cand_on("go-1", UpstreamChannel::Go, "m-go", true),
        ];
        assert_eq!(
            id_at(
                &candidates,
                pick(
                    &mut state,
                    &candidates,
                    SelectionPolicy::StrictPriority,
                    false,
                    None,
                    &[],
                    now,
                )
                .unwrap()
            ),
            "free-1"
        );
        let gated = [
            cand_on("free-1", UpstreamChannel::Free, "m-free", false),
            cand_on("go-1", UpstreamChannel::Go, "m-go", true),
        ];
        assert_eq!(
            id_at(
                &gated,
                pick(
                    &mut state,
                    &gated,
                    SelectionPolicy::StrictPriority,
                    false,
                    None,
                    &[],
                    now,
                )
                .unwrap()
            ),
            "go-1"
        );
    }

    #[test]
    fn production_selector_source_stays_secret_free() {
        let production = production_source(include_str!("selector.rs"));
        assert!(
            production.contains("use ocg_domain::account::UpstreamChannel"),
            "selector.rs must import UpstreamChannel from ocg_domain::account"
        );
        assert!(
            production.contains("now: Instant"),
            "select_at / binding_at must take an explicit Instant"
        );
        assert!(
            !production.contains("Instant::now"),
            "production selector must not read the wall clock"
        );
        for needle in [
            "CoreState",
            "Database",
            "reqwest",
            "rusqlite",
            "tokio",
            "axum",
            "chrono",
            "ocg_core",
            "std::fs",
            "std::net",
            "std::env",
            "std::process",
            "include!",
            "KeyCipher",
            "decrypt_key",
            "key_cipher",
            "parking_lot",
            "free_channel_exhausted",
            "Mutex",
            "anyhow::",
            "password",
            "cooldown",
            "credentials",
        ] {
            assert!(
                !production.contains(needle),
                "production ocg-gateway selector source must not name `{needle}`"
            );
        }
    }
}
