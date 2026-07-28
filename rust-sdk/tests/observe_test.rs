//! observe（機械的 promote gate）のテスト（設計 §3.4/§4.4/§15-10・TASK-54）。
//!
//! - `evaluate_gate` 純粋関数の単体テスト（DB 不要・閾値/FK/schema/golden の各検査）
//! - `KijukuDB::observe` の統合テスト（prod/stg を模擬し gate を評価）

#![allow(deprecated)] // テストは同期 API（create_media 等）を直接使用

use kijuku_db::diff::{
    evaluate_gate, BackupDiffSummary, DiffDistribution, DiffOptions, DiffTotals, GateCheck,
    GateConfig, GoldenAssertion, ObserveOptions, ProdStgDiffSummary,
};
use kijuku_db::migration::FkViolation;
use kijuku_db::{KijukuDB, MediaInput, MediaType};
use std::path::PathBuf;
use tempfile::TempDir;

// ========== evaluate_gate 純粋関数（DB 不要）==========

fn summary_with_totals(added: usize, removed: usize, changed: usize) -> ProdStgDiffSummary {
    ProdStgDiffSummary {
        counts: BackupDiffSummary::default(),
        totals: DiffTotals {
            added,
            removed,
            changed,
        },
        distribution: DiffDistribution::default(),
    }
}

fn passed(checks: &[GateCheck], name: &str) -> bool {
    checks
        .iter()
        .find(|c| c.name == name)
        .map(|c| c.passed)
        .unwrap_or(false)
}

/// 健全な入力では全検査合格（設計 §3.4）
#[test]
fn test_evaluate_gate_all_pass() {
    let summary = summary_with_totals(10, 5, 20);
    let checks = evaluate_gate(
        &summary,
        "ok",
        &[],
        true,
        6,
        6,
        &[],
        &GateConfig::default(),
    );
    assert!(checks.iter().all(|c| c.passed), "全検査合格: {:?}", checks);
}

/// FK 違反あり → foreign_key_check 不合格（設計 §3.4）
#[test]
fn test_evaluate_gate_fk_violation_fails() {
    let summary = summary_with_totals(0, 0, 0);
    let violations = vec![FkViolation {
        table: "media_tags".to_string(),
        rowid: 3,
        parent: Some("media".to_string()),
        fkid: 0,
    }];
    let checks = evaluate_gate(&summary, "ok", &violations, true, 6, 6, &[], &GateConfig::default());
    assert!(!passed(&checks, "foreign_key_check"));
    assert!(passed(&checks, "integrity_check"));
}

/// FK 無効 → foreign_key_check 不合格（接続が FK を強制しないのは危険）
#[test]
fn test_evaluate_gate_fk_disabled_fails() {
    let summary = summary_with_totals(0, 0, 0);
    let checks = evaluate_gate(&summary, "ok", &[], false, 6, 6, &[], &GateConfig::default());
    assert!(!passed(&checks, "foreign_key_check"));
}

/// integrity 異常 → integrity_check 不合格
#[test]
fn test_evaluate_gate_integrity_fail() {
    let summary = summary_with_totals(0, 0, 0);
    let checks = evaluate_gate(
        &summary,
        "error: database disk image is malformed",
        &[],
        true,
        6,
        6,
        &[],
        &GateConfig::default(),
    );
    assert!(!passed(&checks, "integrity_check"));
}

/// schema_version 不一致 → schema_version_match 不合格
#[test]
fn test_evaluate_gate_schema_mismatch_fails() {
    let summary = summary_with_totals(0, 0, 0);
    let checks = evaluate_gate(&summary, "ok", &[], true, 6, 5, &[], &GateConfig::default());
    assert!(!passed(&checks, "schema_version_match"));
}

/// 件数が閾値超過 → 該当 max_* 不合格（設計 §3.4・§15-10）
#[test]
fn test_evaluate_gate_threshold_exceeded() {
    let summary = summary_with_totals(0, 2000, 0);
    let config = GateConfig {
        max_removed: 1000,
        ..Default::default()
    };
    let checks = evaluate_gate(&summary, "ok", &[], true, 6, 6, &[], &config);
    assert!(!passed(&checks, "max_removed"), "removed 2000 > 1000");
    assert!(passed(&checks, "max_added"));
    assert!(passed(&checks, "max_changed"));
}

/// golden assertion: 件数が範囲内なら合格、範囲外なら不合格
#[test]
fn test_evaluate_gate_golden_assertion() {
    let summary = summary_with_totals(0, 0, 0);
    let config = GateConfig {
        golden_assertions: vec![GoldenAssertion {
            name: "active_media_min".to_string(),
            sql: "SELECT COUNT(*) FROM media WHERE flag_exist = 1".to_string(),
            expected_min: Some(1),
            expected_max: None,
        }],
        ..Default::default()
    };
    // 件数 5 >= 1 → 合格
    let checks_ok = evaluate_gate(
        &summary,
        "ok",
        &[],
        true,
        6,
        6,
        &[kijuku_db::diff::GoldenResult::Count(5)],
        &config,
    );
    assert!(passed(&checks_ok, "golden:active_media_min"));

    // 件数 0 < 1 → 不合格
    let checks_ng = evaluate_gate(
        &summary,
        "ok",
        &[],
        true,
        6,
        6,
        &[kijuku_db::diff::GoldenResult::Count(0)],
        &config,
    );
    assert!(!passed(&checks_ng, "golden:active_media_min"));
}

