# V3 Host characterization — CLI and Tauri mutation/lifecycle matrix

Current V3: CLI mutations call the shared ocg-core control-plane services and
revision semantics. Desktop Host capabilities compose ocg-core Native Browser,
Gateway Lifecycle, Desktop Settings, and Updater services. Tauri has no WebView
`invoke` command surface.

CLI tests live in `crates/ocg-cli/src/main.rs` (`#[cfg(test)]`).
Tauri tests live in `src-tauri/src/**` `#[cfg(test)]` modules.
The same matrix is copied at `src-tauri/tests/fixtures/v3/host_requirement_map.md`.

| Kind | Meaning |
| --- | --- |
| Behavioral | Real command/function call |
| Source-text | Labeled source-level assertion, including absence of a Tauri command surface |

## CLI

| Requirement | Status | Tests |
| --- | --- | --- |
| Surface is `serve` / `key` / `status` only (no settings, sub keys, Custom create flags) | Existing | Behavioral: `cli_command_surface_is_serve_key_status_only` |
| `key add` persists enabled ready OpenCode Go through `account_control::create_go_api_key` and bumps that process's `settings_revision` | Implemented | Behavioral: `key_lifecycle_and_status_cover_cli_account_commands`, `go_create_and_toggle_bump_revision_and_allow_pending_custom`. Source-text: `cli_production_source_has_no_cas_custom_create_or_usage_loop_control` |
| `key list` / `status` are credential-oriented and exclude Zen Free | Existing | Behavioral: `key_lifecycle_and_status_cover_cli_account_commands`, `cli_key_operations_reject_the_provider_owned_zen_singleton` |
| `key enable/disable/remove` reject Zen Free without mutation | Existing | Behavioral: `cli_key_operations_reject_the_provider_owned_zen_singleton` |
| Unroutable catalog plans fail closed on enable (no `updated_at` change) | Existing | Behavioral: `cli_enable_rejects_unroutable_catalog_plans_without_mutation` |
| CLI enablement leaves Custom verification optional; pending Custom may be enabled | Implemented | Behavioral: `cli_enable_allows_pending_custom_without_verification` |
| In-process account mutations bump `settings_revision` via the shared control-plane service (no `expectedRevision` on the CLI argv) | Implemented | Behavioral: `cli_key_mutations_share_control_plane_revision_in_process`. Out-of-process `key_command` against a live `start_serve` CoreState still cannot bump that other process's in-memory token |
| Legacy direct `Database::update_account` writes do not bump revision | Existing | Behavioral: `cli_update_shaped_writes_skip_revision_unlike_dashboard`; current CLI mutations use `account_control` |
| `serve --port` persists via `set_config` (this **does** bump that process's revision); no port skips `set_config` | Existing + source-text | Behavioral: `start_serve_binds_port_persists_override_and_stops_cleanly`. Source-text: `if let Some(port)` + `set_config` |
| `stop_serve` uses `GatewayLifecycle::stop_and_wait`; CoreState remains usable; no usage-loop cancel | Implemented | Behavioral: `start_serve_binds_port_persists_override_and_stops_cleanly`, `cli_key_mutations_share_control_plane_revision_in_process`. Source-text: production has no `usage_sync` |
| `key ping` hits configured upstream (Go keys only); empty/disabled targets are no-ops; decrypt failure is printed, not a process error | Existing | Behavioral: `ping_keys_hits_configured_upstream_and_handles_empty_targets` (loopback mock, not a live vendor) |
| Cipher / data-dir / dashboard-dir resolution | Existing | Behavioral: `resolve_cipher_uses_explicit_env_then_file`, `resolve_data_dir_prefers_explicit_path`, `dashboard_dir_prefers_explicit_then_existing_packaged_dist` |
| Incomplete managed setup cannot enable or ping | Existing | Behavioral: `key_lifecycle_and_status_cover_cli_account_commands` |
| Browser-profile staging on `key remove` | Existing | Behavioral: `key_lifecycle_and_status_cover_cli_account_commands` |

## Tauri host capabilities

| Requirement | Status | Tests |
| --- | --- | --- |
| No WebView `invoke` / `generate_handler` commands remain | Implemented | Source-text: `no_tauri_invoke_commands_remain`, `updater_is_not_a_webview_command` |
| Native Browser hooks are owned by the host module | Implemented | Source-text: `native_browser_hooks_are_owned_by_the_host_module`. Behavioral: native_browser argument/URL tests |
| Gateway start uses `start_gateway` (usage-sync once per CoreState); exit/stop is listener-only | Implemented | Source-text: `desktop_capabilities_and_exit_are_separate_from_gateway_and_updater`. Behavioral: ocg-core `settings_port_rebind_keeps_started_usage_loop` |
| HTTP settings port change rebinds through `GatewayLifecycle` without awaiting the serving listener; persist → rebind → compensation is serialized by `settings_host_effects` with config-fingerprint compensation | Implemented | Behavioral: `http_v3_port_change_rebinds_running_listener_or_keeps_old_on_failure`, `concurrent_failed_port_write_does_not_clobber_successful_timeout_write`, `concurrent_port_changes_keep_configured_and_active_ports_in_agreement`. Source-text: `v2_and_v3_settings_updates_share_the_core_state_host_path` |
| Desktop capabilities (window/tray, auto-start hook, Dock hook) are a third lifecycle | Existing | `desktop_capabilities_and_exit_are_separate_from_gateway_and_updater`; Windows `autostart::startup_value_quotes_exe_and_sets_silent_arg` |
| Updater is **not** a WebView/`invoke` command; `configure` registers a CoreState starter | Existing | `updater_is_not_a_webview_command` (no handler list + `capabilities/default.json`) |
| Updater proxy follows process-wide default-leg policy | Existing | Behavioral: `updater_follows_the_process_wide_proxy_policy`, `updater_follows_the_list_mode_default_leg_per_direction` |
| Native browser arguments use only profile and non-automation flags; HTTPS-only | Existing | Behavioral: `browser_arguments_use_only_profile_and_non_automation_flags`, `browser_url_requires_https_without_credentials` |

## Current Host vs Dashboard

| Behavior | Dashboard HTTP | CLI | Tauri host |
| --- | --- | --- | --- |
| Account writes bump `settings_revision` | Yes (CAS) | Yes, via `account_control` (no argv CAS) | N/A (HTTP only) |
| Custom create | Enabled + pending + contract | Cannot create (Go-only `key add`) | N/A (HTTP only) |
| Enable Custom while pending | Allowed; verification remains pending | Allowed; verification remains pending | N/A (HTTP only) |
| Settings port change | Rebind through GatewayLifecycle | `serve --port` persists then binds | HTTP settings rebind; host start/stop listener-only |
| Primary and sub Keys | V3 lifecycle API; schema-v27 `access_keys` rows | None | None |
| Account ping | Upstream | Upstream (`key ping`) | N/A |
| Revision token storage | In-memory per `CoreState` | New process ⇒ new epoch | New process ⇒ new epoch |

## Not covered by this fixture matrix (without production edits or live runtimes)

| Gap | Why |
| --- | --- |
| Live packaged WebView | Needs an AppHandle / packaged runtime. Host registration is source-text. |
| `UsageSyncRuntime.loop_started` from Host crates | Field is private in ocg-core; proven in ocg-core `settings_port_rebind_keeps_started_usage_loop` |
| Windows `autostart::sync(true)` | Would mutate `HKCU\...\Run` on the host; formatting is unit-tested. |
| macOS Dock visibility sync | Requires a live `AppHandle::set_dock_visibility`. |
| `updater::install_update` / GitHub `latest.json` | Live network + signing key + AppHandle. Proxy policy is unit-tested. |
| Process `RunEvent::ExitRequested` | Needs the Tauri event loop. Source-text: exit stops the listener only. |
| CLI argv → `main` binary entry | Tests call `key_command` / `start_serve` / clap parse, not `std::process::Command` of the built binary. |
