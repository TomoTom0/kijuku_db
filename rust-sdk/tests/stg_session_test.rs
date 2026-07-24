//! stg 排他ロック + sync 元 revision 記録の統合テスト（設計 §15-11・TASK-55）。

#![allow(deprecated)] // テストは同期 API（create_media 等）を直接使用

use kijuku_db::diff::ObserveOptions;
use kijuku_db::{meta_path, read_stg_meta, KijukuDB, KijukuError, MediaInput, MediaType, StgLock};
use std::path::PathBuf;
use tempfile::TempDir;

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

/// sync 後 `<stg>.meta.json` に正しい revision が記録される（設計 §15-11）
#[test]
fn test_sync_writes_stg_meta_revision() {
    let dir = TempDir::new().unwrap();
    let (prod, stg) = setup_prod(&dir, 3);
    KijukuDB::replicate_db(&prod, &stg).expect("sync");

    assert!(meta_path(&stg).exists(), "meta ファイル生成");
    let meta = read_stg_meta(&stg)
        .expect("read meta")
        .expect("meta exists");
    assert_eq!(meta.synced_from.prod_path, prod.display().to_string());
    assert_eq!(meta.synced_from.revision.media_count, 3);
    assert_eq!(meta.synced_from.revision.media_max_id, 3);
    assert_eq!(meta.synced_from.revision.schema_version, 6);
}

/// sync 中（ロック保持中）の2つ目のロック取得は StgBusy（設計 §15-11）
#[test]
fn test_stg_lock_excludes_second_session() {
    let dir = TempDir::new().unwrap();
    let stg = dir.path().join("kijuku.stg.db");

    let lock1 = StgLock::acquire(&stg).expect("1回目の取得は成功");
    let err = StgLock::acquire(&stg).unwrap_err();
    assert!(
        matches!(err, KijukuError::StgBusy { .. }),
        "2回目は StgBusy: {err:?}"
    );

    drop(lock1);
    // 解放後は再取得可能
    let _lock2 = StgLock::acquire(&stg).expect("解放後の再取得は成功");
}

/// sync 実行中に別セッションが stg ロック取得 -> StgBusy
#[test]
fn test_sync_holds_lock_during_operation() {
    let dir = TempDir::new().unwrap();
    let (prod, stg) = setup_prod(&dir, 2);

    // stg を事前にロック（編集中セッションを模擬）
    let _held = StgLock::acquire(&stg).expect("hold lock");
    let err = KijukuDB::replicate_db(&prod, &stg).unwrap_err();
    assert!(
        matches!(err, KijukuError::StgBusy { .. }),
        "sync はロック保持中に StgBusy: {err:?}"
    );
}

/// observe: prod 不変更時 `prod_sync_revision` gate 合格（設計 §15-11）
#[test]
fn test_observe_prod_sync_revision_pass_when_unchanged() {
    let dir = TempDir::new().unwrap();
    let (prod, stg) = setup_prod(&dir, 2);
    KijukuDB::replicate_db(&prod, &stg).expect("sync");

    let stg_db = KijukuDB::open(&stg).expect("open stg");
    let result = stg_db
        .observe(&prod, &ObserveOptions::default())
        .expect("observe");
    let rev = result
        .checks
        .iter()
        .find(|c| c.name == "prod_sync_revision")
        .expect("prod_sync_revision check exists");
    assert!(
        rev.passed,
        "prod 不変更 -> revision 一致: {:?}",
        rev.detail
    );
}

/// observe: prod が sync 後に更新されると `prod_sync_revision` 不合格・再 sync で復帰（設計 §15-11）
#[test]
fn test_observe_prod_sync_revision_fails_on_drift_then_resync() {
    let dir = TempDir::new().unwrap();
    let (prod, stg) = setup_prod(&dir, 2);
    KijukuDB::replicate_db(&prod, &stg).expect("sync");

    // prod を sync 後に変更（drift 発生）
    {
        let prod_db = KijukuDB::open(&prod).expect("open prod");
        prod_db
            .create_media(&MediaInput {
                title: "drift".to_string(),
                media_type: MediaType::Comic,
                flag_exist: Some(true),
                ..Default::default()
            })
            .expect("create on prod");
    }

    let stg_db = KijukuDB::open(&stg).expect("open stg");
    let result = stg_db
        .observe(&prod, &ObserveOptions::default())
        .expect("observe");
    let rev = result
        .checks
        .iter()
        .find(|c| c.name == "prod_sync_revision")
        .expect("check exists");
    assert!(
        !rev.passed,
        "prod drifted -> gate 不合格: {:?}",
        rev.detail
    );

    // 再 sync で revision 更新 -> 合格に復帰
    drop(stg_db);
    KijukuDB::replicate_db(&prod, &stg).expect("re-sync");
    let stg_db = KijukuDB::open(&stg).expect("open stg");
    let result2 = stg_db
        .observe(&prod, &ObserveOptions::default())
        .expect("observe");
    let rev2 = result2
        .checks
        .iter()
        .find(|c| c.name == "prod_sync_revision")
        .expect("check exists");
    assert!(
        rev2.passed,
        "再 sync で合格に復帰: {:?}",
        rev2.detail
    );
}
