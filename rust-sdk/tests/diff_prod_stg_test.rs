//! diff（prod RO + stg 比較）のテスト（設計 §4.4/§14.3・TASK-53）。
//!
//! `KijukuDB::diff_with_prod` が prod（stg 編集前）と stg（LLM 編集後）の差分を
//! **stg 編集視点**（added=stg新規=promoteでprod追加 等）で返すことを検証する。
//! セマンティクス反転（`diff_with_backup` と逆）の担保が目的。

use kijuku_db::diff::{DiffDetail, DiffOptions};
use kijuku_db::{KijukuDB, MediaInput, MediaUpdateInput, MediaType};
use tempfile::TempDir;

use std::path::PathBuf;

/// 末尾に2件のメディアを持つ prod DB を作成し、(prod, stg, m1_id, m2_id) を返す。
fn setup_prod(dir: &TempDir) -> (PathBuf, PathBuf, i64, i64) {
    let prod = dir.path().join("kijuku.db");
    let stg = dir.path().join("kijuku.stg.db");
    let prod_db = KijukuDB::open(&prod).expect("open prod");
    prod_db.migrate().expect("migrate prod");
    let m1 = prod_db
        .create_media(&MediaInput {
            title: "prod メディア1".to_string(),
            media_type: MediaType::Comic,
            ..Default::default()
        })
        .expect("create media 1");
    let m2 = prod_db
        .create_media(&MediaInput {
            title: "prod メディア2".to_string(),
            media_type: MediaType::Comic,
            ..Default::default()
        })
        .expect("create media 2");
    (prod, stg, m1.id, m2.id)
}

/// diff_with_prod が stg 編集視点で差分を返す（セマンティクス反転の担保・設計 §4.4）。
///
/// stg で「m1 変更 / m2 削除 / m3 新規」を行い、結果が以下になることを検証:
/// - `added`:   m3（stg 新規 = promote で prod に追加）
/// - `removed`: m2（prod のみ = promote で prod から削除）
/// - `changed`: m1（両方で異なる = promote で上書き）
#[test]
fn test_diff_with_prod_semantics_inverted() {
    let dir = TempDir::new().unwrap();
    let (prod, stg, m1_id, m2_id) = setup_prod(&dir);

    // prod → stg sync
    KijukuDB::replicate_db(&prod, &stg, kijuku_db::SyncOp::Sync).expect("sync");

    // stg で編集: m1 変更 / m2 削除 / m3 新規
    let stg_db = KijukuDB::open(&stg).expect("open stg");
    stg_db
        .update_media(
            m1_id,
            &MediaUpdateInput {
                title: Some("prod メディア1-edited".to_string()),
                ..Default::default()
            },
        )
        .expect("update m1");
    stg_db.delete_media(m2_id).expect("delete m2");
    stg_db
        .create_media(&MediaInput {
            title: "stg 新規メディア".to_string(),
            media_type: MediaType::Comic,
            ..Default::default()
        })
        .expect("create m3");

    let options = DiffOptions {
        detail: Some(DiffDetail::Full),
    };
    let diff = stg_db.diff_with_prod(&prod, &options).expect("diff_with_prod");

    // セマンティクス（stg 編集視点）
    assert_eq!(diff.summary.media.added, 1, "stg 新規 = added");
    assert_eq!(diff.summary.media.removed, 1, "prod 削除 = removed");
    assert_eq!(diff.summary.media.changed, 1, "stg 変更 = changed");

    // added の中身（m3: stg 新規）
    let added = diff.media.added.first().expect("added に m3");
    assert_eq!(added.title, "stg 新規メディア");

    // removed の中身（m2: prod のみ）
    let removed = diff.media.removed.first().expect("removed に m2");
    assert_eq!(removed.id, m2_id);

    // changed の中身（m1: current=prod 側・backup=stg 側）
    let changed = diff.media.changed.first().expect("changed に m1");
    assert_eq!(changed.current.id, m1_id, "current は prod 側");
    assert_eq!(changed.backup.id, m1_id, "backup は stg 側");
    assert_ne!(changed.current.title, changed.backup.title);
}

/// sync 直後（stg 未編集）は差分ゼロ（設計 §4.4・§6.3: stg は sync 時点の prod + LLM 編集）。
#[test]
fn test_diff_with_prod_no_changes_after_sync() {
    let dir = TempDir::new().unwrap();
    let (prod, stg, _m1_id, _m2_id) = setup_prod(&dir);

    KijukuDB::replicate_db(&prod, &stg, kijuku_db::SyncOp::Sync).expect("sync");

    let stg_db = KijukuDB::open(&stg).expect("open stg");
    let diff = stg_db
        .diff_with_prod(&prod, &DiffOptions::default())
        .expect("diff_with_prod");

    assert_eq!(diff.summary.media.added, 0);
    assert_eq!(diff.summary.media.removed, 0);
    assert_eq!(diff.summary.media.changed, 0);
}

/// diff_with_prod は prod を readonly で開くため prod ファイルを変更しない（設計 §5.1）。
/// readonly open は WAL/journal を書かないので prod のバイト内容が不変であることで検証。
#[test]
fn test_diff_with_prod_does_not_modify_prod() {
    let dir = TempDir::new().unwrap();
    let (prod, stg, _m1_id, _m2_id) = setup_prod(&dir);
    KijukuDB::replicate_db(&prod, &stg, kijuku_db::SyncOp::Sync).expect("sync");

    let prod_bytes_before = std::fs::read(&prod).expect("read prod before");

    let stg_db = KijukuDB::open(&stg).expect("open stg");
    stg_db
        .create_media(&MediaInput {
            title: "stg 追加".to_string(),
            media_type: MediaType::Comic,
            ..Default::default()
        })
        .expect("create on stg");
    let _ = stg_db
        .diff_with_prod(&prod, &DiffOptions::default())
        .expect("diff_with_prod");

    let prod_bytes_after = std::fs::read(&prod).expect("read prod after");
    assert_eq!(
        prod_bytes_before, prod_bytes_after,
        "prod ファイルは readonly open で変更されない"
    );
}
