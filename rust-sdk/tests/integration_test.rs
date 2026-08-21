// 結合テストは意図的に deprecated 同期 API を検証（後方互換の保証）
#![allow(deprecated)]

use kijuku_db::{KijukuDB, KijukuBackend, KijukuError, MediaInput, MediaUpdateInput, MediaType, MediaFilter, QueryOptions, SortKey, SortOrder};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::NamedTempFile;

#[test]
fn test_full_workflow() {
    // 一時ファイルでデータベースを作成
    let temp_file = NamedTempFile::new().unwrap();
    let db_path = temp_file.path();

    let db = KijukuDB::open(db_path).unwrap();

    // マイグレーション実行
    db.migrate().unwrap();

    // スキーマバージョンを確認
    let version = db.get_schema_version().unwrap();
    assert_eq!(version, 6);

    // テーブル確認
    let tables = db.get_tables().unwrap();
    assert!(tables.contains(&"media".to_string()));
    assert!(tables.contains(&"tags".to_string()));

    // メディア作成
    let media1 = db.create_media(&MediaInput {
        title: "テストコミック1".to_string(),
        media_type: MediaType::Comic,
        artist: Some("作者A".to_string()),
        series: Some("人気シリーズ".to_string()),
        ..Default::default()
    }).unwrap();

    let media2 = db.create_media(&MediaInput {
        title: "テストコミック2".to_string(),
        media_type: MediaType::Comic,
        artist: Some("作者B".to_string()),
        ..Default::default()
    }).unwrap();

    // メディア取得
    let fetched = db.get_media(media1.id).unwrap();
    assert_eq!(fetched.title, "テストコミック1");
    assert_eq!(fetched.artist, Some("作者A".to_string()));

    // メディア更新（部分更新: 指定したフィールドのみ更新）
    // Option<Option<T>>パターン: Some(Some(val))で値を設定
    db.update_media(media1.id, &MediaUpdateInput {
        title: Some("更新されたタイトル".to_string()),
        description: Some(Some("説明追加".to_string())),
        ..Default::default()
    }).unwrap();

    let updated = db.get_media(media1.id).unwrap();
    assert_eq!(updated.title, "更新されたタイトル");
    assert_eq!(updated.description, Some("説明追加".to_string()));
    // artistは元のまま（部分更新なので変更されない）
    assert_eq!(updated.artist, Some("作者A".to_string()));

    // タグ作成
    let tag1 = db.create_tag("アクション").unwrap();
    let tag2 = db.create_tag("ファンタジー").unwrap();

    // メディアにタグを追加
    db.add_tag_to_media(media1.id, tag1.id).unwrap();
    db.add_tag_to_media(media1.id, tag2.id).unwrap();

    // メディアのタグを取得
    let media_tags = db.get_media_tags(media1.id).unwrap();
    assert_eq!(media_tags.len(), 2);

    // 全タグを取得
    let all_tags = db.get_all_tags().unwrap();
    assert_eq!(all_tags.len(), 2);

    // 検索: メディアタイプでフィルタ
    let filter = MediaFilter {
        media_type: Some(MediaType::Comic),
        ..Default::default()
    };
    let results = db.find_media(&filter, None).unwrap();
    assert_eq!(results.len(), 2);

    // 検索: タグでフィルタ
    let filter = MediaFilter {
        tag_ids: Some(vec![tag1.id]),
        ..Default::default()
    };
    let results = db.find_media(&filter, None).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, media1.id);

    // 検索: ソートとページネーション
    let options = QueryOptions {
        sort_keys: vec![SortKey { field: "title".to_string(), order: SortOrder::Asc }],
        limit: Some(1),
        offset: Some(0),
        ..Default::default()
    };
    let results = db.find_media(&MediaFilter::default(), Some(&options)).unwrap();
    assert_eq!(results.len(), 1);

    // タグを削除
    db.remove_tag_from_media(media1.id, tag1.id).unwrap();
    let media_tags = db.get_media_tags(media1.id).unwrap();
    assert_eq!(media_tags.len(), 1);

    // メディアを削除
    db.delete_media(media2.id).unwrap();
    assert!(db.get_media(media2.id).is_none());

    // バルク作成
    let bulk_data = vec![
        MediaInput {
            title: "バルク1".to_string(),
            media_type: MediaType::Video,
            ..Default::default()
        },
        MediaInput {
            title: "バルク2".to_string(),
            media_type: MediaType::Music,
            ..Default::default()
        },
    ];

    let bulk_results = db.bulk_create_media(&bulk_data).unwrap();
    assert_eq!(bulk_results.len(), 2);
}

