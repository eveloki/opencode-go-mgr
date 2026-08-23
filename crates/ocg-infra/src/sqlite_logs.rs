//! Neutral SQLite statement helpers for forward and gateway logs.
//!
//! Each helper runs exactly one explicit v26 statement on a caller-owned
//! connection and returns the raw rusqlite result. Callers own timestamps,
//! diagnostics serialization, cost policy, redaction, and transactions.

use rusqlite::{Connection, params};

const INSERT_FORWARD_LOG_SQL: &str = "INSERT INTO forward_logs
             (timestamp, model, account_id, account_name, client_key_id, client_key_name,
              route_account_id, provider_id, offering_id, credential_account_id,
              status, http_status, route,
              prompt_tokens, completion_tokens, cached_tokens, cache_creation_tokens, cost,
              raw_cost_usd, quota_debit, effective_paid_cost_usd,
              pricing_revision_id, quota_multiplier, local_adjustment_multiplier,
              service_tier, cost_state, error_message, request_id, attempt,
              error_source, error_stage, duration_ms, diagnostic_json,
              requested_model, resolved_alias, upstream_model,
              native_cost_value, native_cost_unit, native_cost_currency)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16,
                     ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30,
                     ?31, ?32, ?33, ?34, ?35, ?36, ?37, ?38, ?39)";

const UPDATE_FORWARD_LOG_SQL: &str = "UPDATE forward_logs
             SET status = ?2,
                 http_status = COALESCE(?3, http_status),
                 prompt_tokens = ?4,
                 completion_tokens = ?5,
                 cached_tokens = ?6,
                 cache_creation_tokens = ?7,
                 cost = ?8,
                 raw_cost_usd = ?9,
                 quota_debit = ?10,
                 effective_paid_cost_usd = ?11,
                 pricing_revision_id = ?12,
                 quota_multiplier = ?13,
                 local_adjustment_multiplier = ?14,
                 service_tier = ?15,
                 cost_state = ?16,
                 error_message = COALESCE(?17, error_message),
                 error_source = COALESCE(?18, error_source),
                 error_stage = COALESCE(?19, error_stage),
                 duration_ms = COALESCE(?20, duration_ms),
                 diagnostic_json = COALESCE(?21, diagnostic_json),
                 native_cost_value = ?22,
                 native_cost_unit = ?23,
                 native_cost_currency = ?24
             WHERE id = ?1";

const PATCH_FORWARD_LOG_IDENTITY_SQL: &str = "UPDATE forward_logs SET
                requested_model = ?2,
                resolved_alias = ?3,
                upstream_model = ?4,
                native_cost_value = ?5,
                native_cost_unit = ?6,
                native_cost_currency = ?7
             WHERE id = ?1";

const INSERT_GATEWAY_LOG_SQL: &str = "INSERT INTO gateway_logs
             (level, category, message, created_at, request_id, attempt,
              error_source, error_stage, duration_ms, diagnostic_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)";

/// Borrowed v26 `forward_logs` insert payload. Field order matches the
/// 39-column statement binding order.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ForwardLogInsertRow<'a> {
    pub timestamp: &'a str,
    pub model: &'a str,
    pub account_id: &'a str,
    pub account_name: &'a str,
    pub client_key_id: Option<&'a str>,
    pub client_key_name: Option<&'a str>,
    pub route_account_id: Option<&'a str>,
    pub provider_id: Option<&'a str>,
    pub offering_id: Option<&'a str>,
    pub credential_account_id: Option<&'a str>,
    pub status: &'a str,
    pub http_status: Option<i32>,
    pub route: &'a str,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub cached_tokens: i64,
    pub cache_creation_tokens: i64,
    pub cost: f64,
    pub raw_cost_usd: Option<f64>,
    pub quota_debit: Option<f64>,
    pub effective_paid_cost_usd: Option<f64>,
    pub pricing_revision_id: Option<&'a str>,
    pub quota_multiplier: Option<f64>,
    pub local_adjustment_multiplier: Option<f64>,
    pub service_tier: Option<&'a str>,
    pub cost_state: &'a str,
    pub error_message: Option<&'a str>,
    pub request_id: Option<&'a str>,
    pub attempt: Option<i64>,
    pub error_source: Option<&'a str>,
    pub error_stage: Option<&'a str>,
    pub duration_ms: Option<i64>,
    pub diagnostic_json: Option<&'a str>,
    pub requested_model: Option<&'a str>,
    pub resolved_alias: Option<&'a str>,
    pub upstream_model: Option<&'a str>,
    pub native_cost_value: Option<f64>,
    pub native_cost_unit: Option<&'a str>,
    pub native_cost_currency: Option<&'a str>,
}