/// golden assertion の SQL エラー → 当該検査不合格（安全側）
#[test]
fn test_evaluate_gate_golden_error_fails() {
    let summary = summary_with_totals(0, 0, 0);
    let config = GateConfig {
        golden_assertions: vec![GoldenAssertion {
            name: "bad".to_string(),
            sql: "SELECT * FROM no_such_table".to_string(),
            expected_min: None,
            expected_max: Some(0),
        }],
        ..Default::default()
    };
    let checks = evaluate_gate(
        &summary,
        "ok",
        &[],
        true,
        6,
        6,
        &[kijuku_db::diff::GoldenResult::Error("no such table".to_string())],
        &config,
    );
    assert!(!passed(&checks, "golden:bad"), "SQL エラーは gate 不合格");
}

// ========== KijukuDB::observe 統合テスト（prod/stg 模擬）==========

/// prod DB を作成し n 件のメディアを投入、(prod, stg) を返す
fn setup_prod(dir: &TempDir, n: usize) -> (PathBuf, PathBuf) {
    let prod = dir.path().join("kijuku.db");
    let stg = dir.path().join("kijuku.stg.db");
    let prod_db = KijukuDB::open(&prod).expect("open prod");
    prod_db.migrate().expect("migrate prod");
    for i in 0..n {
        prod_db
            .create_media(&MediaInput {
                title: format!("m{}", i),
                media_type: MediaType::Comic,
                flag_exist: Some(true),
                ..Default::default()
            })
            .expect("create media");
    }
    (prod, stg)
}

/// sync + 小変更 → gate 全合格（promote 可能・設計 §3.4）
#[test]
fn test_observe_pass_after_small_change() {
    let dir = TempDir::new().unwrap();
    let (prod, stg) = setup_prod(&dir, 2);
    KijukuDB::replicate_db(&prod, &stg, kijuku_db::SyncOp::Sync).expect("sync");

    let stg_db = KijukuDB::open(&stg).expect("open stg");
    // 小変更: 1件追加（added=1・閾値以内）
    stg_db
        .create_media(&MediaInput {
            title: "stg 新規".to_string(),
            media_type: MediaType::Comic,
            ..Default::default()
        })
        .expect("create on stg");

    let result = stg_db
        .observe(&prod, &ObserveOptions::default())
        .expect("observe");
    assert!(result.passed, "小変更は gate 合格: {:?}", result.checks);
    assert!(passed(&result.checks, "integrity_check"));
    assert!(passed(&result.checks, "foreign_key_check"));
    assert!(passed(&result.checks, "schema_version_match"));
    assert_eq!(result.prod_schema_version, result.stg_schema_version);
    assert_eq!(result.summary.totals.added, 1, "stg 新規1件 = added");
}

/// 大量削除（閾値超過）→ max_removed 不合格で promote 不可
#[test]
fn test_observe_fail_on_bulk_delete() {
    let dir = TempDir::new().unwrap();
    let (prod, stg) = setup_prod(&dir, 5);
    KijukuDB::replicate_db(&prod, &stg, kijuku_db::SyncOp::Sync).expect("sync");

    let stg_db = KijukuDB::open(&stg).expect("open stg");
    // stg で3件削除（閾値 max_removed=1 を超過）
    for id in 1..=3 {
        stg_db.delete_media(id).expect("delete on stg");
    }

    let options = ObserveOptions {
        diff_options: DiffOptions::default(),
        gate_config: GateConfig {
            max_removed: 1,
            ..Default::default()
        },
    };
    let result = stg_db.observe(&prod, &options).expect("observe");
    assert!(!result.passed, "大量削除は gate 不合格");
    assert!(!passed(&result.checks, "max_removed"));
    assert_eq!(result.summary.totals.removed, 3);
}

/// golden assertion を gate_config で指定し評価（active media 件数）
#[test]
fn test_observe_with_golden_assertion() {
    let dir = TempDir::new().unwrap();
    let (prod, stg) = setup_prod(&dir, 3);
    KijukuDB::replicate_db(&prod, &stg, kijuku_db::SyncOp::Sync).expect("sync");

    let stg_db = KijukuDB::open(&stg).expect("open stg");
    let options = ObserveOptions {
        diff_options: DiffOptions::default(),
        gate_config: GateConfig {
            golden_assertions: vec![GoldenAssertion {
                name: "active_media".to_string(),
                sql: "SELECT COUNT(*) FROM media WHERE flag_exist = 1".to_string(),
                expected_min: Some(1),
                expected_max: None,
            }],
            ..Default::default()
        },
    };
    let result = stg_db.observe(&prod, &options).expect("observe");
    assert!(result.passed, "3件 active で golden 合格: {:?}", result.checks);
    let golden = result
        .checks
        .iter()
        .find(|c| c.name == "golden:active_media")
        .expect("golden check 存在");
    assert!(golden.passed);
    assert!(golden.detail.contains("count=3"), "detail に件数: {}", golden.detail);
}

/// observe は prod を readonly で開くため prod を変更しない（設計 §5.1）
#[test]
fn test_observe_does_not_modify_prod() {
    let dir = TempDir::new().unwrap();
    let (prod, stg) = setup_prod(&dir, 2);
    KijukuDB::replicate_db(&prod, &stg, kijuku_db::SyncOp::Sync).expect("sync");
    let prod_bytes_before = std::fs::read(&prod).expect("read prod before");

    let stg_db = KijukuDB::open(&stg).expect("open stg");
    let _ = stg_db.observe(&prod, &ObserveOptions::default()).expect("observe");

    let prod_bytes_after = std::fs::read(&prod).expect("read prod after");
    assert_eq!(
        prod_bytes_before, prod_bytes_after,
        "prod は readonly open で変更されない"
    );
}
