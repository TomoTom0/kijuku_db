//! bulk_load（export / 全データ移行）のローカル→ローカル統合テスト。
//!
//! 従来 `transfer` / `verify` の検証は feature `d1-integration` の実 D1
//! テスト（`d1_test.rs`）しかなく、通常の `cargo test` では一切実行され
//! なかった。本ファイルは同一公開 API を 2 つのインメモリ DB 間で実行し、
//! consumer が依存する export 契約を通常テストで担保する:
//! - media の `uuid` が保持され、dest 側で `get_media_by_uuid` できる
//! - tags / media_tags / attributes / hashes が欠落なく運ばれる
//!   （属性の key/value/value_type・ハッシュの content_hash・`alternative_of`
//!   の代替連鎖まで内容レベルで一致する）
//! - `verify` が一致で成功し、不一致で失敗する: 件数不一致（dest 側の余分な
//!   media）に加え、件数が同じでも内容が違う場合（title 改変・属性値改変・
//!   ハッシュ欠落）も検出する
//! - uuid 重複する dest への再転送は失敗する（dest は空前提という契約）

use kijuku_db::{
    transfer, verify, AttributeValueType, KijukuBackend, KijukuDB, MediaHashInput, MediaInput,
    MediaType, MediaUpdateInput, TransferOptions,
};

/// テスト内の期待値で使う原本ハッシュ（Comic の `001.jpg`）。
const PRIMARY_HASH: [u8; 32] = [0x11; 32];
/// テスト内の期待値で使う代替ハッシュ（Comic の `001.gif`）。
const ALT_HASH: [u8; 32] = [0x13; 32];
/// テスト内の期待値で使う Video 全体ハッシュ。
const VIDEO_HASH: [u8; 32] = [0x22; 32];

/// tags / attributes / hashes 付きの source DB（media 2件）を作る。
///
/// Comic 側（m1）は原本 `001.jpg` に加えて代替フォーマット `001.gif`
/// （`alternative_of = "001.jpg"`）の 2 ハッシュを持つ
/// （`docs/design/content-hash.md` の Comic 表現に従う）。
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

    // m1 (Comic): 原本 + 代替フォーマット。alternative_of は原本の filename を指す
    let comic_hashes = [
        ("001.jpg", PRIMARY_HASH, None),
        ("001.gif", ALT_HASH, Some("001.jpg")),
    ];
    for (filename, hash, alternative_of) in comic_hashes {
        backend
            .add_media_hash(&MediaHashInput {
                item_uuid: m1.uuid.clone(),
                filename: filename.to_string(),
                time_range: String::new(),
                content_hash: hash.to_vec(),
                alternative_of: alternative_of.map(str::to_string),
            })
            .await
            .unwrap();
    }

    // m2 (Video): 全体を表す 1 ハッシュ（filename/time_range 空文字）
    backend
        .add_media_hash(&MediaHashInput {
            item_uuid: m2.uuid.clone(),
            filename: String::new(),
            time_range: String::new(),
            content_hash: VIDEO_HASH.to_vec(),
            alternative_of: None,
        })
        .await
        .unwrap();

    db
}

async fn make_empty_dest() -> KijukuDB {
    let db = KijukuDB::open_in_memory().unwrap();
    let backend: &dyn KijukuBackend = &db;
    backend.migrate().await.unwrap();
    db
}