/// Borrowed v26 `forward_logs` finalize payload. `None` http/error/diagnostic
/// fields leave the stored value unchanged via SQL `COALESCE`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ForwardLogUpdateRow<'a> {
    pub id: i64,
    pub status: &'a str,
    pub http_status: Option<i32>,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub cached_tokens: i64,
    pub cache_creation_tokens: i64,
    pub cost: f64,
    pub raw_cost_usd: Option<f64>,
    pub quota_debit: Option<f64>,
    pub effective_paid_cost_usd: Option<f64>,
    pub pricing_revision_id: Option<&'a str>,
    pub quota_multiplier: Option<f64>,
    pub local_adjustment_multiplier: Option<f64>,
    pub service_tier: Option<&'a str>,
    pub cost_state: &'a str,
    pub error_message: Option<&'a str>,
    pub error_source: Option<&'a str>,
    pub error_stage: Option<&'a str>,
    pub duration_ms: Option<i64>,
    pub diagnostic_json: Option<&'a str>,
    pub native_cost_value: Option<f64>,
    pub native_cost_unit: Option<&'a str>,
    pub native_cost_currency: Option<&'a str>,
}

/// Borrowed six-column native identity patch for `forward_logs`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ForwardLogIdentityPatch<'a> {
    pub id: i64,
    pub requested_model: Option<&'a str>,
    pub resolved_alias: Option<&'a str>,
    pub upstream_model: Option<&'a str>,
    pub native_cost_value: Option<f64>,
    pub native_cost_unit: Option<&'a str>,
    pub native_cost_currency: Option<&'a str>,
}

/// Borrowed v26 `gateway_logs` insert payload, including optional diagnostics.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GatewayLogInsertRow<'a> {
    pub level: &'a str,
    pub category: &'a str,
    pub message: &'a str,
    pub created_at: &'a str,
    pub request_id: Option<&'a str>,
    pub attempt: Option<i64>,
    pub error_source: Option<&'a str>,
    pub error_stage: Option<&'a str>,
    pub duration_ms: Option<i64>,
    pub diagnostic_json: Option<&'a str>,
}

