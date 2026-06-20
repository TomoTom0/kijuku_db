// 結合テストは意図的に deprecated 同期 API を検証（後方互換の保証）
#![allow(deprecated)]

use kijuku_db::{KijukuDB, MediaInput, MediaUpdateInput, MediaType, MediaFilter, QueryOptions, SortKey, SortOrder};
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
