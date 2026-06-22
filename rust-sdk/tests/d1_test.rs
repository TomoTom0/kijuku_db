//! D1 バックエンドの実インスタンス統合テスト（feature `d1-integration`）。
//!
//! 実行には wrangler login 済み環境で以下が必要:
//!   D1_ACCOUNT_ID, D1_DATABASE_ID
//! 実行例:
//!   D1_ACCOUNT_ID=... D1_DATABASE_ID=... \
//!     cargo test --features d1-integration --test d1_test

#![cfg(feature = "d1-integration")]

use kijuku_db::{
    AttributeValueType, D1Config, D1KijukuDB, KijukuBackend, MediaFilter, MediaHashInput,
    MediaInput, MediaType, TransferOptions, transfer,
};
use std::env;
use uuid::Uuid;

fn connect() -> D1KijukuDB {
    let account_id = env::var("D1_ACCOUNT_ID").expect("D1_ACCOUNT_ID required");
    let database_id = env::var("D1_DATABASE_ID").expect("D1_DATABASE_ID required");
    D1KijukuDB::from_wrangler(D1Config {
        account_id,
        database_id,
    })
    .expect("D1 connect (wrangler login 済みか)")
}

/// 接続＋マイグレーション済みの D1 バックエンドを返す（テスト並列実行でも安全）
async fn setup() -> D1KijukuDB {
    let db = connect();
    db.migrate().await.expect("migrate");
    db
}

/// dest（テスト用 D1）のデータテーブルを全て空にする。
///
/// `transfer` は dest が空であることを前提とし、`verify` が件数・内容の完全一致を
/// 求めるため、テスト間・再実行の冪等性に必須。FK を持つテーブルから順に削除し、
/// `tags` 親行（media の CASCADE では消えない）も明示的に消す。
async fn wipe(db: &D1KijukuDB) {
    db.execute_raw("DELETE FROM media_hashes").await.unwrap();
    db.execute_raw("DELETE FROM media_tags").await.unwrap();
    db.execute_raw("DELETE FROM media_attributes").await.unwrap();
    db.execute_raw("DELETE FROM tags").await.unwrap();
    db.execute_raw("DELETE FROM media").await.unwrap();
}

#[tokio::test]
async fn d1_migrate_idempotent() {
    let db = connect();
    // schema は既に投入済み (version=6) のため no-op で通るはず
    db.migrate().await.expect("migrate");
}

#[tokio::test]
async fn d1_create_get_find() {
    let db = setup().await;
    let title = format!("d1-cud-{}", Uuid::new_v4());
    let m = db
        .create_media(&MediaInput {
            title: title.clone(),
            media_type: MediaType::Comic,
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(m.title, title);
    assert!(!m.uuid.is_empty());

    let got = db.get_media(m.id).await.unwrap().unwrap();
    assert_eq!(got.id, m.id);
    assert_eq!(got.uuid, m.uuid);

    let found = db.find_media(&MediaFilter::default(), None).await.unwrap();
    assert!(found.iter().any(|x| x.id == m.id), "find_media に含まれる");

    db.delete_media(m.id).await.unwrap();
    assert!(db.get_media(m.id).await.unwrap().is_none());
}

#[tokio::test]
async fn d1_blob_hex_roundtrip() {
    let db = setup().await;
    let m = db
        .create_media(&MediaInput {
            title: format!("d1-hash-{}", Uuid::new_v4()),
            media_type: MediaType::Music,
            ..Default::default()
        })
        .await
        .unwrap();

    // content_hash (BLOB 32byte) を hex で送受信できるか
    let h = vec![0xABu8; 32];
    db.add_media_hash(&MediaHashInput {
        item_uuid: m.uuid.clone(),
        filename: String::new(),
        time_range: String::new(),
        content_hash: h.clone(),
        alternative_of: None,
    })
    .await
    .unwrap();

    let found = db.find_by_content_hash(&h).await.unwrap();
    assert_eq!(found.len(), 1, "content_hash で1件ヒット");
    assert_eq!(found[0].content_hash, h, "BLOB が hex 経由で往復一致");
    assert_eq!(found[0].item_uuid, m.uuid);
    assert_eq!(found[0].embedding, None, "embedding (NULL BLOB) は None");

    db.delete_media(m.id).await.unwrap();
}

/// ローカル→D1 の全データ移行（bulk_load::transfer）が通るか。
///
/// NOTE: 本テストはテスト用 D1 インスタンスの media を開始時に全削除する
/// （`transfer` の検証が件数の完全一致を求めるため）。テスト用インスタンス前提。
/// 既存テスト群も各自の media を delete で掃除しているので、直列実行を想定。
#[tokio::test]
async fn d1_bulk_load_from_local() {
    // ---- source: インメモリのローカルDBにサンプルを詰める ----
    let source = kijuku_db::KijukuDB::open_in_memory().unwrap();
    let src: &dyn KijukuBackend = &source;
    src.migrate().await.unwrap();

    let m = src
        .create_media(&MediaInput {
            title: format!("d1-bulk-{}", Uuid::new_v4()),
            media_type: MediaType::Comic,
            artist: Some("作家".to_string()),
            ..Default::default()
        })
        .await
        .unwrap();
    let tag = src
        .create_tag(&format!("d1-tag-{}", Uuid::new_v4()))
        .await
        .unwrap();
    src.add_tag_to_media(m.id, tag.id).await.unwrap();
    src.set_media_attribute(m.id, "rating", Some("5"), Some(AttributeValueType::Integer))
        .await
        .unwrap();
    src.add_media_hash(&MediaHashInput {
        item_uuid: m.uuid.clone(),
        filename: String::new(),
        time_range: String::new(),
        content_hash: vec![0xCDu8; 32],
        alternative_of: None,
    })
    .await
    .unwrap();

    // ---- dest: D1。開始時に全テーブルを空にする（transfer は空前提・冪等性のため） ----
    let dest = setup().await;
    wipe(&dest).await;

    // ---- 移行 + 検証 ----
    let report = transfer(src, &dest, &TransferOptions::default())
        .await
        .expect("transfer + verify");
    assert_eq!(report.media, 1);
    assert_eq!(report.tags, 1);
    assert_eq!(report.media_tags, 1);
    assert_eq!(report.attributes, 1);
    assert_eq!(report.hashes, 1);
    assert!(report.verified, "件数・内容一致の検証が通ること");

    // D1 固有の懸念: content_hash (BLOB 32byte) が hex 経由で正しく往復したか
    let hashes = dest.get_media_hashes(&m.uuid).await.unwrap();
    assert_eq!(hashes.len(), 1);
    assert_eq!(hashes[0].content_hash, vec![0xCDu8; 32]);

    // ---- クリーンアップ: 全テーブルを空にする（次回実行・他テストへの影響を回避） ----
    wipe(&dest).await;
}