/// Insert one `forward_logs` row and return its auto-assigned id.
pub fn insert_forward_log(
    conn: &Connection,
    row: &ForwardLogInsertRow<'_>,
) -> rusqlite::Result<i64> {
    conn.execute(
        INSERT_FORWARD_LOG_SQL,
        params![
            row.timestamp,
            row.model,
            row.account_id,
            row.account_name,
            row.client_key_id,
            row.client_key_name,
            row.route_account_id,
            row.provider_id,
            row.offering_id,
            row.credential_account_id,
            row.status,
            row.http_status,
            row.route,
            row.prompt_tokens,
            row.completion_tokens,
            row.cached_tokens,
            row.cache_creation_tokens,
            row.cost,
            row.raw_cost_usd,
            row.quota_debit,
            row.effective_paid_cost_usd,
            row.pricing_revision_id,
            row.quota_multiplier,
            row.local_adjustment_multiplier,
            row.service_tier,
            row.cost_state,
            row.error_message,
            row.request_id,
            row.attempt,
            row.error_source,
            row.error_stage,
            row.duration_ms,
            row.diagnostic_json,
            row.requested_model,
            row.resolved_alias,
            row.upstream_model,
            row.native_cost_value,
            row.native_cost_unit,
            row.native_cost_currency,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Finalize one `forward_logs` row. Returns the number of affected rows.
pub fn update_forward_log(
    conn: &Connection,
    row: &ForwardLogUpdateRow<'_>,
) -> rusqlite::Result<usize> {
    conn.execute(
        UPDATE_FORWARD_LOG_SQL,
        params![
            row.id,
            row.status,
            row.http_status,
            row.prompt_tokens,
            row.completion_tokens,
            row.cached_tokens,
            row.cache_creation_tokens,
            row.cost,
            row.raw_cost_usd,
            row.quota_debit,
            row.effective_paid_cost_usd,
            row.pricing_revision_id,
            row.quota_multiplier,
            row.local_adjustment_multiplier,
            row.service_tier,
            row.cost_state,
            row.error_message,
            row.error_source,
            row.error_stage,
            row.duration_ms,
            row.diagnostic_json,
            row.native_cost_value,
            row.native_cost_unit,
            row.native_cost_currency,
        ],
    )
}

/// Patch the six native identity columns. Returns the number of affected rows.
pub fn patch_forward_log_identity(
    conn: &Connection,
    row: &ForwardLogIdentityPatch<'_>,
) -> rusqlite::Result<usize> {
    conn.execute(
        PATCH_FORWARD_LOG_IDENTITY_SQL,
        params![
            row.id,
            row.requested_model,
            row.resolved_alias,
            row.upstream_model,
            row.native_cost_value,
            row.native_cost_unit,
            row.native_cost_currency,
        ],
    )
}

/// Insert one `gateway_logs` row. Returns the number of affected rows.
pub fn insert_gateway_log(
    conn: &Connection,
    row: &GatewayLogInsertRow<'_>,
) -> rusqlite::Result<usize> {
    conn.execute(
        INSERT_GATEWAY_LOG_SQL,
        params![
            row.level,
            row.category,
            row.message,
            row.created_at,
            row.request_id,
            row.attempt,
            row.error_source,
            row.error_stage,
            row.duration_ms,
            row.diagnostic_json,
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::types::Value;
    use std::fs;
    use std::path::{Path, PathBuf};

    const FORWARD_INSERT_COLUMNS: [&str; 39] = [
        "timestamp",
        "model",
        "account_id",
        "account_name",
        "client_key_id",
        "client_key_name",
        "route_account_id",
        "provider_id",
        "offering_id",
        "credential_account_id",
        "status",
        "http_status",
        "route",
        "prompt_tokens",
        "completion_tokens",
        "cached_tokens",
        "cache_creation_tokens",
        "cost",
        "raw_cost_usd",
        "quota_debit",
        "effective_paid_cost_usd",
        "pricing_revision_id",
        "quota_multiplier",
        "local_adjustment_multiplier",
        "service_tier",
        "cost_state",
        "error_message",
        "request_id",
        "attempt",
        "error_source",
        "error_stage",
        "duration_ms",
        "diagnostic_json",
        "requested_model",
        "resolved_alias",
        "upstream_model",
        "native_cost_value",
        "native_cost_unit",
        "native_cost_currency",
    ];

    const V26_LOG_DDL: &str = "
        CREATE TABLE forward_logs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp TEXT NOT NULL,
            model TEXT NOT NULL,
            account_id TEXT NOT NULL,
            account_name TEXT NOT NULL,
            client_key_id TEXT,
            client_key_name TEXT,
            route_account_id TEXT,
            provider_id TEXT,
            offering_id TEXT,
            credential_account_id TEXT,
            status TEXT NOT NULL,
            http_status INTEGER,
            route TEXT NOT NULL DEFAULT '',
            prompt_tokens INTEGER NOT NULL DEFAULT 0,
            completion_tokens INTEGER NOT NULL DEFAULT 0,
            cached_tokens INTEGER NOT NULL DEFAULT 0,
            cache_creation_tokens INTEGER NOT NULL DEFAULT 0,
            cost REAL NOT NULL DEFAULT 0,
            raw_cost_usd REAL,
            quota_debit REAL,
            effective_paid_cost_usd REAL,
            pricing_revision_id TEXT,
            quota_multiplier REAL,
            local_adjustment_multiplier REAL,
            service_tier TEXT,
            cost_state TEXT NOT NULL DEFAULT 'not_applicable',
            error_message TEXT,
            request_id TEXT,
            attempt INTEGER,
            error_source TEXT,
            error_stage TEXT,
            duration_ms INTEGER,
            diagnostic_json TEXT,
            requested_model TEXT,
            resolved_alias TEXT,
            upstream_model TEXT,
            native_cost_value REAL,
            native_cost_unit TEXT,
            native_cost_currency TEXT
        );
        CREATE TABLE gateway_logs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            level TEXT NOT NULL,
            category TEXT NOT NULL,
            message TEXT NOT NULL,
            created_at TEXT NOT NULL,
            request_id TEXT,
            attempt INTEGER,
            error_source TEXT,
            error_stage TEXT,
            duration_ms INTEGER,
            diagnostic_json TEXT
        );
    ";

    fn v26_conn() -> Connection {
        let conn = Connection::open_in_memory().expect("in-memory sqlite");
        conn.execute_batch(V26_LOG_DDL)
            .expect("v26 log tables should create");
        conn
    }

    fn sentinel_insert_row() -> ForwardLogInsertRow<'static> {
        ForwardLogInsertRow {
            timestamp: "ts-1",
            model: "model-2",
            account_id: "acct-3",
            account_name: "name-4",
            client_key_id: Some("key-5"),
            client_key_name: Some("keyname-6"),
            route_account_id: Some("route-acct-7"),
            provider_id: Some("prov-8"),
            offering_id: Some("off-9"),
            credential_account_id: Some("cred-10"),
            status: "status-11",
            http_status: Some(12),
            route: "proxy",
            prompt_tokens: 14,
            completion_tokens: 15,
            cached_tokens: 16,
            cache_creation_tokens: 17,
            cost: 18.5,
            raw_cost_usd: Some(19.5),
            quota_debit: Some(20.5),
            effective_paid_cost_usd: Some(21.5),
            pricing_revision_id: Some("rev-22"),
            quota_multiplier: Some(23.5),
            local_adjustment_multiplier: Some(24.5),
            service_tier: Some("tier-25"),
            cost_state: "state-26",
            error_message: Some("err-27"),
            request_id: Some("req-28"),
            attempt: Some(29),
            error_source: Some("src-30"),
            error_stage: Some("stage-31"),
            duration_ms: Some(32),
            diagnostic_json: Some("{\"k\":33}"),
            requested_model: Some("req-model-34"),
            resolved_alias: Some("alias-35"),
            upstream_model: Some("up-36"),
            native_cost_value: Some(37.5),
            native_cost_unit: Some("unit-38"),
            native_cost_currency: Some("CUR-39"),
        }
    }

    fn expected_insert_values() -> [Value; 39] {
        [
            text("ts-1"),
            text("model-2"),
            text("acct-3"),
            text("name-4"),
            text("key-5"),
            text("keyname-6"),
            text("route-acct-7"),
            text("prov-8"),
            text("off-9"),
            text("cred-10"),
            text("status-11"),
            Value::Integer(12),
            text("proxy"),
            Value::Integer(14),
            Value::Integer(15),
            Value::Integer(16),
            Value::Integer(17),
            Value::Real(18.5),
            Value::Real(19.5),
            Value::Real(20.5),
            Value::Real(21.5),
            text("rev-22"),
            Value::Real(23.5),
            Value::Real(24.5),
            text("tier-25"),
            text("state-26"),
            text("err-27"),
            text("req-28"),
            Value::Integer(29),
            text("src-30"),
            text("stage-31"),
            Value::Integer(32),
            text("{\"k\":33}"),
            text("req-model-34"),
            text("alias-35"),
            text("up-36"),
            Value::Real(37.5),
            text("unit-38"),
            text("CUR-39"),
        ]
    }

    fn text(value: &str) -> Value {
        Value::Text(value.to_string())
    }

    fn select_forward_columns(conn: &Connection, id: i64) -> Vec<Value> {
        let sql = format!(
            "SELECT {} FROM forward_logs WHERE id = ?1",
            FORWARD_INSERT_COLUMNS.join(", ")
        );
        let mut stmt = conn.prepare(&sql).expect("select forward columns");
        stmt.query_row([id], |row| {
            let mut values = Vec::with_capacity(FORWARD_INSERT_COLUMNS.len());
            for index in 0..FORWARD_INSERT_COLUMNS.len() {
                values.push(row.get(index)?);
            }
            Ok(values)
        })
        .expect("forward row should exist")
    }

    fn parenthesized_lists(sql: &str) -> Vec<Vec<String>> {
        let mut lists = Vec::new();
        let mut rest = sql;
        while let Some(start) = rest.find('(') {
            let after = &rest[start + 1..];
            let end = after.find(')').expect("closing paren");
            lists.push(
                after[..end]
                    .split(',')
                    .map(|part| part.trim().to_string())
                    .filter(|part| !part.is_empty())
                    .collect(),
            );
            rest = &after[end + 1..];
        }
        lists
    }

    fn filled_gateway_row() -> GatewayLogInsertRow<'static> {
        GatewayLogInsertRow {
            level: "error",
            category: "gateway",
            message: "upstream failed",
            created_at: "created-at",
            request_id: Some("req-g"),
            attempt: Some(7),
            error_source: Some("upstream"),
            error_stage: Some("read"),
            duration_ms: Some(42),
            diagnostic_json: Some("{\"code\":500}"),
        }
    }

    fn empty_gateway_row() -> GatewayLogInsertRow<'static> {
        GatewayLogInsertRow {
            level: "info",
            category: "account",
            message: "verified managed account demo",
            created_at: "created-empty",
            request_id: None,
            attempt: None,
            error_source: None,
            error_stage: None,
            duration_ms: None,
            diagnostic_json: None,
        }
    }

    #[test]
    fn insert_forward_log_round_trips_every_column_in_binding_order() {
        let lists = parenthesized_lists(INSERT_FORWARD_LOG_SQL);
        assert_eq!(lists.len(), 2, "{lists:?}");
        assert_eq!(lists[0], FORWARD_INSERT_COLUMNS);
        let expected_placeholders: Vec<String> =
            (1..=39).map(|index| format!("?{index}")).collect();
        assert_eq!(lists[1], expected_placeholders);

        let conn = v26_conn();
        let row = sentinel_insert_row();
        let id = insert_forward_log(&conn, &row).expect("insert");
        assert_eq!(id, 1);
        assert_eq!(select_forward_columns(&conn, id), expected_insert_values());
    }

    #[test]
    fn insert_gateway_log_round_trips_all_optional_diagnostic_fields() {
        let lists = parenthesized_lists(INSERT_GATEWAY_LOG_SQL);
        assert_eq!(
            lists[0],
            [
                "level",
                "category",
                "message",
                "created_at",
                "request_id",
                "attempt",
                "error_source",
                "error_stage",
                "duration_ms",
                "diagnostic_json",
            ]
        );

        let conn = v26_conn();
        assert_eq!(insert_gateway_log(&conn, &filled_gateway_row()).unwrap(), 1);
        assert_eq!(insert_gateway_log(&conn, &empty_gateway_row()).unwrap(), 1);

        let mut stmt = conn
            .prepare(
                "SELECT level, category, message, created_at, request_id, attempt,
                        error_source, error_stage, duration_ms, diagnostic_json
                 FROM gateway_logs ORDER BY id ASC",
            )
            .unwrap();
        let rows: Vec<[Value; 10]> = stmt
            .query_map([], |row| {
                Ok([
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                    row.get(9)?,
                ])
            })
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(
            rows[0],
            [
                text("error"),
                text("gateway"),
                text("upstream failed"),
                text("created-at"),
                text("req-g"),
                Value::Integer(7),
                text("upstream"),
                text("read"),
                Value::Integer(42),
                text("{\"code\":500}"),
            ]
        );
        assert_eq!(
            rows[1],
            [
                text("info"),
                text("account"),
                text("verified managed account demo"),
                text("created-empty"),
                Value::Null,
                Value::Null,
                Value::Null,
                Value::Null,
                Value::Null,
                Value::Null,
            ]
        );
    }

    #[test]
    fn update_and_patch_missing_rows_return_zero() {
        let conn = v26_conn();
        let id = insert_forward_log(&conn, &sentinel_insert_row()).unwrap();
        let missing_id = id + 99;
        let update = ForwardLogUpdateRow {
            id: missing_id,
            status: "success",
            http_status: Some(200),
            prompt_tokens: 1,
            completion_tokens: 2,
            cached_tokens: 3,
            cache_creation_tokens: 4,
            cost: 5.5,
            raw_cost_usd: Some(5.5),
            quota_debit: Some(5.5),
            effective_paid_cost_usd: Some(5.5),
            pricing_revision_id: Some("rev"),
            quota_multiplier: Some(1.0),
            local_adjustment_multiplier: Some(1.0),
            service_tier: Some("default"),
            cost_state: "priced",
            error_message: Some("nope"),
            error_source: Some("src"),
            error_stage: Some("stage"),
            duration_ms: Some(9),
            diagnostic_json: Some("{}"),
            native_cost_value: Some(5.5),
            native_cost_unit: Some("usd"),
            native_cost_currency: Some("USD"),
        };
        assert_eq!(update_forward_log(&conn, &update).unwrap(), 0);
        assert_eq!(
            patch_forward_log_identity(
                &conn,
                &ForwardLogIdentityPatch {
                    id: missing_id,
                    requested_model: Some("other"),
                    resolved_alias: Some("alias"),
                    upstream_model: Some("up"),
                    native_cost_value: Some(1.0),
                    native_cost_unit: Some("credits"),
                    native_cost_currency: Some("CR"),
                },
            )
            .unwrap(),
            0
        );
        assert_eq!(
            select_forward_columns(&conn, id),
            expected_insert_values(),
            "missing-row update/patch must leave the existing row untouched"
        );

        let coalesced = ForwardLogUpdateRow {
            id,
            status: "streaming",
            http_status: None,
            prompt_tokens: 100,
            completion_tokens: 101,
            cached_tokens: 102,
            cache_creation_tokens: 103,
            cost: 0.0,
            raw_cost_usd: None,
            quota_debit: None,
            effective_paid_cost_usd: None,
            pricing_revision_id: None,
            quota_multiplier: None,
            local_adjustment_multiplier: None,
            service_tier: None,
            cost_state: "not_applicable",
            error_message: None,
            error_source: None,
            error_stage: None,
            duration_ms: None,
            diagnostic_json: None,
            native_cost_value: None,
            native_cost_unit: None,
            native_cost_currency: None,
        };
        assert_eq!(update_forward_log(&conn, &coalesced).unwrap(), 1);
        let after = select_forward_columns(&conn, id);
        assert_eq!(after[11], Value::Integer(12), "http_status COALESCE");
        assert_eq!(after[26], text("err-27"), "error_message COALESCE");
        assert_eq!(after[29], text("src-30"), "error_source COALESCE");
        assert_eq!(after[10], text("streaming"));
        assert_eq!(
            patch_forward_log_identity(
                &conn,
                &ForwardLogIdentityPatch {
                    id,
                    requested_model: Some("patched-model"),
                    resolved_alias: Some("patched-alias"),
                    upstream_model: Some("patched-up"),
                    native_cost_value: Some(9.25),
                    native_cost_unit: Some("usd"),
                    native_cost_currency: Some("USD"),
                },
            )
            .unwrap(),
            1
        );
        let patched = select_forward_columns(&conn, id);
        assert_eq!(patched[33], text("patched-model"));
        assert_eq!(patched[34], text("patched-alias"));
        assert_eq!(patched[35], text("patched-up"));
        assert_eq!(patched[36], Value::Real(9.25));
        assert_eq!(patched[37], text("usd"));
        assert_eq!(patched[38], text("USD"));
    }

    #[test]
    fn helpers_use_the_caller_connection_and_do_not_commit_a_private_transaction() {
        let conn = v26_conn();
        let tx = conn.unchecked_transaction().unwrap();
        let id = insert_forward_log(&tx, &sentinel_insert_row()).unwrap();
        assert_eq!(insert_gateway_log(&tx, &empty_gateway_row()).unwrap(), 1);
        tx.rollback().unwrap();
        let forward_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM forward_logs WHERE id = ?1",
                [id],
                |row| row.get(0),
            )
            .unwrap();
        let gateway_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM gateway_logs", [], |row| row.get(0))
            .unwrap();
        assert_eq!(forward_count, 0);
        assert_eq!(gateway_count, 0);
    }

    #[test]
    fn production_sources_have_no_core_domain_or_gateway_edge() {
        let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let manifest_path = crate_root.join("Cargo.toml");
        let manifest = fs::read_to_string(&manifest_path)
            .unwrap_or_else(|error| panic!("read {}: {error}", manifest_path.display()));
        for name in ["ocg-core", "ocg-domain", "ocg-gateway", "serde", "tokio"] {
            assert!(
                !manifest_has_direct_dep(&manifest, name),
                "ocg-infra must not depend on {name} directly"
            );
        }

        let src_root = crate_root.join("src");
        let mut scanned = Vec::new();
        visit_rust_files(&src_root, &mut |path| {
            scanned.push(path.to_path_buf());
            let source = fs::read_to_string(path)
                .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
            let production = production_source(&source);
            for needle in [
                "ocg_core",
                "ocg-core",
                "ocg_domain",
                "ocg-domain",
                "ocg_gateway",
                "ocg-gateway",
                "AppConfig",
                "CoreState",
                "Database",
            ] {
                assert!(
                    !production.contains(needle),
                    "{} production source must not name `{needle}`",
                    path.display()
                );
            }
        });
        for required in [
            "inference_http.rs",
            "http.rs",
            "crypto.rs",
            "lib.rs",
            "sqlite_logs.rs",
        ] {
            assert!(
                scanned.iter().any(|path| {
                    path.file_name().and_then(|name| name.to_str()) == Some(required)
                }),
                "source boundary guard must scan {required}, scanned={scanned:?}"
            );
        }

        let logs_file =
            fs::read_to_string(src_root.join("sqlite_logs.rs")).expect("sqlite_logs.rs");
        let logs_source = production_source(&logs_file);
        for needle in [
            "Connection::open",
            "open_in_memory",
            "unchecked_transaction",
            ".transaction(",
            "PRAGMA",
            "std::thread",
            "thread::spawn",
            "tokio",
            "use serde",
            "serde_json",
            "chrono",
            "anyhow",
            "ForwardMetrics",
            "ForwardLogNativeAttribution",
            "spawn(",
        ] {
            assert!(
                !logs_source.contains(needle),
                "sqlite_logs production source must not name `{needle}`"
            );
        }
        assert_eq!(
            logs_source.matches(".execute(").count(),
            4,
            "each helper must perform exactly one statement"
        );
    }

    fn manifest_has_direct_dep(manifest: &str, name: &str) -> bool {
        manifest.lines().any(|line| {
            let line = line.trim();
            line.starts_with(&format!("{name} "))
                || line.starts_with(&format!("{name}="))
                || line.starts_with(&format!("[{name}."))
                || line.starts_with(&format!("[dependencies.{name}]"))
                || line.starts_with(&format!("[dev-dependencies.{name}]"))
                || (line.starts_with("[target.") && line.contains(&format!(".{name}]")))
        })
    }

    fn production_source(source: &str) -> &str {
        source.split("#[cfg(test)]").next().unwrap_or(source)
    }

    fn visit_rust_files(dir: &Path, visit: &mut impl FnMut(&Path)) {
        let mut entries: Vec<_> = fs::read_dir(dir)
            .unwrap_or_else(|error| panic!("read {}: {error}", dir.display()))
            .map(|entry| entry.expect("dir entry").path())
            .collect();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                visit_rust_files(&path, visit);
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
                visit(&path);
            }
        }
    }
}
