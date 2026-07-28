//! promote コア（stg→prod 反映）のテスト（設計 §4.5/§6.4/§8・TASK-57）。
//!
//! - gate 合格で stg→prod が反映され、pre-stash（promote 前 prod）が作られること
//! - gate 不合格では prod が一切触られないこと（`PromoteGateFailed`）
//! - stg==prod の同一パスは誤設定として拒否されること

#![allow(deprecated)] // テストは同期 API（create_media / find_media 等）を直接使用

use kijuku_db::diff::{GateConfig, ObserveOptions};
use kijuku_db::{BackupOptions, DBOptions, KijukuDB, KijukuError, MediaFilter, MediaInput, MediaType};
use std::path::Path;
use tempfile::TempDir;

/// `title` の Comic メディア入力を生成。
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

/// DB ファイルのメディア件数。
fn media_count(db_path: &Path) -> usize {
    let db = KijukuDB::open(db_path).expect("open for count");
    db.find_media(&MediaFilter::default(), None).expect("find_media").len()
}

/// gate 合格: stg→prod 反映 + pre-stash（promote 前 prod）作成。
#[test]
fn promote_gate_pass_reflects_stg_and_pre_stashes_prod() {
    let dir = TempDir::new().unwrap();
    let prod = setup_prod(&dir, 2);
    let stg = dir.path().join("kijuku.stg.db");
    KijukuDB::replicate_db(&prod, &stg, kijuku_db::SyncOp::Sync).expect("replicate prod->stg");

    let stg_db = KijukuDB::open(&stg).expect("open stg");
    stg_db.create_media(&make_input("new")).expect("add stg media");

    assert_eq!(media_count(&prod), 2, "prod before promote");
    let outcome = stg_db
        .promote(&prod, &ObserveOptions::default(), None)
        .expect("promote");
    assert!(outcome.observe.passed, "gate should pass: {:?}", outcome.observe.checks);

    // prod に stg が反映（3件・"new" 含む）。
    assert_eq!(media_count(&prod), 3, "prod after promote");
    let titles: Vec<String> = KijukuDB::open(&prod)
        .unwrap()
        .find_media(&MediaFilter::default(), None)
        .unwrap()
        .into_iter()
        .map(|m| m.title)
        .collect();
    assert!(titles.iter().any(|t| t == "new"), "new reflected: {:?}", titles);

    // pre-stash は promote 前 prod（2件）= §8 即時復旧の戻し先。
    let pre = outcome.pre_stash_path.expect("pre-stash path");
    assert!(pre.exists(), "pre-stash file exists: {}", pre.display());
    assert_eq!(media_count(&pre), 2, "pre-stash = pre-promote prod");
}

/// gate 不合格: prod は一切触られない（max_added=0 で追加1件を拒否）。
#[test]
fn promote_gate_fail_leaves_prod_untouched() {
    let dir = TempDir::new().unwrap();
    let prod = setup_prod(&dir, 1);
    let stg = dir.path().join("kijuku.stg.db");
    KijukuDB::replicate_db(&prod, &stg, kijuku_db::SyncOp::Sync).expect("replicate prod->stg");
    let stg_db = KijukuDB::open(&stg).expect("open stg");
    stg_db.create_media(&make_input("new")).expect("add stg media");

    let opts = ObserveOptions {
        gate_config: GateConfig {
            max_added: 0,
            max_removed: 0,
            max_changed: 0,
            golden_assertions: Vec::new(),
        },
        ..Default::default()
    };
    let err = stg_db.promote(&prod, &opts, None).unwrap_err();
    assert!(
        matches!(err, KijukuError::PromoteGateFailed { .. }),
        "expected PromoteGateFailed, got: {:?}",
        err
    );
    assert_eq!(media_count(&prod), 1, "prod untouched on gate fail");
}

/// stg==prod の同一パスは誤設定として拒否。
#[test]
fn promote_same_path_rejected() {
    let dir = TempDir::new().unwrap();
    let stg = dir.path().join("kijuku.stg.db");
    let stg_db = KijukuDB::open(&stg).expect("open stg");
    stg_db.migrate().expect("migrate stg");

    let err = stg_db.promote(&stg, &ObserveOptions::default(), None).unwrap_err();
    assert!(
        matches!(err, KijukuError::Other(_)) && err.to_string().contains("same path"),
        "expected same-path rejection, got: {:?}",
        err
    );
}

