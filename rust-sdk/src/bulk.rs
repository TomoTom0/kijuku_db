use crate::crud::{build_update_media_sql, create_media, create_media_async, delete_media, update_media};
use crate::db_value::SqlParam;
use crate::error::Result;
use crate::exec::SqlExec;
use crate::types::{BulkUpdateItem, Media, MediaInput};
use rusqlite::Connection;

const DEFAULT_MAX_BATCH_SIZE: usize = 500;

/// 複数のメディアを一括作成
///
/// 大量データでのDBロック長期化を防ぐため、DEFAULT_MAX_BATCH_SIZE件ごとに
/// トランザクションを分割して処理する。
pub fn bulk_create_media(conn: &Connection, data_list: &[MediaInput]) -> Result<Vec<Media>> {
    if data_list.is_empty() {
        return Ok(Vec::new());
    }

    let mut results = Vec::new();
    for chunk in data_list.chunks(DEFAULT_MAX_BATCH_SIZE) {
        let tx = conn.unchecked_transaction()?;
        for data in chunk {
            let media = create_media(&tx, data)?;
            results.push(media);
        }
        tx.commit()?;
    }

    Ok(results)
}

/// 複数のメディアを一括削除
///
/// 大量データでのDBロック長期化を防ぐため、DEFAULT_MAX_BATCH_SIZE件ごとに
/// トランザクションを分割して処理する。
pub fn bulk_delete_media(conn: &Connection, ids: &[i64]) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }

    for chunk in ids.chunks(DEFAULT_MAX_BATCH_SIZE) {
        let tx = conn.unchecked_transaction()?;
        for id in chunk {
            delete_media(&tx, *id)?;
        }
        tx.commit()?;
    }

    Ok(())
}

/// 複数のメディアを一括更新（部分更新）
///
/// MediaUpdateInputを使用して、指定されたフィールドのみを更新します。
/// 大量データでのDBロック長期化を防ぐため、DEFAULT_MAX_BATCH_SIZE件ごとに
/// トランザクションを分割して処理する。
pub fn bulk_update_media(conn: &Connection, updates: &[BulkUpdateItem]) -> Result<()> {
    if updates.is_empty() {
        return Ok(());
    }

    for chunk in updates.chunks(DEFAULT_MAX_BATCH_SIZE) {
        let tx = conn.unchecked_transaction()?;
        for item in chunk {
            update_media(&tx, item.id, &item.data)?;
        }
        tx.commit()?;
    }

    Ok(())
}

// ========== async バックエンド（SqlExec）用 ==========
//
// Local と D1 が共有。D1 は REST でトランザクションを開けないため、`execute_batch`
// でチャンク単位に実行する（Local は原子的・D1 は順次実行で近似）。作成（`create`）は戻り値の行が必要なため
// 件ごとに `create_media_async`（`RETURNING *`）を呼ぶ。delete/update は存在確認を
// 省略したバッチ実行とし、対象なき id は無視される（バッチ近似の制約）。

/// async バックエンド経由で複数のメディアを一括作成。
///
/// 作成された `Media`（id/uuid 含む）を返す必要があるため、件ごとに `create_media_async`
/// を呼ぶ。D1 では件数分の往復になるが、作成行の取得には `RETURNING *` が必須。
pub async fn bulk_create_media_async(
    exec: &dyn SqlExec,
    data_list: &[MediaInput],
) -> Result<Vec<Media>> {
    if data_list.is_empty() {
        return Ok(Vec::new());
    }

    let mut results = Vec::with_capacity(data_list.len());
    for data in data_list {
        results.push(create_media_async(exec, data).await?);
    }
    Ok(results)
}

