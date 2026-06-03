use serde_json::{json, Value};
use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};
use tempfile::{NamedTempFile, TempDir};

/// CLIにJSONコマンドを送信し、レスポンスを取得する
fn execute_cli_command(db_path: &str, command: Value) -> Value {
    execute_cli_command_with_env(db_path, command, &[])
}

/// CLIにJSONコマンドを送信し、レスポンスを取得する（環境変数追加版）
fn execute_cli_command_with_env(db_path: &str, command: Value, env: &[(&str, &str)]) -> Value {
    let mut cmd = Command::new("cargo");
    cmd.args(&["run", "--bin", "kijuku-cli", "--", "--db", db_path])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, val) in env {
        cmd.env(key, val);
    }
    let mut child = cmd.spawn().expect("CLIの起動に失敗");

    // 標準入力にJSONコマンドを書き込む
    let command_str = serde_json::to_string(&command).unwrap();
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(command_str.as_bytes()).unwrap();
    }

    // 結果を取得
    let output = child.wait_with_output().expect("CLIの実行に失敗");
    let stdout = String::from_utf8_lossy(&output.stdout);

    // JSONパース
    serde_json::from_str(&stdout).expect(&format!("JSONパースに失敗: {}", stdout))
}

#[test]
fn test_cli_migrate() {
    let temp_file = NamedTempFile::new().unwrap();
    let db_path = temp_file.path().to_str().unwrap();

    let command = json!({
        "operation": "migrate",
        "params": {}
    });

    let response = execute_cli_command(db_path, command);

    assert_eq!(response["success"], true);
    assert_eq!(response["data"]["ok"], true);
}

#[test]
fn test_cli_get_schema_version() {
    let temp_file = NamedTempFile::new().unwrap();
    let db_path = temp_file.path().to_str().unwrap();

    // まずマイグレーション
    execute_cli_command(db_path, json!({
        "operation": "migrate",
        "params": {}
    }));

    // スキーマバージョン取得
    let command = json!({
        "operation": "getSchemaVersion",
        "params": {}
    });

    let response = execute_cli_command(db_path, command);

    assert_eq!(response["success"], true);
    assert_eq!(response["data"]["version"], 6);
}

#[test]
fn test_cli_create_media() {
    let temp_file = NamedTempFile::new().unwrap();
    let db_path = temp_file.path().to_str().unwrap();

    // マイグレーション
    execute_cli_command(db_path, json!({
        "operation": "migrate",
        "params": {}
    }));

    // メディア作成
    let command = json!({
        "operation": "createMedia",
        "params": {
            "data": {
                "title": "テストコミック",
                "media_type": "comic",
                "artist": "テスト作者"
            }
        }
    });

    let response = execute_cli_command(db_path, command);

    assert_eq!(response["success"], true);
    assert_eq!(response["data"]["title"], "テストコミック");
    assert_eq!(response["data"]["artist"], "テスト作者");
}

#[test]
fn test_cli_get_media() {
    let temp_file = NamedTempFile::new().unwrap();
    let db_path = temp_file.path().to_str().unwrap();

    // マイグレーション
    execute_cli_command(db_path, json!({
        "operation": "migrate",
        "params": {}
    }));

    // メディア作成
    let create_response = execute_cli_command(db_path, json!({
        "operation": "createMedia",
        "params": {
            "data": {
                "title": "テストコミック",
                "media_type": "comic"
            }
        }
    }));
    let media_id = create_response["data"]["id"].as_i64().unwrap();

    // メディア取得
    let get_response = execute_cli_command(db_path, json!({
        "operation": "getMedia",
        "params": {
            "id": media_id
        }
    }));

    assert_eq!(get_response["success"], true);
    assert_eq!(get_response["data"]["id"], media_id);
    assert_eq!(get_response["data"]["title"], "テストコミック");
}