#[test]
fn test_update_nullable_fields() {
    let temp_file = NamedTempFile::new().unwrap();
    let db = KijukuDB::open(temp_file.path()).unwrap();
    db.migrate().unwrap();

    // nullableフィールド付きでメディア作成
    let media = db.create_media(&MediaInput {
        title: "Nullable Test".to_string(),
        media_type: MediaType::Comic,
        artist: Some("Original Artist".to_string()),
        series: Some("Original Series".to_string()),
        description: Some("Original Description".to_string()),
        ..Default::default()
    }).unwrap();
    assert_eq!(media.artist, Some("Original Artist".to_string()));
    assert_eq!(media.series, Some("Original Series".to_string()));

    // Case 1: 値の更新 (Some(Some(value)))
    db.update_media(media.id, &MediaUpdateInput {
        artist: Some(Some("Updated Artist".to_string())),
        ..Default::default()
    }).unwrap();
    let updated = db.get_media(media.id).unwrap();
    assert_eq!(updated.artist, Some("Updated Artist".to_string()));
    // 他のフィールドは変更されない
    assert_eq!(updated.series, Some("Original Series".to_string()));
    assert_eq!(updated.description, Some("Original Description".to_string()));

    // Case 2: NULL設定 (Some(None))
    db.update_media(media.id, &MediaUpdateInput {
        artist: Some(None),
        description: Some(None),
        ..Default::default()
    }).unwrap();
    let updated = db.get_media(media.id).unwrap();
    assert_eq!(updated.artist, None);
    assert_eq!(updated.description, None);
    // 他のフィールドは変更されない
    assert_eq!(updated.series, Some("Original Series".to_string()));

    // Case 3: 更新なし (None)
    db.update_media(media.id, &MediaUpdateInput {
        ..Default::default()
    }).unwrap();
    let updated = db.get_media(media.id).unwrap();
    assert_eq!(updated.artist, None);
    assert_eq!(updated.series, Some("Original Series".to_string()));

    // Case 4: 整数nullableフィールドのNULL設定
    db.update_media(media.id, &MediaUpdateInput {
        file_size: Some(Some(1024)),
        page_count: Some(Some(100)),
        ..Default::default()
    }).unwrap();
    let updated = db.get_media(media.id).unwrap();
    assert_eq!(updated.file_size, Some(1024));
    assert_eq!(updated.page_count, Some(100));

    db.update_media(media.id, &MediaUpdateInput {
        file_size: Some(None),
        page_count: Some(None),
        ..Default::default()
    }).unwrap();
    let updated = db.get_media(media.id).unwrap();
    assert_eq!(updated.file_size, None);
    assert_eq!(updated.page_count, None);
}

#[test]
fn test_update_media_json_deserialization() {
    let temp_file = NamedTempFile::new().unwrap();
    let db = KijukuDB::open(temp_file.path()).unwrap();
    db.migrate().unwrap();

    let media = db.create_media(&MediaInput {
        title: "JSON Deser Test".to_string(),
        media_type: MediaType::Comic,
        artist: Some("Original".to_string()),
        ..Default::default()
    }).unwrap();

    // JSONからdeserialization: artist = null → Some(None)
    let json = serde_json::json!({
        "artist": null
    });
    let input: MediaUpdateInput = serde_json::from_value(json).unwrap();
    assert_eq!(input.artist, Some(None));
    assert!(input.title.is_none());

    db.update_media(media.id, &input).unwrap();
    let updated = db.get_media(media.id).unwrap();
    assert_eq!(updated.artist, None);
    assert_eq!(updated.title, "JSON Deser Test"); // 変更されない

    // JSONからdeserialization: artist = "NewValue" → Some(Some("NewValue"))
    let json = serde_json::json!({
        "artist": "NewValue"
    });
    let input: MediaUpdateInput = serde_json::from_value(json).unwrap();
    assert_eq!(input.artist, Some(Some("NewValue".to_string())));

    db.update_media(media.id, &input).unwrap();
    let updated = db.get_media(media.id).unwrap();
    assert_eq!(updated.artist, Some("NewValue".to_string()));

    // JSONからdeserialization: 空オブジェクト → 全てNone
    let json = serde_json::json!({});
    let input: MediaUpdateInput = serde_json::from_value(json).unwrap();
    assert!(input.artist.is_none());
    assert!(input.title.is_none());

    // UUIDにnullは設定できない
    let json = serde_json::json!({
        "uuid": null
    });
    let input: MediaUpdateInput = serde_json::from_value(json).unwrap();
    let result = db.update_media(media.id, &input);
    assert!(result.is_err());
}