/// async バックエンド経由で複数のメディアを一括削除。
///
/// `DEFAULT_MAX_BATCH_SIZE` 件ごとに `execute_batch`（Local はチャンク単位で原子的、D1 は順次実行）。
/// バッチ DELETE は存在確認を行わないため、存在しない id は無視される
/// （同期版 `delete_media` の存在確認エラーとは挙動が異なる・バッチ近似の制約）。
pub async fn bulk_delete_media_async(exec: &dyn SqlExec, ids: &[i64]) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }

    for chunk in ids.chunks(DEFAULT_MAX_BATCH_SIZE) {
        let stmts: Vec<(String, Vec<SqlParam>)> = chunk
            .iter()
            .map(|id| {
                (
                    "DELETE FROM media WHERE id = ?1".to_string(),
                    vec![SqlParam::Int(*id)],
                )
            })
            .collect();
        exec.execute_batch(stmts).await?;
    }

    Ok(())
}

/// async バックエンド経由で複数のメディアを一括更新（部分更新）。
///
/// `DEFAULT_MAX_BATCH_SIZE` 件ごとに `execute_batch`（Local はチャンク単位で原子的、D1 は順次実行）。
/// 各件の SQL を `build_update_media_sql` で組み立て、更新フィールドなしはスキップ。
/// ビルド時のバリデーションエラー（例: uuid への NULL）はチャンク実行前に伝播し、
/// そのチャンクは実行されない。対象なき id は無視される（バッチ近似の制約）。
pub async fn bulk_update_media_async(
    exec: &dyn SqlExec,
    updates: &[BulkUpdateItem],
) -> Result<()> {
    if updates.is_empty() {
        return Ok(());
    }

    for chunk in updates.chunks(DEFAULT_MAX_BATCH_SIZE) {
        let mut stmts: Vec<(String, Vec<SqlParam>)> = Vec::new();
        for item in chunk {
            if let Some((sql, params)) = build_update_media_sql(item.id, &item.data)? {
                stmts.push((sql, params));
            }
        }
        if !stmts.is_empty() {
            exec.execute_batch(stmts).await?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crud::get_media_async;
    use crate::exec_local::LocalExec;
    use crate::migration;
    use crate::types::MediaType;
    use parking_lot::ReentrantMutex;
    use std::sync::Arc;

    fn setup_async() -> LocalExec {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();
        LocalExec::new(Arc::new(ReentrantMutex::new(conn)))
    }

    #[tokio::test]
    async fn test_bulk_create_media_async() {
        let exec = setup_async();
        let data_list = vec![
            MediaInput {
                title: "メディア1".to_string(),
                media_type: MediaType::Comic,
                ..Default::default()
            },
            MediaInput {
                title: "メディア2".to_string(),
                media_type: MediaType::Video,
                ..Default::default()
            },
        ];

        let results = bulk_create_media_async(&exec, &data_list).await.unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].title, "メディア1");
        assert_eq!(results[1].title, "メディア2");
    }

    #[tokio::test]
    async fn test_bulk_create_empty_async() {
        let exec = setup_async();
        let results = bulk_create_media_async(&exec, &[]).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_bulk_delete_media_async() {
        let exec = setup_async();
        let results = bulk_create_media_async(
            &exec,
            &[
                MediaInput {
                    title: "m1".to_string(),
                    media_type: MediaType::Comic,
                    ..Default::default()
                },
                MediaInput {
                    title: "m2".to_string(),
                    media_type: MediaType::Video,
                    ..Default::default()
                },
            ],
        )
        .await
        .unwrap();

        let ids: Vec<i64> = results.iter().map(|m| m.id).collect();
        bulk_delete_media_async(&exec, &ids).await.unwrap();

        for m in &results {
            let got = get_media_async(&exec, m.id).await.unwrap();
            assert!(got.is_none(), "id={} should be deleted", m.id);
        }
    }

    #[tokio::test]
    async fn test_bulk_update_media_async() {
        use crate::types::MediaUpdateInput;

        let exec = setup_async();
        let results = bulk_create_media_async(
            &exec,
            &[
                MediaInput {
                    title: "元1".to_string(),
                    media_type: MediaType::Comic,
                    ..Default::default()
                },
                MediaInput {
                    title: "元2".to_string(),
                    media_type: MediaType::Video,
                    ..Default::default()
                },
            ],
        )
        .await
        .unwrap();

        let updates = vec![
            BulkUpdateItem {
                id: results[0].id,
                data: MediaUpdateInput {
                    title: Some("更新1".to_string()),
                    ..Default::default()
                },
            },
            BulkUpdateItem {
                id: results[1].id,
                data: MediaUpdateInput {
                    artist: Some(Some("作家2".to_string())),
                    ..Default::default()
                },
            },
        ];
        bulk_update_media_async(&exec, &updates).await.unwrap();

        let m1 = get_media_async(&exec, results[0].id).await.unwrap().unwrap();
        assert_eq!(m1.title, "更新1");
        assert_eq!(m1.media_type, MediaType::Comic); // 未指定はそのまま

        let m2 = get_media_async(&exec, results[1].id).await.unwrap().unwrap();
        assert_eq!(m2.artist, Some("作家2".to_string()));
        assert_eq!(m2.title, "元2"); // 未指定はそのまま
    }

    #[tokio::test]
    async fn test_bulk_create_chunking_async() {
        // DEFAULT_MAX_BATCH_SIZE(500) を超えても正常に作成できる
        let exec = setup_async();
        let data_list: Vec<MediaInput> = (0..600)
            .map(|i| MediaInput {
                title: format!("m{}", i),
                media_type: MediaType::Comic,
                ..Default::default()
            })
            .collect();

        let results = bulk_create_media_async(&exec, &data_list).await.unwrap();
        assert_eq!(results.len(), 600);
    }

    #[test]
    fn test_bulk_create_media() {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        let data_list = vec![
            MediaInput {
                title: "メディア1".to_string(),
                media_type: MediaType::Comic,
                ..Default::default()
            },
            MediaInput {
                title: "メディア2".to_string(),
                media_type: MediaType::Video,
                ..Default::default()
            },
            MediaInput {
                title: "メディア3".to_string(),
                media_type: MediaType::Music,
                ..Default::default()
            },
        ];

        let results = bulk_create_media(&conn, &data_list).unwrap();
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].title, "メディア1");
        assert_eq!(results[1].title, "メディア2");
        assert_eq!(results[2].title, "メディア3");
    }

    #[test]
    fn test_bulk_create_empty() {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        let results = bulk_create_media(&conn, &[]).unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_bulk_delete_media() {
        use crate::crud::get_media;

        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        // テスト用のメディアを作成
        let data_list = vec![
            MediaInput {
                title: "メディア1".to_string(),
                media_type: MediaType::Comic,
                ..Default::default()
            },
            MediaInput {
                title: "メディア2".to_string(),
                media_type: MediaType::Video,
                ..Default::default()
            },
            MediaInput {
                title: "メディア3".to_string(),
                media_type: MediaType::Music,
                ..Default::default()
            },
        ];

        let results = bulk_create_media(&conn, &data_list).unwrap();
        assert_eq!(results.len(), 3);

        // 最初の2つを一括削除
        let ids_to_delete = vec![results[0].id, results[1].id];
        bulk_delete_media(&conn, &ids_to_delete).unwrap();

        // 削除されたことを確認
        assert!(get_media(&conn, results[0].id).is_none());
        assert!(get_media(&conn, results[1].id).is_none());
        assert!(get_media(&conn, results[2].id).is_some());
    }

    #[test]
    fn test_bulk_delete_empty() {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        // 空のリストで削除してもエラーにならない
        bulk_delete_media(&conn, &[]).unwrap();
    }

    #[test]
    fn test_bulk_update_media() {
        use crate::crud::get_media;
        use crate::types::MediaUpdateInput;

        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        // テスト用のメディアを作成
        let data_list = vec![
            MediaInput {
                title: "メディア1".to_string(),
                media_type: MediaType::Comic,
                ..Default::default()
            },
            MediaInput {
                title: "メディア2".to_string(),
                media_type: MediaType::Video,
                ..Default::default()
            },
        ];

        let results = bulk_create_media(&conn, &data_list).unwrap();
        assert_eq!(results.len(), 2);

        // 一括更新（部分更新: 指定したフィールドのみ更新）
        // Option<Option<T>>パターン: Some(Some(val))で値を設定
        let updates = vec![
            BulkUpdateItem {
                id: results[0].id,
                data: MediaUpdateInput {
                    title: Some("更新後メディア1".to_string()),
                    artist: Some(Some("アーティスト1".to_string())),
                    ..Default::default()
                },
            },
            BulkUpdateItem {
                id: results[1].id,
                data: MediaUpdateInput {
                    title: Some("更新後メディア2".to_string()),
                    artist: Some(Some("アーティスト2".to_string())),
                    ..Default::default()
                },
            },
        ];
        bulk_update_media(&conn, &updates).unwrap();

        // 更新されたことを確認
        let updated1 = get_media(&conn, results[0].id).unwrap();
        assert_eq!(updated1.title, "更新後メディア1");
        assert_eq!(updated1.artist, Some("アーティスト1".to_string()));
        // media_typeは元のまま
        assert_eq!(updated1.media_type, MediaType::Comic);

        let updated2 = get_media(&conn, results[1].id).unwrap();
        assert_eq!(updated2.title, "更新後メディア2");
        assert_eq!(updated2.artist, Some("アーティスト2".to_string()));
        // media_typeは元のまま
        assert_eq!(updated2.media_type, MediaType::Video);
    }

    #[test]
    fn test_bulk_update_empty() {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        // 空のリストで更新してもエラーにならない
        bulk_update_media(&conn, &[]).unwrap();
    }

    #[test]
    fn test_bulk_delete_rollback_on_failure() {
        use crate::crud::get_media;

        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        // テスト用のメディアを作成
        let data_list = vec![
            MediaInput {
                title: "メディア1".to_string(),
                media_type: MediaType::Comic,
                ..Default::default()
            },
            MediaInput {
                title: "メディア2".to_string(),
                media_type: MediaType::Video,
                ..Default::default()
            },
        ];

        let results = bulk_create_media(&conn, &data_list).unwrap();
        assert_eq!(results.len(), 2);

        let id1 = results[0].id;
        let id2 = results[1].id;

        // 存在しないIDを含めて削除を試みる
        // 2番目は存在するが、3番目（9999）は存在しない
        let ids_to_delete = vec![id1, 9999];
        let result = bulk_delete_media(&conn, &ids_to_delete);

        // エラーが発生するはず
        assert!(result.is_err());

        // ロールバックされているので、id1も削除されていない
        assert!(get_media(&conn, id1).is_some());
        assert!(get_media(&conn, id2).is_some());
    }

    #[test]
    fn test_bulk_update_rollback_on_failure() {
        use crate::crud::get_media;
        use crate::types::MediaUpdateInput;

        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        // テスト用のメディアを作成
        let data_list = vec![MediaInput {
            title: "元のタイトル".to_string(),
            media_type: MediaType::Comic,
            ..Default::default()
        }];

        let results = bulk_create_media(&conn, &data_list).unwrap();
        let id1 = results[0].id;

        // 1番目は有効、2番目は存在しないIDへの更新
        let updates = vec![
            BulkUpdateItem {
                id: id1,
                data: MediaUpdateInput {
                    title: Some("更新後タイトル".to_string()),
                    ..Default::default()
                },
            },
            BulkUpdateItem {
                id: 9999, // 存在しないID
                data: MediaUpdateInput {
                    title: Some("存在しない".to_string()),
                    ..Default::default()
                },
            },
        ];

        let result = bulk_update_media(&conn, &updates);

        // エラーが発生するはず
        assert!(result.is_err());

        // ロールバックされているので、id1の更新も取り消されている
        let media1 = get_media(&conn, id1).unwrap();
        assert_eq!(media1.title, "元のタイトル");
    }
}
