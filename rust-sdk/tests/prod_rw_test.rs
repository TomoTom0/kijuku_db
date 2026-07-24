//! prod RW 一時取得基盤（ProdRwScope）のテスト（設計 §5.2/§7.2/§7.3・TASK-56 C1）。
//!
//! `ProdRwScope` が prod の排他ロック取得と pre-stash 強制（enabled と独立して常時実行）を
//! 完了し、RAII で解放すること。promote(copy_db_online)・(b)操作(RW KijukuDB) の両書込経路で
//! prod を開けること。pre-stash パスが §8 即時復旧の戻し先として使えること。

#![allow(deprecated)] // テストは同期 API（create_media 等）を直接使用

use kijuku_db::{
    BackupManager, BackupOptions, KijukuDB, KijukuError, MediaInput, MediaType, ProdRwScope, StgLock,
};
use std::path::PathBuf;
use tempfile::TempDir;

/// prod DB を作成し n 件のメディアを投入、(prod, stg) を返す（stg_session_test.rs と同等）
fn setup_prod(dir: &TempDir, n: usize) -> (PathBuf, PathBuf) {
    let prod = dir.path().join("kijuku.db");
    let stg = dir.path().join("kijuku.stg.db");
    let prod_db = KijukuDB::open(&prod).expect("open prod");
    prod_db.migrate().expect("migrate prod");
    for i in 0..n {
        prod_db
            .create_media(&MediaInput {
                title: format!("prod{}", i),
                media_type: MediaType::Comic,
                flag_exist: Some(true),
                ..Default::default()
            })
            .expect("create prod media");
    }
    (prod, stg)
}

/// stg DB を新規作成し n 件のメディアを投入、パスを返す
fn setup_stg(dir: &TempDir, n: usize) -> PathBuf {
    let stg = dir.path().join("kijuku.stg.db");
    let stg_db = KijukuDB::open(&stg).expect("open stg");
    stg_db.migrate().expect("migrate stg");
    for i in 0..n {
        stg_db
            .create_media(&MediaInput {
                title: format!("stg{}", i),
                media_type: MediaType::Comic,
                flag_exist: Some(true),
                ..Default::default()
            })
            .expect("create stg media");
    }
    stg
}

/// 同一 prod の2つ目の acquire が ProdBusy で失敗する（設計 §5.3 排他）
#[test]
fn test_prod_rw_scope_acquires_exclusive_lock() {
    let dir = TempDir::new().unwrap();
    let (prod, _stg) = setup_prod(&dir, 1);

    let _scope = ProdRwScope::acquire(&prod, None).expect("1st acquire ok");
    let err = ProdRwScope::acquire(&prod, None).unwrap_err();
    assert!(
        matches!(err, KijukuError::ProdBusy { .. }),
        "2nd acquire should be ProdBusy: {err:?}"
    );
}

/// acquire が backup/tmp/ 配下に *-pre_promote.db を生成し、pre_stash_path() と一致する（§7.2）
#[test]
fn test_prod_rw_scope_creates_pre_stash_in_tmp() {
    let dir = TempDir::new().unwrap();
    let (prod, _stg) = setup_prod(&dir, 2);

    let scope = ProdRwScope::acquire(&prod, None).expect("acquire");
    let pre = scope.pre_stash_path().expect("pre_stash_path is Some");
    let pre_str = pre.to_string_lossy();
    assert!(pre.exists(), "pre-stash file exists");
    assert!(
        pre_str.ends_with("-pre_promote.db"),
        "pre-stash filename: {pre_str}"
    );
    assert!(
        pre_str.contains("backup"),
        "pre-stash is under backup dir: {pre_str}"
    );
}

/// enabled=false を渡しても pre-stash が作成される（§7.2 の核心・enabled と独立して常時強制）
#[test]
fn test_prod_rw_scope_pre_stash_regardless_of_enabled_false() {
    let dir = TempDir::new().unwrap();
    let (prod, _stg) = setup_prod(&dir, 1);

    let scope = ProdRwScope::acquire(
        &prod,
        Some(BackupOptions {
            enabled: Some(false),
            ..Default::default()
        }),
    )
    .expect("acquire with enabled=false");
    let pre = scope.pre_stash_path().expect("pre-stash created even if enabled=false");
    assert!(pre.exists(), "pre-stash file exists");
}