#[test]
fn test_error_handling() {
    let temp_file = NamedTempFile::new().unwrap();
    let db = KijukuDB::open(temp_file.path()).unwrap();
    db.migrate().unwrap();

    // 存在しないメディアの取得
    assert!(db.get_media(9999).is_none());

    // 存在しないメディアの更新
    let result = db.update_media(9999, &MediaUpdateInput {
        title: Some("存在しない".to_string()),
        ..Default::default()
    });
    assert!(result.is_err());

    // 存在しないメディアの削除
    let result = db.delete_media(9999);
    assert!(result.is_err());
}

#[test]
fn test_transaction_success() {
    let temp_file = NamedTempFile::new().unwrap();
    let db_path = temp_file.path();
    let db = KijukuDB::open(db_path).unwrap();
    db.migrate().unwrap();

    // トランザクション内で複数のメディアを作成
    let result = db.transaction(|db| {
        db.create_media(&MediaInput {
            title: "トランザクション内メディア1".to_string(),
            media_type: MediaType::Comic,
            ..Default::default()
        })?;

        db.create_media(&MediaInput {
            title: "トランザクション内メディア2".to_string(),
            media_type: MediaType::Comic,
            ..Default::default()
        })?;

        Ok(())
    });

    assert!(result.is_ok());

    // 両方のメディアが作成されていることを確認
    let all_media = db.find_media(&MediaFilter::default(), None).unwrap();
    assert_eq!(all_media.len(), 2);
}

#[test]
fn test_transaction_rollback() {
    let temp_file = NamedTempFile::new().unwrap();
    let db_path = temp_file.path();
    let db = KijukuDB::open(db_path).unwrap();
    db.migrate().unwrap();

    // トランザクション内でエラーが発生した場合、ロールバックされる
    let result: Result<(), _> = db.transaction(|db| {
        db.create_media(&MediaInput {
            title: "メディア1".to_string(),
            media_type: MediaType::Comic,
            ..Default::default()
        })?;

        // 2つ目の作成は成功
        db.create_media(&MediaInput {
            title: "メディア2".to_string(),
            media_type: MediaType::Comic,
            ..Default::default()
        })?;

        // エラーを返す
        Err(kijuku_db::KijukuError::Other("意図的なエラー".to_string()))
    });

    assert!(result.is_err());

    // トランザクションがロールバックされたので、メディアは作成されていない
    let all_media = db.find_media(&MediaFilter::default(), None).unwrap();
    assert_eq!(all_media.len(), 0);
}

#[test]
fn test_transaction_with_tags() {
    let temp_file = NamedTempFile::new().unwrap();
    let db_path = temp_file.path();
    let db = KijukuDB::open(db_path).unwrap();
    db.migrate().unwrap();

    // トランザクション内でメディアとタグを作成し、関連付ける
    let (media_id, tag_id) = db.transaction(|db| {
        let media = db.create_media(&MediaInput {
            title: "タグ付きメディア".to_string(),
            media_type: MediaType::Comic,
            ..Default::default()
        })?;

        let tag = db.create_tag("アクション")?;
        db.add_tag_to_media(media.id, tag.id)?;

        Ok((media.id, tag.id))
    }).unwrap();

    // メディアとタグが正しく作成され、関連付けられていることを確認
    let media = db.get_media(media_id).unwrap();
    assert_eq!(media.title, "タグ付きメディア");

    let tags = db.get_media_tags(media_id).unwrap();
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].id, tag_id);
    assert_eq!(tags[0].name, "アクション");
}

