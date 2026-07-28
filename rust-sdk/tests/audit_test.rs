//! 監査ログ（prod 保護操作の事後追跡・設計 §10・TASK-46）のテスト。
//!
//! - appender の append-only JSONL 性（ヘッダなし・1行1JSON・不正行スキップ）
//! - sync/discard/observe/promote で監査レコードが prod の `backup/meta/audit.log` に追記
//! - **promote 後も prod 側 audit.log が残る**（§15-2・破壊的上書きで消えない・核心）
//! - observe 内部呼出（promote 内）の監査重複回避
//! - `list_audit_logs` のフィルタ・未存在は空配列

#![allow(deprecated)] // テストは同期 API（create_media / find_media 等）を直接使用

use kijuku_db::audit::AuditRecord;
use kijuku_db::backup::BackupManager;
use kijuku_db::diff::ObserveOptions;
use kijuku_db::{
    AuditLogFilter, AuditResult, AuditTarget, BackupOptions, KijukuDB, MediaInput, MediaType, SyncOp,
};
use std::path::Path;
use tempfile::TempDir;

fn make_input(title: &str) -> MediaInput {
    MediaInput {
        title: title.to_string(),
        media_type: MediaType::Comic,
        flag_exist: Some(true),
        ..Default::default()
    }
}

/// prod を作成（migrate + n件投入）しパスを返す。ハンドルはスコープ抜けで閉じる。
fn setup_prod(dir: &TempDir, n: usize) -> std::path::PathBuf {
    let prod = dir.path().join("kijuku.db");
    let db = KijukuDB::open(&prod).expect("open prod");
    db.migrate().expect("migrate prod");
    for i in 0..n {
        db.create_media(&make_input(&format!("prod{}", i)))
            .expect("create prod media");
    }
    prod
}

/// prod の audit.log を全件読む（新しい順）。
fn read_audit(prod: &Path) -> Vec<AuditRecord> {
    let db = KijukuDB::open(prod).expect("open prod for audit read");
    db.list_audit_logs(&AuditLogFilter::default())
        .expect("read audit logs")
}

/// appender の append-only JSONL 性（ヘッダなし・1行1JSON・新しい順）。
#[test]
fn append_audit_record_is_jsonl_append_only() {
    let dir = TempDir::new().unwrap();
    let prod = dir.path().join("kijuku.db");
    let bm = BackupManager::new(
        &prod,
        BackupOptions {
            enabled: Some(false),
            ..Default::default()
        },
    )
    .expect("bm");
    for i in 0..3 {
        bm.append_audit_record(&AuditRecord {
            timestamp: format!("2026-07-27T00:00:0{}.000Z", i),
            operation: "sync".to_string(),
            target: AuditTarget::Stg,
            actor: None,
            prod_db_path: prod.display().to_string(),
            result: AuditResult::Success,
            error: None,
            summary: serde_json::json!({"i": i}),
        })
        .expect("append");
    }
    let logs = bm.read_audit_logs(&AuditLogFilter::default()).expect("read");
    assert_eq!(logs.len(), 3, "3 records");
    // 新しい順（timestamp 降順）
    assert_eq!(logs[0].timestamp, "2026-07-27T00:00:02.000Z");
    assert_eq!(logs[2].timestamp, "2026-07-27T00:00:00.000Z");
}

/// `enabled:false` BackupManager でも `meta/audit.log` が作られて書ける。
#[test]
fn append_audit_with_disabled_backup_manager() {
    let dir = TempDir::new().unwrap();
    let prod = dir.path().join("kijuku.db");
    let bm = BackupManager::new(
        &prod,
        BackupOptions {
            enabled: Some(false),
            ..Default::default()
        },
    )
    .expect("bm");
    bm.append_audit_record(&AuditRecord {
        timestamp: "2026-07-27T00:00:00.000Z".to_string(),
        operation: "sync".to_string(),
        target: AuditTarget::Stg,
        actor: None,
        prod_db_path: prod.display().to_string(),
        result: AuditResult::Success,
        error: None,
        summary: serde_json::json!({}),
    })
    .expect("append");
    let audit_path = dir.path().join("backup").join("meta").join("audit.log");
    assert!(audit_path.exists(), "audit.log exists");
}

/// 不正行スキップ（前方互換・将来スキーマ拡張時の耐性）。
#[test]
fn read_audit_skips_invalid_lines() {
    let dir = TempDir::new().unwrap();
    let prod = dir.path().join("kijuku.db");
    let audit_path = dir.path().join("backup").join("meta").join("audit.log");
    std::fs::create_dir_all(audit_path.parent().unwrap()).unwrap();
    // 有効行・不正行・有効行
    std::fs::write(
        &audit_path,
        concat!(
            r#"{"timestamp":"2026-07-27T00:00:00.000Z","operation":"sync","target":"stg","actor":null,"prodDbPath":"/p","result":"success","error":null,"summary":{}}"#,
            "\n",
            "this is not json\n",
            r#"{"timestamp":"2026-07-27T00:00:01.000Z","operation":"observe","target":"stg","actor":null,"prodDbPath":"/p","result":"success","error":null,"summary":{}}"#,
            "\n",
        ),
    )
    .unwrap();
    let bm = BackupManager::new(
        &prod,
        BackupOptions {
            enabled: Some(false),
            ..Default::default()
        },
    )
    .expect("bm");
    let logs = bm.read_audit_logs(&AuditLogFilter::default()).expect("read");
    assert_eq!(logs.len(), 2, "invalid line skipped");
}