/// backup_opts で pre-stash 先（backup_dir）をカスタマイズ（設計 §4.5/§7.2・TASK-62）。
/// 指定した backup_dir 配下の tmp/ に pre-stash が作られること。省略時（None）は
/// デフォルトの `{prod 親}/backup/tmp/` に作られることとの対比。
#[test]
fn promote_with_backup_opts_routes_pre_stash_to_custom_dir() {
    let dir = TempDir::new().unwrap();
    let prod = setup_prod(&dir, 2);
    let stg = dir.path().join("kijuku.stg.db");
    KijukuDB::replicate_db(&prod, &stg, kijuku_db::SyncOp::Sync).expect("replicate prod->stg");

    let stg_db = KijukuDB::open(&stg).expect("open stg");
    stg_db.create_media(&make_input("new")).expect("add stg media");

    // pre-stash 先を明示カスタマイズ（enabled:false で auto/manual/meta dir を作らず tmp/ のみ）。
    let custom_backup_dir = dir.path().join("custom-backup");
    let backup_opts = BackupOptions {
        backup_dir: Some(custom_backup_dir.to_string_lossy().to_string()),
        enabled: Some(false),
        ..Default::default()
    };
    let outcome = stg_db
        .promote(&prod, &ObserveOptions::default(), Some(backup_opts))
        .expect("promote");
    assert!(outcome.observe.passed, "gate should pass: {:?}", outcome.observe.checks);

    // prod に stg が反映され、pre-stash は指定 backup_dir 配下に作られる。
    assert_eq!(media_count(&prod), 3, "prod after promote");
    let pre = outcome.pre_stash_path.expect("pre-stash path");
    assert!(
        pre.starts_with(&custom_backup_dir),
        "pre-stash under custom backup_dir: {}",
        pre.display()
    );
    assert!(pre.exists(), "pre-stash file exists: {}", pre.display());
    assert_eq!(media_count(&pre), 2, "pre-stash = pre-promote prod");
}

/// promote 後、list_pre_stashes() で pre-stash を発見できる（設計 §8・preStashPath 喪失時の復旧経路）。
/// 戻り値の path は outcome.pre_stash_path と一致し、そのまま byPath restore に渡せる。
#[test]
fn list_pre_stashes_discovers_promote_pre_stash() {
    let dir = TempDir::new().unwrap();
    let prod = setup_prod(&dir, 2);
    let stg = dir.path().join("kijuku.stg.db");
    KijukuDB::replicate_db(&prod, &stg, kijuku_db::SyncOp::Sync).expect("replicate prod->stg");
    let stg_db = KijukuDB::open(&stg).expect("open stg");
    stg_db.create_media(&make_input("new")).expect("add stg media");

    let custom_backup_dir = dir.path().join("custom-backup");
    let backup_opts = BackupOptions {
        backup_dir: Some(custom_backup_dir.to_string_lossy().to_string()),
        enabled: Some(false),
        ..Default::default()
    };
    let outcome = stg_db
        .promote(&prod, &ObserveOptions::default(), Some(backup_opts))
        .expect("promote");
    let pre = outcome.pre_stash_path.expect("pre-stash path");

    // prod を同じ backup_dir で開き、pre-stash 一覧から発見できる（preStashPath がなくても復旧可能）。
    let prod_db = KijukuDB::open_with_options(
        &prod,
        DBOptions {
            backup: Some(BackupOptions {
                backup_dir: Some(custom_backup_dir.to_string_lossy().to_string()),
                ..Default::default()
            }),
            ..Default::default()
        },
    )
    .expect("open prod with backup dir");

    let stashes = prod_db.list_pre_stashes().expect("list_pre_stashes");
    assert_eq!(stashes.len(), 1, "pre-stash 1件: {stashes:?}");
    // 発見した path は promote が作った pre-stash と一致（byPath restore に直接渡せる）。
    assert_eq!(
        stashes[0].path.canonicalize().unwrap(),
        pre.canonicalize().unwrap(),
        "discovered path == outcome.pre_stash_path"
    );
}