#[test]
fn test_close_method() {
    let temp_file = NamedTempFile::new().unwrap();
    let db_path = temp_file.path();
    let db = KijukuDB::open(db_path).unwrap();
    db.migrate().unwrap();

    // メディアを作成
    db.create_media(&MediaInput {
        title: "テストメディア".to_string(),
        media_type: MediaType::Comic,
        ..Default::default()
    }).unwrap();

    // 明示的にクローズ
    db.close();

    // この後、db は使用できない（コンパイル時にエラーになる）
    // db.get_media(1); // これはコンパイルエラー

    // データベースを再度開いてデータが保存されていることを確認
    let db2 = KijukuDB::open(db_path).unwrap();
    let media = db2.get_media(1);
    assert!(media.is_some());
    assert_eq!(media.unwrap().title, "テストメディア");
}

#[test]
fn test_transaction_serializes_concurrent_access() {
    // トランザクション実行中、別スレッドからのクエリは COMMIT/ROLLBACK までブロックされる。
    // ReentrantMutex による接続ロック保持で直列化されることを検証する。
    let temp_file = NamedTempFile::new().unwrap();
    let db = Arc::new(KijukuDB::open(temp_file.path()).unwrap());
    db.migrate().unwrap();

    let barrier = Arc::new(Barrier::new(2));
    let db2 = Arc::clone(&db);
    let barrier2 = Arc::clone(&barrier);

    // スレッド A: トランザクション内で barrier で待ち合わせし、ロックを保持したまま待機
    let handle = thread::spawn(move || {
        db2.transaction(|db| {
            db.create_media(&MediaInput {
                title: "tx".to_string(),
                media_type: MediaType::Comic,
                ..Default::default()
            })?;
            barrier2.wait(); // トランザクション保持中にスレッド B へ通知
            thread::sleep(Duration::from_millis(200)); // ロックを保持したまま待機
            Ok::<_, KijukuError>(())
        })
        .unwrap();
    });

    barrier.wait(); // スレッド A がトランザクションを開始するまで待機
    let start = Instant::now();
    // 主スレッドからのクエリはトランザクション終了までブロックされるはず
    let _found = db.find_media(&MediaFilter::default(), None).unwrap();
    let elapsed = start.elapsed();
    assert!(
        elapsed >= Duration::from_millis(150),
        "他スレッドのトランザクション中はブロックされるべき: {:?}",
        elapsed
    );
    handle.join().unwrap();
}

#[test]
fn test_transaction_rollback_isolation() {
    // ロールバックされるトランザクションの未コミット内容は他スレッドから見えないことを検証。
    // 旧実装では BEGIN〜ROLLBACK 間に別スレッドのクエリが巻き込まれる競合があった。
    let temp_file = NamedTempFile::new().unwrap();
    let db = Arc::new(KijukuDB::open(temp_file.path()).unwrap());
    db.migrate().unwrap();

    let started = Arc::new(AtomicBool::new(false));
    let db2 = Arc::clone(&db);
    let started2 = Arc::clone(&started);
    let handle = thread::spawn(move || {
        let _ = db2.transaction(|db| {
            db.create_media(&MediaInput {
                title: "m1".to_string(),
                media_type: MediaType::Comic,
                ..Default::default()
            })?;
            started2.store(true, Ordering::SeqCst); // トランザクション開始を通知
            thread::sleep(Duration::from_millis(300)); // ロック保持して待機
            Err::<(), KijukuError>(KijukuError::Other("intentional rollback".to_string()))
        });
    });

    // スレッド A がトランザクションを開始するまで待機
    while !started.load(Ordering::SeqCst) {
        thread::sleep(Duration::from_millis(10));
    }
    // このクエリはトランザクションの ROLLBACK が終わるまでブロックされ、
    // ロールバック後は空（未コミット内容は見えない）
    let found = db.find_media(&MediaFilter::default(), None).unwrap();
    assert_eq!(
        found.len(),
        0,
        "ロールバックされたトランザクションの内容は見えないべき"
    );
    handle.join().unwrap();
}