/// 属性集合を (key, value, value_type) の組として比較用に取り出す。
/// `AttributeValueType` は `Ord` を持たないため value_type は文字列化する
/// （`bulk_load.rs` の verify 実装と同じ方針）。
fn attr_tuples(
    attrs: &[kijuku_db::MediaAttribute],
) -> std::collections::BTreeSet<(String, Option<String>, String)> {
    attrs
        .iter()
        .map(|a| {
            (
                a.key.clone(),
                a.value.clone(),
                a.value_type.as_str().to_string(),
            )
        })
        .collect()
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
    assert_eq!(report.hashes, 3); // Comic 2件（原本+代替） + Video 1件
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

        // 属性: 件数だけでなく key / value / value_type の内容まで一致する
        let s_attrs = src.get_media_attributes(s.id).await.unwrap();
        let d_attrs = dst.get_media_attributes(d.id).await.unwrap();
        assert_eq!(d_attrs.len(), 1);
        assert_eq!(attr_tuples(&s_attrs), attr_tuples(&d_attrs));

        // ハッシュ: 件数・item_uuid・内容（filename / content_hash / alternative_of）
        let s_hashes = src.get_media_hashes(&s.uuid).await.unwrap();
        let d_hashes = dst.get_media_hashes(&s.uuid).await.unwrap();
        assert_eq!(d_hashes.len(), s_hashes.len());
        for h in &d_hashes {
            assert_eq!(h.item_uuid, s.uuid);
        }
        let to_hash_key = |h: &kijuku_db::MediaHash| {
            (
                h.filename.clone(),
                h.time_range.clone(),
                h.content_hash.clone(),
                h.alternative_of.clone(),
            )
        };
        let s_hash_set: std::collections::BTreeSet<_> =
            s_hashes.iter().map(to_hash_key).collect();
        let d_hash_set: std::collections::BTreeSet<_> =
            d_hashes.iter().map(to_hash_key).collect();
        assert_eq!(d_hash_set, s_hash_set);

        // media ごとの期待値を明示的に検証（失敗時の診断を容易にするため）
        if s.title == "export元コミック" {
            assert_eq!(
                attr_tuples(&d_attrs),
                [(
                    "rating".to_string(),
                    Some("5".to_string()),
                    "integer".to_string()
                )]
                .into_iter()
                .collect::<std::collections::BTreeSet<_>>()
            );
            assert_eq!(d_hashes.len(), 2);

            // 代替ハッシュの連鎖（alternative_of）が正しく運ばれる
            let alt = d_hashes
                .iter()
                .find(|h| h.filename == "001.gif")
                .expect("代替ハッシュ (001.gif) が転送されている");
            assert_eq!(alt.content_hash, ALT_HASH.to_vec());
            assert_eq!(
                alt.alternative_of.as_deref(),
                Some("001.jpg"),
                "alternative_of が原本の filename を指すこと"
            );
            let primary = d_hashes
                .iter()
                .find(|h| h.filename == "001.jpg")
                .expect("原本ハッシュ (001.jpg) が転送されている");
            assert_eq!(primary.content_hash, PRIMARY_HASH.to_vec());
            assert!(primary.alternative_of.is_none());
        } else {
            assert_eq!(
                attr_tuples(&d_attrs),
                [(
                    "note".to_string(),
                    Some("メモ".to_string()),
                    "string".to_string()
                )]
                .into_iter()
                .collect::<std::collections::BTreeSet<_>>()
            );
            assert_eq!(d_hashes.len(), 1);
            assert_eq!(d_hashes[0].content_hash, VIDEO_HASH.to_vec());
            assert!(d_hashes[0].alternative_of.is_none());
        }
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
        err.to_string().contains("media 件数不一致"),
        "media 件数不一致のエラーであること: {err}"
    );
}

#[tokio::test]
async fn bulk_load_verify_detects_content_divergence() {
    // 件数が同じでも内容が違えば verify は失敗する。verify は最初の不一致で
    // Err を返すため、シナリオごとに source / dest を作り直す。

    // (1) media 内容不一致: dest 側の title を書き換える
    {
        let source = make_source().await;
        let dest = make_empty_dest().await;
        let src: &dyn KijukuBackend = &source;
        let dst: &dyn KijukuBackend = &dest;
        transfer(src, dst, &TransferOptions::default())
            .await
            .unwrap();

        let victim = &src.find_media(&Default::default(), None).await.unwrap()[0];
        let d = dst
            .get_media_by_uuid(&victim.uuid)
            .await
            .unwrap()
            .expect("転送済み media");
        dst.update_media(
            d.id,
            &MediaUpdateInput {
                title: Some("title書き換え".to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let err = verify(src, dst).await.expect_err("title 改変は検出される");
        assert!(
            err.to_string().contains("media 内容不一致"),
            "media 内容不一致のエラーであること: {err}"
        );
    }

    // (2) attributes 不一致: dest 側の属性値を書き換える（rating 5 -> 3）
    {
        let source = make_source().await;
        let dest = make_empty_dest().await;
        let src: &dyn KijukuBackend = &source;
        let dst: &dyn KijukuBackend = &dest;
        transfer(src, dst, &TransferOptions::default())
            .await
            .unwrap();

        let comic = src
            .find_media(&Default::default(), None)
            .await
            .unwrap()
            .into_iter()
            .find(|m| m.title == "export元コミック")
            .expect("Comic (m1)");
        let d = dst
            .get_media_by_uuid(&comic.uuid)
            .await
            .unwrap()
            .expect("転送済み media");
        dst.set_media_attribute(
            d.id,
            "rating",
            Some("3"),
            Some(AttributeValueType::Integer),
        )
        .await
        .unwrap();

        let err = verify(src, dst).await.expect_err("属性値改変は検出される");
        assert!(
            err.to_string().contains("attributes 不一致"),
            "attributes 不一致のエラーであること: {err}"
        );
    }

    // (3) hashes 不一致: dest 側のハッシュを欠落させる
    {
        let source = make_source().await;
        let dest = make_empty_dest().await;
        let src: &dyn KijukuBackend = &source;
        let dst: &dyn KijukuBackend = &dest;
        transfer(src, dst, &TransferOptions::default())
            .await
            .unwrap();

        let comic = src
            .find_media(&Default::default(), None)
            .await
            .unwrap()
            .into_iter()
            .find(|m| m.title == "export元コミック")
            .expect("Comic (m1)");
        dst.delete_media_hashes(&comic.uuid).await.unwrap();

        let err = verify(src, dst).await.expect_err("ハッシュ欠落は検出される");
        assert!(
            err.to_string().contains("hashes 不一致"),
            "hashes 不一致のエラーであること: {err}"
        );
    }
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
