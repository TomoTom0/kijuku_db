//! bulk_load（export / 全データ移行）のローカル→ローカル統合テスト。
//!
//! 従来 `transfer` / `verify` の検証は feature `d1-integration` の実 D1
//! テスト（`d1_test.rs`）しかなく、通常の `cargo test` では一切実行され
//! なかった。本ファイルは同一公開 API を 2 つのインメモリ DB 間で実行し、
//! consumer が依存する export 契約を通常テストで担保する:
//! - media の `uuid` が保持され、dest 側で `get_media_by_uuid` できる
//! - tags / media_tags / attributes / hashes が欠落なく運ばれる
//! - `verify` が一致で成功し、不一致（dest 側の余分な media）で失敗する
//! - uuid 重複する dest への再転送は失敗する（dest は空前提という契約）

use kijuku_db::{
    transfer, verify, AttributeValueType, KijukuBackend, KijukuDB, MediaHashInput, MediaInput,
    MediaType, TransferOptions,
};

/// tags / attributes / hashes 付きの source DB（media 2件）を作る。
async fn make_source() -> KijukuDB {
    let db = KijukuDB::open_in_memory().unwrap();
    let backend: &dyn KijukuBackend = &db;
    backend.migrate().await.unwrap();

    let m1 = backend
        .create_media(&MediaInput {
            title: "export元コミック".to_string(),
            media_type: MediaType::Comic,
            artist: Some("作者A".to_string()),
            series: Some("シリーズX".to_string()),
            ..Default::default()
        })
        .await
        .unwrap();
    let m2 = backend
        .create_media(&MediaInput {
            title: "export元ビデオ".to_string(),
            media_type: MediaType::Video,
            ..Default::default()
        })
        .await
        .unwrap();

    let tag_shared = backend.create_tag("共通タグ").await.unwrap();
    let tag_m1_only = backend.create_tag("m1専用タグ").await.unwrap();
    backend
        .add_tag_to_media(m1.id, tag_shared.id)
        .await
        .unwrap();
    backend
        .add_tag_to_media(m1.id, tag_m1_only.id)
        .await
        .unwrap();
    backend
        .add_tag_to_media(m2.id, tag_shared.id)
        .await
        .unwrap();

    backend
        .set_media_attribute(
            m1.id,
            "rating",
            Some("5"),
            Some(AttributeValueType::Integer),
        )
        .await
        .unwrap();
    backend
        .set_media_attribute(
            m2.id,
            "note",
            Some("メモ"),
            Some(AttributeValueType::String),
        )
        .await
        .unwrap();

    for (uuid, byte) in [(&m1.uuid, 0x11u8), (&m2.uuid, 0x22u8)] {
        backend
            .add_media_hash(&MediaHashInput {
                item_uuid: uuid.clone(),
                filename: String::new(),
                time_range: String::new(),
                content_hash: vec![byte; 32],
                alternative_of: None,
            })
            .await
            .unwrap();
    }

    db
}

async fn make_empty_dest() -> KijukuDB {
    let db = KijukuDB::open_in_memory().unwrap();
    let backend: &dyn KijukuBackend = &db;
    backend.migrate().await.unwrap();
    db
}

#[tokio::test]
async fn bulk_load_transfer_local_to_local_preserves_uuid_and_all_relations() {
    let source = make_source().await;
    let dest = make_empty_dest().await;
    let src: &dyn KijukuBackend = &source;
    let dst: &dyn KijukuBackend = &dest;

    let src_media = src.find_media(&Default::default(), None).await.unwrap();
    assert_eq!(src_media.len(), 2);

    // chunk_size=1 でページング境界も踏む
    let report = transfer(
        src,
        dst,
        &TransferOptions {
            chunk_size: 1,
            verbose: false,
        },
    )
    .await
    .expect("local -> local transfer + verify");
    assert_eq!(report.media, 2);
    assert_eq!(report.tags, 2);
    assert_eq!(report.media_tags, 3);
    assert_eq!(report.attributes, 2);
    assert_eq!(report.hashes, 2);
    assert!(report.verified);

    // consumer 契約: uuid が保持され、dest 側で uuid 取得できる
    for s in &src_media {
        let d = dst
            .get_media_by_uuid(&s.uuid)
            .await
            .unwrap()
            .expect("dest に同じ uuid の media がある");
        assert_eq!(d.title, s.title);
        assert_eq!(d.media_type, s.media_type);
        assert_eq!(d.artist, s.artist);

        // タグは名前で張り直される
        let mut s_names: Vec<String> = src
            .get_media_tags(s.id)
            .await
            .unwrap()
            .iter()
            .map(|t| t.name.clone())
            .collect();
        let mut d_names: Vec<String> = dst
            .get_media_tags(d.id)
            .await
            .unwrap()
            .iter()
            .map(|t| t.name.clone())
            .collect();
        s_names.sort();
        d_names.sort();
        assert_eq!(s_names, d_names);

        // 属性・ハッシュも一致する
        let d_attrs = dst.get_media_attributes(d.id).await.unwrap();
        assert_eq!(d_attrs.len(), 1);
        let d_hashes = dst.get_media_hashes(&s.uuid).await.unwrap();
        assert_eq!(d_hashes.len(), 1);
        assert_eq!(d_hashes[0].item_uuid, s.uuid);
    }
}

#[tokio::test]
async fn bulk_load_verify_detects_divergence() {
    let source = make_source().await;
    let dest = make_empty_dest().await;
    let src: &dyn KijukuBackend = &source;
    let dst: &dyn KijukuBackend = &dest;

    transfer(src, dst, &TransferOptions::default())
        .await
        .unwrap();

    // 一致状態では単独 verify が通る
    let report = verify(src, dst).await.expect("一致状態の verify");
    assert_eq!(report.media, 2);
    assert_eq!(report.media_tags, 3);

    // dest 側に余分な media を作ると件数不一致で失敗する
    dst.create_media(&MediaInput {
        title: "dest余分".to_string(),
        media_type: MediaType::Music,
        ..Default::default()
    })
    .await
    .unwrap();
    let err = verify(src, dst).await.expect_err("不一致は Err になる");
    assert!(
        err.to_string().contains("media"),
        "media 件数不一致のエラーであること: {err}"
    );
}

#[tokio::test]
async fn bulk_load_transfer_rejects_nonempty_dest_with_same_uuid() {
    let source = make_source().await;
    let dest = make_empty_dest().await;
    let src: &dyn KijukuBackend = &source;
    let dst: &dyn KijukuBackend = &dest;

    transfer(src, dst, &TransferOptions::default())
        .await
        .unwrap();

    // dest は空前提: 同じ uuid 群を再転送すると UNIQUE 制約等で失敗する
    let result = transfer(src, dst, &TransferOptions::default()).await;
    assert!(
        result.is_err(),
        "uuid 重複する dest への再転送は失敗すること（dest は空前提）"
    );
}