#[test]
fn test_cli_find_media() {
    let temp_file = NamedTempFile::new().unwrap();
    let db_path = temp_file.path().to_str().unwrap();

    // マイグレーション
    execute_cli_command(db_path, json!({
        "operation": "migrate",
        "params": {}
    }));

    // メディアを複数作成
    execute_cli_command(db_path, json!({
        "operation": "createMedia",
        "params": {
            "data": {
                "title": "コミック1",
                "media_type": "comic"
            }
        }
    }));

    execute_cli_command(db_path, json!({
        "operation": "createMedia",
        "params": {
            "data": {
                "title": "ビデオ1",
                "media_type": "video"
            }
        }
    }));

    // メディア検索（comicのみ）
    let find_response = execute_cli_command(db_path, json!({
        "operation": "findMedia",
        "params": {
            "filter": {
                "media_type": "comic"
            },
            "options": null
        }
    }));

    assert_eq!(find_response["success"], true);
    assert_eq!(find_response["data"].as_array().unwrap().len(), 1);
    assert_eq!(find_response["data"][0]["title"], "コミック1");
}

#[test]
fn test_cli_create_and_get_tag() {
    let temp_file = NamedTempFile::new().unwrap();
    let db_path = temp_file.path().to_str().unwrap();

    // マイグレーション
    execute_cli_command(db_path, json!({
        "operation": "migrate",
        "params": {}
    }));

    // タグ作成
    let create_response = execute_cli_command(db_path, json!({
        "operation": "createTag",
        "params": {
            "name": "アクション"
        }
    }));

    assert_eq!(create_response["success"], true);
    assert_eq!(create_response["data"]["name"], "アクション");

    // タグ取得
    let get_response = execute_cli_command(db_path, json!({
        "operation": "getTagByName",
        "params": {
            "name": "アクション"
        }
    }));

    assert_eq!(get_response["success"], true);
    assert_eq!(get_response["data"]["name"], "アクション");
}

#[test]
fn test_cli_error_handling() {
    let temp_file = NamedTempFile::new().unwrap();
    let db_path = temp_file.path().to_str().unwrap();

    // マイグレーション
    execute_cli_command(db_path, json!({
        "operation": "migrate",
        "params": {}
    }));

    // 存在しないメディアを取得（null が返る）
    let response = execute_cli_command(db_path, json!({
        "operation": "getMedia",
        "params": {
            "id": 9999
        }
    }));

    assert_eq!(response["success"], true);
    assert!(response["data"].is_null());
}

#[test]
fn test_cli_backup() {
    let temp_file = NamedTempFile::new().unwrap();
    let db_path = temp_file.path().to_str().unwrap();

    execute_cli_command(db_path, json!({"operation": "migrate", "params": {}}));

    let response = execute_cli_command(db_path, json!({
        "operation": "backup",
        "params": {}
    }));

    assert_eq!(response["success"], true);
    assert!(response["data"]["path"].as_str().is_some());
}

#[test]
fn test_cli_backup_with_label() {
    let temp_file = NamedTempFile::new().unwrap();
    let db_path = temp_file.path().to_str().unwrap();

    execute_cli_command(db_path, json!({"operation": "migrate", "params": {}}));

    let response = execute_cli_command(db_path, json!({
        "operation": "backup",
        "params": { "label": "テストバックアップ" }
    }));

    assert_eq!(response["success"], true);
    assert!(response["data"]["path"].as_str().is_some());
}

#[test]
fn test_cli_list_backups() {
    let temp_file = NamedTempFile::new().unwrap();
    let db_path = temp_file.path().to_str().unwrap();

    execute_cli_command(db_path, json!({"operation": "migrate", "params": {}}));
    execute_cli_command(db_path, json!({"operation": "backup", "params": {}}));

    let response = execute_cli_command(db_path, json!({
        "operation": "listBackups",
        "params": {}
    }));

    assert_eq!(response["success"], true);
    let backups = response["data"].as_array().unwrap();
    assert_eq!(backups.len(), 1);
    assert!(backups[0]["id"].as_str().is_some());
    assert!(backups[0]["path"].as_str().is_some());
    assert_eq!(backups[0]["scope"], "manual");
}