#[test]
fn test_transaction_panic_rolls_back() {
    // クロージャ内でパニックが発生した場合、Transaction の Drop により自動的に
    // ROLLBACK され、接続がトランザクション状態で残留しないことを検証する。
    // 手動 SQL の BEGIN/COMMIT/ROLLBACK ではパニック経路を捕捉できず、接続が
    // トランザクション開いたまま残留し、以後の操作が失敗する問題があった。
    let temp_file = NamedTempFile::new().unwrap();
    let db = KijukuDB::open(temp_file.path()).unwrap();
    db.migrate().unwrap();

    // 事前に1件コミットしておく
    db.create_media(&MediaInput {
        title: "committed".to_string(),
        media_type: MediaType::Comic,
        ..Default::default()
    })
    .unwrap();

    // トランザクション内で2件目を作成してからパニック
    let result: Result<Result<(), KijukuError>, _> =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            db.transaction(|db| {
                db.create_media(&MediaInput {
                    title: "panicked".to_string(),
                    media_type: MediaType::Comic,
                    ..Default::default()
                })?;
                panic!("intentional panic inside transaction");
            })
        }));
    assert!(result.is_err(), "クロージャ内のパニックは伝播するべき");

    // パニック後: 未コミットのメディアはロールバックされ、コミット済み1件のみ残る
    let all_media = db.find_media(&MediaFilter::default(), None).unwrap();
    assert_eq!(
        all_media.len(),
        1,
        "パニック時の未コミット内容はロールバックされるべき"
    );
    assert_eq!(all_media[0].title, "committed");

    // 接続がトランザクション状態で残留していないことの検証:
    // 残留していた場合、新規トランザクション開始で SQLite エラーになる。
    let r = db.transaction(|db| {
        db.create_media(&MediaInput {
            title: "after_panic".to_string(),
            media_type: MediaType::Comic,
            ..Default::default()
        })?;
        Ok::<_, KijukuError>(())
    });
    assert!(
        r.is_ok(),
        "パニック後に新規トランザクションが開始できるべき: {:?}",
        r.err()
    );

    let all_media = db.find_media(&MediaFilter::default(), None).unwrap();
    assert_eq!(all_media.len(), 2);
}

#[test]
fn test_get_media_by_uuid() {
    let temp_file = NamedTempFile::new().unwrap();
    let db = KijukuDB::open(temp_file.path()).unwrap();
    db.migrate().unwrap();

    let manual_uuid = "550e8400-e29b-41d4-a716-446655440000".to_string();
    let media = db.create_media(&MediaInput {
        title: "UUID取得テスト".to_string(),
        media_type: MediaType::Comic,
        uuid: Some(manual_uuid.clone()),
        ..Default::default()
    }).unwrap();

    // UUIDで取得（sync deprecated API）
    let fetched = db.get_media_by_uuid(&manual_uuid).unwrap();
    assert_eq!(fetched.id, media.id);
    assert_eq!(fetched.uuid, manual_uuid);

    // 存在しないUUIDはNone
    assert!(db.get_media_by_uuid("00000000-0000-0000-0000-000000000000").is_none());
}

#[tokio::test]
async fn test_get_media_by_uuid_async() {
    let temp_file = NamedTempFile::new().unwrap();
    let db = KijukuDB::open(temp_file.path()).unwrap();
    db.migrate().unwrap();

    let manual_uuid = "6ba7b810-9dad-11d1-80b4-00c04fd430c8".to_string();
    let media = db.create_media(&MediaInput {
        title: "UUID取得テスト（async）".to_string(),
        media_type: MediaType::Comic,
        uuid: Some(manual_uuid.clone()),
        ..Default::default()
    }).unwrap();

    // UUIDで取得（KijukuBackend async API）
    let fetched = KijukuBackend::get_media_by_uuid(&db, &manual_uuid).await.unwrap().unwrap();
    assert_eq!(fetched.id, media.id);
    assert_eq!(fetched.uuid, manual_uuid);

    // 存在しないUUIDはNone
    assert!(KijukuBackend::get_media_by_uuid(&db, "00000000-0000-0000-0000-000000000000").await.unwrap().is_none());
}

