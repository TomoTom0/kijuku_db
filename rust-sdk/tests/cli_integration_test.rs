use serde_json::{json, Value};
use std::process::{Command, Stdio};
use std::io::Write;
use tempfile::NamedTempFile;

/// CLIにJSONコマンドを送信し、レスポンスを取得する
fn execute_cli_command(db_path: &str, command: Value) -> Value {
    let mut child = Command::new("cargo")
        .args(&["run", "--bin", "kijuku-cli", "--", "--db", db_path])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("CLIの起動に失敗");

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
    assert_eq!(response["data"]["migrated"], true);
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
    assert_eq!(response["data"]["version"], 4);
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

    // 存在しないメディアを取得
    let response = execute_cli_command(db_path, json!({
        "operation": "getMedia",
        "params": {
            "id": 9999
        }
    }));

    assert_eq!(response["success"], false);
    assert!(response["error"].as_str().unwrap().contains("見つかりません"));
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