#[test]
fn test_cli_restore() {
    let temp_file = NamedTempFile::new().unwrap();
    let db_path = temp_file.path().to_str().unwrap();

    execute_cli_command(db_path, json!({"operation": "migrate", "params": {}}));

    // メディアを作成してからバックアップ
    let create_response = execute_cli_command(db_path, json!({
        "operation": "createMedia",
        "params": { "data": { "title": "復元テスト作品", "media_type": "comic" } }
    }));
    let media_id = create_response["data"]["id"].as_i64().unwrap();

    // ラベル付きで手動バックアップを作成
    execute_cli_command(db_path, json!({
        "operation": "backup",
        "params": { "label": "pre-delete" }
    }));

    // メディアを削除
    execute_cli_command(db_path, json!({
        "operation": "deleteMedia",
        "params": { "id": media_id }
    }));

    // バックアップ一覧を取得して手動バックアップを探す
    let list_response = execute_cli_command(db_path, json!({
        "operation": "listBackups",
        "params": {}
    }));
    let backups = list_response["data"].as_array().unwrap();
    let backup_index = backups.iter().position(|b| {
        b["label"].as_str() == Some("pre-delete")
    }).unwrap();

    // 特定バックアップから復元
    let restore_response = execute_cli_command(db_path, json!({
        "operation": "restore",
        "params": { "selector": { "type": "nth", "n": backup_index } }
    }));

    assert_eq!(restore_response["success"], true);
    assert!(restore_response["data"]["path"].as_str().is_some());

    // 復元後にメディアが存在することを確認
    let get_response = execute_cli_command(db_path, json!({
        "operation": "getMedia",
        "params": { "id": media_id }
    }));
    assert_eq!(get_response["success"], true);
    assert_eq!(get_response["data"]["title"], "復元テスト作品");
}

#[test]
fn test_cli_check_thumbnail_skipped_no_path() {
    let temp_file = NamedTempFile::new().unwrap();
    let db_path = temp_file.path().to_str().unwrap();

    execute_cli_command(db_path, json!({"operation": "migrate", "params": {}}));

    // pathなしのメディアを作成
    execute_cli_command(db_path, json!({
        "operation": "createMedia",
        "params": { "data": { "title": "pathなし", "media_type": "comic" } }
    }));

    let response = execute_cli_command(db_path, json!({
        "operation": "checkThumbnail",
        "params": {}
    }));

    assert_eq!(response["success"], true);
    assert_eq!(response["data"]["total"], 1);
    assert_eq!(response["data"]["skipped"], 1);
    assert_eq!(response["data"]["ok"], 0);
    assert_eq!(response["data"]["missing"], 0);
    assert!(response["data"]["details"].is_array());
}

#[test]
fn test_cli_check_thumbnail_missing() {
    let temp_file = NamedTempFile::new().unwrap();
    let db_path = temp_file.path().to_str().unwrap();

    execute_cli_command(db_path, json!({"operation": "migrate", "params": {}}));

    // contentありのpathを持つメディア（thumbnail_pathなし）
    execute_cli_command(db_path, json!({
        "operation": "createMedia",
        "params": {
            "data": {
                "title": "サムネなし",
                "media_type": "comic",
                "path": "/media/onepiece/vol1/content",
                "uuid": "test-uuid-missing-thumb"
            }
        }
    }));

    let response = execute_cli_command(db_path, json!({
        "operation": "checkThumbnail",
        "params": {}
    }));

    assert_eq!(response["success"], true);
    assert_eq!(response["data"]["total"], 1);
    assert_eq!(response["data"]["missing"], 1);
    assert_eq!(response["data"]["skipped"], 0);
}

#[test]
fn test_cli_check_thumbnail_with_filter() {
    let temp_file = NamedTempFile::new().unwrap();
    let db_path = temp_file.path().to_str().unwrap();

    execute_cli_command(db_path, json!({"operation": "migrate", "params": {}}));

    // comicとvideo各1件を作成
    execute_cli_command(db_path, json!({
        "operation": "createMedia",
        "params": {
            "data": {
                "title": "コミック",
                "media_type": "comic",
                "path": "/media/onepiece/content",
                "uuid": "uuid-comic-filter"
            }
        }
    }));
    execute_cli_command(db_path, json!({
        "operation": "createMedia",
        "params": { "data": { "title": "ビデオ", "media_type": "video" } }
    }));

    // comicのみを対象にフィルタ
    let response = execute_cli_command(db_path, json!({
        "operation": "checkThumbnail",
        "params": { "filter": { "media_type": "comic" } }
    }));

    assert_eq!(response["success"], true);
    assert_eq!(response["data"]["total"], 1);
}

