# V3 Host characterization — CLI and Tauri mutation/lifecycle matrix

Deterministic offline tests only. Current Host behavior is frozen, including
differences from Dashboard CAS and Custom enablement. Do not treat these
differences as bugs in Stage 0.

CLI tests live in `crates/ocg-cli/src/main.rs` (`#[cfg(test)]`).
Tauri tests live in `src-tauri/src/**` `#[cfg(test)]` modules.
The same matrix is copied at `src-tauri/tests/fixtures/v3/host_requirement_map.md`.

| Kind | Meaning |
| --- | --- |
| Behavioral | Real command/function call |
| Source-text | Labeled; crate construction cannot invoke the behavior |

## CLI

| Requirement | Status | Tests |
| --- | --- | --- |
| Surface is `serve` / `key` / `status` only (no settings, sub keys, Custom create flags) | New | Behavioral: `cli_command_surface_is_serve_key_status_only` |
| `key add` always persists enabled ready OpenCode Go, never Custom | Existing + new | Behavioral: `key_lifecycle_and_status_cover_cli_account_commands`, `cli_key_mutations_do_not_bump_a_live_serve_revision`. Source-text: `cli_production_source_has_no_cas_custom_create_or_usage_loop_control` |
| `key list` / `status` are credential-oriented and exclude Zen Free | Existing | Behavioral: `key_lifecycle_and_status_cover_cli_account_commands`, `cli_key_operations_reject_the_provider_owned_zen_singleton` |
| `key enable/disable/remove` reject Zen Free without mutation | Existing | Behavioral: `cli_key_operations_reject_the_provider_owned_zen_singleton` |
| Unroutable catalog plans fail closed on enable (no `updated_at` change) | Existing | Behavioral: `cli_enable_rejects_unroutable_catalog_plans_without_mutation` |
| CLI enablement does **not** consult Custom verification; pending Custom can be enabled | New | Behavioral: `cli_enables_pending_custom_without_dashboard_verification` |
| Account mutations do **not** bump `settings_revision` (no Dashboard CAS) | New | Behavioral: `cli_key_mutations_do_not_bump_a_live_serve_revision` (out-of-process `key_command` against a live `start_serve` CoreState), in-process `toggle_account`. Source-text: no `bump_settings_revision` / `expected_revision` in production |
| Direct `Database::update_account` (the write `toggle_account` uses) does not bump revision | New | Behavioral: `cli_update_shaped_writes_skip_revision_unlike_dashboard` |
| `serve --port` persists via `set_config` (this **does** bump that process's revision); no port skips `set_config` | Existing + source-text | Behavioral: `start_serve_binds_port_persists_override_and_stops_cleanly`. Source-text: `if let Some(port)` + `set_config` |
| `stop_serve` clears only the gateway listener; CoreState remains usable; no usage-loop cancel | Existing + new | Behavioral: `start_serve_binds_port_persists_override_and_stops_cleanly`, `cli_key_mutations_do_not_bump_a_live_serve_revision`. Source-text: production has no `usage_sync` |
| `key ping` hits configured upstream (Go keys only); empty/disabled targets are no-ops; decrypt failure is printed, not a process error | Existing | Behavioral: `ping_keys_hits_configured_upstream_and_handles_empty_targets` (loopback mock, not a live vendor) |
| Cipher / data-dir / dashboard-dir resolution | Existing | Behavioral: `resolve_cipher_uses_explicit_env_then_file`, `resolve_data_dir_prefers_explicit_path`, `dashboard_dir_prefers_explicit_then_existing_packaged_dist` |
| Incomplete managed setup cannot enable or ping | Existing | Behavioral: `key_lifecycle_and_status_cover_cli_account_commands` |
| Browser-profile staging on `key remove` | Existing | Behavioral: `key_lifecycle_and_status_cover_cli_account_commands` |

## Tauri commands

| Requirement | Status | Tests |
| --- | --- | --- |
| Account create/update/toggle/delete/test/cooldown do **not** bump `settings_revision` and have no `expected_revision` | New | Behavioral: `tauri_account_mutations_skip_cas_and_enable_pending_custom` |
| Go create persists `enabled=true` | Existing | Behavioral: `account_command_inners_cover_lifecycle`, `unroutable_offerings_cannot_be_enabled_through_account_commands` |
| Custom create persists `enabled=true` + verification `pending` **without** custom_config/capabilities (Dashboard stays disabled pending with a contract) | New | Behavioral: `tauri_account_mutations_skip_cas_and_enable_pending_custom` |
| Custom toggle/update can re-enable while verification is still pending | New | Behavioral: `tauri_account_mutations_skip_cas_and_enable_pending_custom` |
| Unroutable offerings save disabled and cannot enable | Existing | Behavioral: `unroutable_offerings_cannot_be_enabled_through_account_commands` |
| Zen Free singleton cannot be created through the account command | Existing | Behavioral: `account_command_inners_cover_lifecycle` |
| `test_account` decrypts/masks locally; it does not ping upstream or require a gateway | New | Behavioral: `tauri_account_mutations_skip_cas_and_enable_pending_custom` (`core.gateway` stays `None`) |
| Settings update has **no CAS**; empty primary and sub-key collision reject without revision change | New | Behavioral: `tauri_settings_have_no_cas_and_skip_dashboard_write_gates` |
| Settings write skips Dashboard-only gates: proxy-list membership, auto-start/dock availability, preserving Claude Desktop models | New | Behavioral: `tauri_settings_have_no_cas_and_skip_dashboard_write_gates` |
| Successful settings save and primary rotation **do** bump revision via `set_config` and refresh the credential snapshot | Existing + new | Behavioral: `settings_inners_update_and_regenerate_without_autostart_side_effects`, `tauri_settings_have_no_cas_and_skip_dashboard_write_gates` |
| Port change restarts a running gateway; failed bind keeps the old listener and config | Existing | Behavioral: `settings_port_change_restarts_running_gateway_or_keeps_old_on_failure`, `failed_port_change_keeps_old_gateway_running` |
| Gateway restart/stop is a **listener** lifecycle and does not bump `settings_revision` | New | Behavioral: `restart_inner_is_a_listener_lifecycle_and_does_not_bump_revision` |
| Usage worker is process-level (`CoreState`), not cancelled by gateway stop | Source-text + core | Source-text: `desktop_capabilities_and_exit_are_separate_from_gateway_and_updater`. Behavioral coverage of the once-per-`CoreState` loop remains `ocg-core` `spawn_usage_sync_loop_starts_once_per_core_state` (`loop_started` is private) |
| Desktop capabilities (window/tray, auto-start hook, Dock hook) are a third lifecycle | Source-text | `desktop_capabilities_and_exit_are_separate_from_gateway_and_updater`; Windows `autostart::startup_value_quotes_exe_and_sets_silent_arg` |
| Updater is **not** a WebView/`invoke` command; `configure` registers a CoreState starter | Source-text | `updater_is_not_a_webview_command` (handler list + `capabilities/default.json`) |
| Updater proxy follows process-wide default-leg policy | Existing | Behavioral: `updater_follows_the_process_wide_proxy_policy`, `updater_follows_the_list_mode_default_leg_per_direction` |
| Tauri dashboard summary counts Go + Zen Free only; enabled pending Custom is not "available" | Existing + new | Behavioral: `dashboard_summary_excludes_goat_and_honors_zen_free_cooldown`, `dashboard_summary_excludes_enabled_pending_custom` |
| Legacy `open_browser` recovers staged profiles, rejects missing accounts, HTTPS-only, no CDP flags | Existing + new | Behavioral: `legacy_open_recovers_staged_profile_before_native_launch`, `legacy_open_rejects_missing_account_without_launching`, `browser_arguments_use_only_profile_and_non_automation_flags`, `browser_url_requires_https_without_credentials` |

## Frozen Host vs Dashboard differences (do not repair in Stage 0)

| Behavior | Dashboard HTTP | CLI | Tauri command |
| --- | --- | --- | --- |
| `expected_revision` CAS / `bump_settings_revision` on account writes | Yes | No | No |
| Custom create | Disabled + pending + contract | Cannot create (Go-only `key add`) | Enabled + pending, **no** contract |
| Enable Custom while pending | `409` verify-first | Allowed | Allowed |
| Proxy-list member validation | Write gate | No settings command | Persists unknown ids |
| Auto-start / Dock availability gate | Rejects unsupported flips | N/A | Persists flags; `sync_auto_start` only when the command path passes `true` |
| Claude Desktop models | Preserved on settings update | N/A | Written from the `AppConfig` payload |
| Sub gateway keys | Lifecycle API | None | None (primary uniqueness gate still shared) |
| Account ping | Upstream | Upstream (`key ping`) | Local decrypt/mask only |
| Revision token storage | In-memory per `CoreState` | New process ⇒ new epoch | New process ⇒ new epoch |

## Untestable in this slice (without production edits or live runtimes)

| Gap | Why |
| --- | --- |
| Live `tauri::command` `invoke` through a WebView | Needs an AppHandle / packaged runtime. Inners (`*_inner`) are the behavioral stand-in. |
| `reset_browser_profile` command body | Not extracted to an inner; cannot call without `State<'_, AppState>`. |
| `UsageSyncRuntime.loop_started` from Host crates | Field is private in ocg-core; Host freeze is source-text + listener-stop behavior. |
| Windows `autostart::sync(true)` | Would mutate `HKCU\...\Run` on the host; formatting is unit-tested. |
| macOS Dock visibility sync | Requires a live `AppHandle::set_dock_visibility`. |
| `updater::install_update` / GitHub `latest.json` | Live network + signing key + AppHandle. Proxy policy is unit-tested. |
| Process `RunEvent::ExitRequested` | Needs the Tauri event loop. Source-text: exit stops the listener only. |
| CLI argv → `main` binary entry | Tests call `key_command` / `start_serve` / clap parse, not `std::process::Command` of the built binary. |
| Cross-process Dashboard HTTP CAS against a CLI/Tauri host | Would require a running dashboard session in this crate. Covered in ocg-core `v3_control_plane` for Database-shaped writes; Host tests freeze the actual CLI/Tauri functions. |