/// sync で prod 側 audit.log に operation=sync のレコード（target/summary 含む）。
#[test]
fn sync_appends_audit_record() {
    let dir = TempDir::new().unwrap();
    let prod = setup_prod(&dir, 2);
    let stg = dir.path().join("kijuku.stg.db");
    KijukuDB::replicate_db(&prod, &stg, SyncOp::Sync).expect("sync");

    let logs = read_audit(&prod);
    let sync_log = logs
        .iter()
        .find(|r| r.operation == "sync")
        .expect("sync audit record");
    assert_eq!(sync_log.target, AuditTarget::Stg);
    assert_eq!(sync_log.result, AuditResult::Success);
    assert_eq!(sync_log.prod_db_path, prod.display().to_string());
    assert!(sync_log.summary.get("stgPath").is_some(), "stgPath in summary");
    assert!(sync_log.summary.get("revision").is_some(), "revision in summary");
}

/// discard で operation="discard" のレコード。
#[test]
fn discard_appends_audit_record() {
    let dir = TempDir::new().unwrap();
    let prod = setup_prod(&dir, 1);
    let stg = dir.path().join("kijuku.stg.db");
    KijukuDB::replicate_db(&prod, &stg, SyncOp::Sync).expect("initial sync");
    KijukuDB::replicate_db(&prod, &stg, SyncOp::Discard).expect("discard");

    let logs = read_audit(&prod);
    let discard_log = logs
        .iter()
        .find(|r| r.operation == "discard")
        .expect("discard audit record");
    assert_eq!(discard_log.result, AuditResult::Success);
}

/// observe で監査レコード（passed・diffTotals・failedChecks）。
#[test]
fn observe_appends_audit_record() {
    let dir = TempDir::new().unwrap();
    let prod = setup_prod(&dir, 1);
    let stg = dir.path().join("kijuku.stg.db");
    KijukuDB::replicate_db(&prod, &stg, SyncOp::Sync).expect("sync");
    let stg_db = KijukuDB::open(&stg).expect("open stg");
    let result = stg_db
        .observe(&prod, &ObserveOptions::default())
        .expect("observe");

    let logs = read_audit(&prod);
    let observe_log = logs
        .iter()
        .find(|r| r.operation == "observe")
        .expect("observe audit record");
    assert_eq!(
        observe_log
            .summary
            .get("passed")
            .and_then(|v| v.as_bool()),
        Some(result.passed)
    );
    assert!(
        observe_log.summary.get("diffTotals").is_some(),
        "diffTotals in summary"
    );
}

/// **核心**: promote 後も prod 側 audit.log が残る（§15-2・promote 上書きで消えない）。
#[test]
fn promote_survives_in_prod_audit_log() {
    let dir = TempDir::new().unwrap();
    let prod = setup_prod(&dir, 2);
    let stg = dir.path().join("kijuku.stg.db");
    KijukuDB::replicate_db(&prod, &stg, SyncOp::Sync).expect("sync");
    let stg_db = KijukuDB::open(&stg).expect("open stg");
    stg_db.create_media(&make_input("new")).expect("add stg media");
    stg_db
        .promote(&prod, &ObserveOptions::default(), None)
        .expect("promote");

    let logs = read_audit(&prod);
    let promote_log = logs
        .iter()
        .find(|r| r.operation == "promote" && r.result == AuditResult::Success);
    assert!(
        promote_log.is_some(),
        "promote success audit record: {:?}",
        logs
    );
    let promote_log = promote_log.unwrap();
    assert_eq!(promote_log.target, AuditTarget::Prod);
    assert!(
        promote_log.summary.get("observe").is_some(),
        "observe in summary"
    );
    assert!(
        promote_log.summary.get("preStashPath").is_some(),
        "preStashPath in summary"
    );
}

/// observe 内部呼出（promote 内）の監査重複回避: promote 後に observe レコードなし。
#[test]
fn promote_does_not_emit_observe_audit() {
    let dir = TempDir::new().unwrap();
    let prod = setup_prod(&dir, 1);
    let stg = dir.path().join("kijuku.stg.db");
    KijukuDB::replicate_db(&prod, &stg, SyncOp::Sync).expect("sync");
    let stg_db = KijukuDB::open(&stg).expect("open stg");
    stg_db.create_media(&make_input("new")).expect("add stg media");
    stg_db
        .promote(&prod, &ObserveOptions::default(), None)
        .expect("promote");

    let logs = read_audit(&prod);
    // sync（replicate）+ promote のみ。observe は promote 内部の再評価で記録されない。
    let observe_logs: Vec<_> = logs.iter().filter(|r| r.operation == "observe").collect();
    assert!(
        observe_logs.is_empty(),
        "no observe audit from promote internal: {:?}",
        logs
    );
    let promote_logs: Vec<_> = logs.iter().filter(|r| r.operation == "promote").collect();
    assert_eq!(promote_logs.len(), 1, "exactly one promote record");
}

/// `list_audit_logs` の operation フィルタ・未存在は空配列。
#[test]
fn list_audit_logs_filter_and_empty() {
    let dir = TempDir::new().unwrap();
    let prod = setup_prod(&dir, 1);
    let stg = dir.path().join("kijuku.stg.db");

    // sync 前: audit.log 未存在 -> 空配列
    let db = KijukuDB::open(&prod).expect("open prod");
    assert!(
        db.list_audit_logs(&AuditLogFilter::default())
            .unwrap()
            .is_empty(),
        "empty before sync"
    );

    KijukuDB::replicate_db(&prod, &stg, SyncOp::Sync).expect("sync");
    let logs = db
        .list_audit_logs(&AuditLogFilter {
            operation: Some("sync".to_string()),
            ..Default::default()
        })
        .unwrap();
    assert!(
        logs.iter().all(|r| r.operation == "sync"),
        "filtered by operation"
    );
}