#[test]
fn test_cli_update_thumbnail_skip_no_content() {
    let temp_file = NamedTempFile::new().unwrap();
    let db_path = temp_file.path().to_str().unwrap();

    execute_cli_command(db_path, json!({"operation": "migrate", "params": {}}));

    // contentなしのpathを持つメディア
    execute_cli_command(db_path, json!({
        "operation": "createMedia",
        "params": {
            "data": {
                "title": "contentなし",
                "media_type": "comic",
                "path": "/media/onepiece/vol1",
                "uuid": "test-uuid-no-content-cli"
            }
        }
    }));

    let response = execute_cli_command(db_path, json!({
        "operation": "updateThumbnail",
        "params": {}
    }));

    assert_eq!(response["success"], true);
    assert_eq!(response["data"]["total"], 1);
    assert_eq!(response["data"]["skipped"], 1);
    assert_eq!(response["data"]["generated"], 0);
}

#[test]
fn test_cli_update_thumbnail_dry_run_no_db_update() {
    let temp_file = NamedTempFile::new().unwrap();
    let db_path = temp_file.path().to_str().unwrap();
    let tmp_dir = TempDir::new().unwrap();

    execute_cli_command(db_path, json!({"operation": "migrate", "params": {}}));

    // 001.jpgを持つcontentディレクトリを用意
    let content_dir = tmp_dir.path().join("content");
    fs::create_dir(&content_dir).unwrap();
    fs::write(content_dir.join("001.jpg"), b"dummy").unwrap();

    let create_response = execute_cli_command(db_path, json!({
        "operation": "createMedia",
        "params": {
            "data": {
                "title": "dry_runテスト",
                "media_type": "comic",
                "path": content_dir.to_str().unwrap(),
                "uuid": "test-uuid-dryrun-cli",
                "extension": "jpg"
            }
        }
    }));
    let media_id = create_response["data"]["id"].as_i64().unwrap();

    let response = execute_cli_command(db_path, json!({
        "operation": "updateThumbnail",
        "params": { "thumbnail_options": { "dry_run": true, "force": false } }
    }));

    assert_eq!(response["success"], true);
    assert_eq!(response["data"]["generated"], 1);

    // DBのthumbnail_pathは更新されていないことを確認
    let get_response = execute_cli_command(db_path, json!({
        "operation": "getMedia",
        "params": { "id": media_id }
    }));
    assert_eq!(get_response["success"], true);
    assert!(get_response["data"]["thumbnail_path"].is_null());

    // coverディレクトリも作成されていないことを確認
    assert!(!tmp_dir.path().join("cover").exists());
}

#[test]
fn test_cli_update_thumbnail_success_generates_and_updates_db() {
    let temp_file = NamedTempFile::new().unwrap();
    let db_path = temp_file.path().to_str().unwrap();
    let tmp_dir = TempDir::new().unwrap();

    // fake convertスクリプトを作成
    let fake_bin_dir = TempDir::new().unwrap();
    let fake_convert = fake_bin_dir.path().join("convert");
    fs::write(
        &fake_convert,
        "#!/bin/sh\nfor last; do true; done\ntouch \"$last\"\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&fake_convert, fs::Permissions::from_mode(0o755)).unwrap();
    }

    let original_path = std::env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", fake_bin_dir.path().display(), original_path);

    execute_cli_command(db_path, json!({"operation": "migrate", "params": {}}));

    // 001.jpgを持つcontentディレクトリを用意
    let content_dir = tmp_dir.path().join("content");
    fs::create_dir(&content_dir).unwrap();
    fs::write(content_dir.join("001.jpg"), b"dummy").unwrap();

    let uuid = "test-uuid-success-cli";
    let create_response = execute_cli_command(db_path, json!({
        "operation": "createMedia",
        "params": {
            "data": {
                "title": "成功ケース",
                "media_type": "comic",
                "path": content_dir.to_str().unwrap(),
                "uuid": uuid,
                "extension": "jpg"
            }
        }
    }));
    let media_id = create_response["data"]["id"].as_i64().unwrap();

    let response = execute_cli_command_with_env(
        db_path,
        json!({
            "operation": "updateThumbnail",
            "params": { "thumbnail_options": { "dry_run": false, "force": false } }
        }),
        &[("PATH", new_path.as_str())],
    );

    assert_eq!(response["success"], true);
    assert_eq!(response["data"]["generated"], 1);
    assert_eq!(response["data"]["errors"], 0);

    // DBにthumbnail_pathが保存されている
    let get_response = execute_cli_command(db_path, json!({
        "operation": "getMedia",
        "params": { "id": media_id }
    }));
    assert_eq!(get_response["success"], true);
    assert!(get_response["data"]["thumbnail_path"].as_str().is_some());

    // coverディレクトリが作成されている
    assert!(tmp_dir.path().join("cover").exists());
}