/// Drop で排他ロックが解放され、再 acquire 可能になる（RAII）
#[test]
fn test_prod_rw_scope_drop_releases_lock_for_reacquire() {
    let dir = TempDir::new().unwrap();
    let (prod, _stg) = setup_prod(&dir, 1);

    {
        let _scope = ProdRwScope::acquire(&prod, None).expect("1st acquire");
    } // Drop で解放
    let _scope2 = ProdRwScope::acquire(&prod, None).expect("re-acquire after drop");
}

/// pre-stash パスが prod の退避時点の内容と一致する（§8 即時復旧の前提）
#[test]
fn test_prod_rw_scope_pre_stash_path_exposed_for_recovery() {
    let dir = TempDir::new().unwrap();
    let (prod, _stg) = setup_prod(&dir, 3);

    let scope = ProdRwScope::acquire(&prod, None).expect("acquire");
    let pre = scope.pre_stash_path().expect("pre_stash_path");

    // pre-stash は prod の退避時点（3件）と一致
    let pre_db = KijukuDB::open(pre).expect("open pre-stash");
    assert!(pre_db.get_media(1).is_some(), "media id=1 exists in pre-stash");
    assert!(pre_db.get_media(2).is_some(), "media id=2 exists in pre-stash");
    assert!(pre_db.get_media(3).is_some(), "media id=3 exists in pre-stash");
    assert!(pre_db.get_media(4).is_none(), "media id=4 absent in pre-stash");
}

/// scope 保持中に copy_db_online(stg, prod) で prod が上書きされる（C2 promote 書込経路の模擬）
#[test]
fn test_prod_rw_scope_allows_promote_via_copy_db_online() {
    let dir = TempDir::new().unwrap();
    let (prod, _stg) = setup_prod(&dir, 1); // prod は1件
    let stg = setup_stg(&dir, 2); // stg は2件

    let scope = ProdRwScope::acquire(&prod, None).expect("acquire");
    BackupManager::copy_db_online(&stg, scope.prod_path()).expect("copy_db_online stg->prod");

    // prod は stg の内容（2件）で上書きされる
    let prod_db = KijukuDB::open(&prod).expect("open prod after copy");
    assert_eq!(prod_db.get_media(1).unwrap().title, "stg0");
    assert_eq!(prod_db.get_media(2).unwrap().title, "stg1");
    assert!(prod_db.get_media(3).is_none(), "prod overwritten with stg content (2 rows)");
}

/// scope 保持中に RW KijukuDB で prod を開き書込できる（C4 (b)操作 書込経路の模擬）
#[test]
fn test_prod_rw_scope_allows_opening_prod_rw_kijulkudb() {
    let dir = TempDir::new().unwrap();
    let (prod, _stg) = setup_prod(&dir, 1);

    let scope = ProdRwScope::acquire(&prod, None).expect("acquire");
    let prod_db = KijukuDB::open(scope.prod_path()).expect("open prod RW under scope");
    prod_db
        .create_media(&MediaInput {
            title: "added".to_string(),
            media_type: MediaType::Comic,
            flag_exist: Some(true),
            ..Default::default()
        })
        .expect("create media under scope");
    assert_eq!(prod_db.get_media(2).unwrap().title, "added");
    drop(prod_db);
    drop(scope);

    // prod に永続化されている
    let prod_db2 = KijukuDB::open(&prod).expect("reopen prod");
    assert!(prod_db2.get_media(2).is_some(), "media persisted in prod");
}

/// StgLock が <prod>.lock を保持中は ProdRwScope::acquire が ProdBusy（lock_path 共有の検証）
#[test]
fn test_prod_rw_scope_lock_rejected_when_stglock_holds_prod_path() {
    let dir = TempDir::new().unwrap();
    let (prod, _stg) = setup_prod(&dir, 1);

    let _stg_lock = StgLock::acquire(&prod).expect("acquire stg lock on prod path");
    let err = ProdRwScope::acquire(&prod, None).unwrap_err();
    assert!(
        matches!(err, KijukuError::ProdBusy { .. }),
        "ProdRwScope should be ProdBusy when StgLock holds <prod>.lock: {err:?}"
    );
}