#[tokio::test]
async fn test_find_media_by_uuid_filter() {
    let temp_file = NamedTempFile::new().unwrap();
    let db = KijukuDB::open(temp_file.path()).unwrap();
    db.migrate().unwrap();

    let uuid1 = "550e8400-e29b-41d4-a716-446655440000".to_string();
    let uuid2 = "6ba7b810-9dad-11d1-80b4-00c04fd430c8".to_string();
    for (title, uuid) in [("作品1", uuid1.clone()), ("作品2", uuid2.clone())] {
        db.create_media(&MediaInput {
            title: title.to_string(),
            media_type: MediaType::Comic,
            uuid: Some(uuid),
            ..Default::default()
        }).unwrap();
    }
    db.create_media(&MediaInput {
        title: "自動UUID".to_string(),
        media_type: MediaType::Comic,
        ..Default::default()
    }).unwrap();

    // uuid完全一致
    let results = KijukuBackend::find_media(
        &db,
        &MediaFilter { uuid: Some(uuid1.clone()), ..Default::default() },
        None,
    ).await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].uuid, uuid1);

    // uuid_in（重複指定を含む）
    let results = KijukuBackend::find_media(
        &db,
        &MediaFilter { uuid_in: Some(vec![uuid1.clone(), uuid2.clone(), uuid1.clone()]), ..Default::default() },
        None,
    ).await.unwrap();
    assert_eq!(results.len(), 2);
}

#[tokio::test]
async fn test_find_media_by_uuid_in_over_param_limit_async() {
    // SQLite のパラメータ数上限（999）を超える uuid_in が async API
    // （find_media_async: Local/D1 共通の SqlParam 実装）でも動作することを確認
    // （json_each による1パラメータ化の回帰テスト）
    let temp_file = NamedTempFile::new().unwrap();
    let db = KijukuDB::open(temp_file.path()).unwrap();
    db.migrate().unwrap();

    const COUNT: usize = 1200;
    let mut uuids = Vec::with_capacity(COUNT);
    for i in 0..COUNT {
        let uuid = format!("{:08x}-0000-4000-8000-{:012x}", i, i);
        db.create_media(&MediaInput {
            title: format!("作品{}", i),
            media_type: MediaType::Comic,
            uuid: Some(uuid.clone()),
            ..Default::default()
        }).unwrap();
        uuids.push(uuid);
    }

    let results = KijukuBackend::find_media(
        &db,
        &MediaFilter { uuid_in: Some(uuids.clone()), ..Default::default() },
        None,
    ).await.unwrap();
    assert_eq!(results.len(), COUNT);

    let got: std::collections::HashSet<&str> =
        results.iter().map(|m| m.uuid.as_str()).collect();
    assert!(got.contains(uuids[0].as_str()));
    assert!(got.contains(uuids[COUNT - 1].as_str()));
}

#[tokio::test]
async fn test_get_media_tags_bulk_deduplicated() {
    // 重複した media_ids を渡しても、結果の HashMap にタグが重複して登録されないこと
    // （IN句は集合扱いだが、ユニーク化でプレースホルダー無駄増加も防止）を検証する。
    let temp_file = NamedTempFile::new().unwrap();
    let db = KijukuDB::open(temp_file.path()).unwrap();
    db.migrate().unwrap();

    let media = db
        .create_media(&MediaInput {
            title: "tagged".to_string(),
            media_type: MediaType::Comic,
            ..Default::default()
        })
        .unwrap();
    let tag = db.create_tag("アクション").unwrap();
    db.add_tag_to_media(media.id, tag.id).unwrap();

    // media.id を重複して指定しても1エントリ・1タグ（重複なし）
    let result = db
        .get_media_tags_bulk(&[media.id, media.id, media.id])
        .await
        .unwrap();
    assert_eq!(result.len(), 1);
    let tags = result
        .get(&media.id)
        .expect("media.id のエントリが存在するべき");
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].name, "アクション");
}