#[test]
fn test_cli_bulk_create() {
    let temp_file = NamedTempFile::new().unwrap();
    let db_path = temp_file.path().to_str().unwrap();

    // マイグレーション
    execute_cli_command(db_path, json!({
        "operation": "migrate",
        "params": {}
    }));

    // 一括作成
    let response = execute_cli_command(db_path, json!({
        "operation": "bulkCreateMedia",
        "params": {
            "data_list": [
                {
                    "title": "バルク1",
                    "media_type": "comic"
                },
                {
                    "title": "バルク2",
                    "media_type": "video"
                }
            ]
        }
    }));

    assert_eq!(response["success"], true);
    assert_eq!(response["data"].as_array().unwrap().len(), 2);
}

#[test]
fn test_cli_get_distinct_values_single_field() {
    let temp_file = NamedTempFile::new().unwrap();
    let db_path = temp_file.path().to_str().unwrap();

    execute_cli_command(db_path, json!({ "operation": "migrate", "params": {} }));

    execute_cli_command(db_path, json!({ "operation": "createMedia", "params": { "data": { "title": "作品1", "media_type": "comic", "artist": "Author1" } } }));
    execute_cli_command(db_path, json!({ "operation": "createMedia", "params": { "data": { "title": "作品2", "media_type": "comic", "artist": "Author2" } } }));
    execute_cli_command(db_path, json!({ "operation": "createMedia", "params": { "data": { "title": "作品3", "media_type": "video", "artist": "Author1" } } }));

    let response = execute_cli_command(db_path, json!({
        "operation": "getDistinctValues",
        "params": {
            "fields": ["artist"],
            "filter": {}
        }
    }));

    assert_eq!(response["success"], true);
    let rows = response["data"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0][0], "Author1");
    assert_eq!(rows[1][0], "Author2");
}

#[test]
fn test_cli_get_distinct_values_with_filter() {
    let temp_file = NamedTempFile::new().unwrap();
    let db_path = temp_file.path().to_str().unwrap();

    execute_cli_command(db_path, json!({ "operation": "migrate", "params": {} }));

    execute_cli_command(db_path, json!({ "operation": "createMedia", "params": { "data": { "title": "コミック1", "media_type": "comic", "artist": "AuthorA" } } }));
    execute_cli_command(db_path, json!({ "operation": "createMedia", "params": { "data": { "title": "コミック2", "media_type": "comic", "artist": "AuthorB" } } }));
    execute_cli_command(db_path, json!({ "operation": "createMedia", "params": { "data": { "title": "動画1", "media_type": "video", "artist": "AuthorC" } } }));

    let response = execute_cli_command(db_path, json!({
        "operation": "getDistinctValues",
        "params": {
            "fields": ["artist"],
            "filter": { "media_type": "comic" }
        }
    }));

    assert_eq!(response["success"], true);
    let rows = response["data"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    let artists: Vec<&str> = rows.iter().map(|r| r[0].as_str().unwrap()).collect();
    assert!(artists.contains(&"AuthorA"));
    assert!(artists.contains(&"AuthorB"));
    assert!(!artists.contains(&"AuthorC"));
}

#[test]
fn test_cli_get_distinct_values_invalid_field() {
    let temp_file = NamedTempFile::new().unwrap();
    let db_path = temp_file.path().to_str().unwrap();

    execute_cli_command(db_path, json!({ "operation": "migrate", "params": {} }));

    let response = execute_cli_command(db_path, json!({
        "operation": "getDistinctValues",
        "params": {
            "fields": ["id"],
            "filter": {}
        }
    }));

    assert_eq!(response["success"], false);
}
