use serde_json::{json, Value};
use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};
use tempfile::TempDir;

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

/// CLI に JSON コマンドを送信（`--media-root` + 環境変数付き）。FS 操作の prod 直接検証用。
fn execute_cli_command_full(
    db_path: &str,
    media_root: Option<&str>,
    command: Value,
    env: &[(&str, &str)],
) -> Value {
    let mut cmd = Command::new("cargo");
    cmd.args(&["run", "--bin", "kijuku-cli", "--", "--db", db_path]);
    if let Some(mr) = media_root {
        cmd.args(&["--media-root", mr]);
    }
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, val) in env {
        cmd.env(key, val);
    }
    let mut child = cmd.spawn().expect("CLIの起動に失敗");
    let command_str = serde_json::to_string(&command).unwrap();
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(command_str.as_bytes()).unwrap();
    }
    let output = child.wait_with_output().expect("CLIの実行に失敗");
    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(&stdout).expect(&format!("JSONパースに失敗: {}", stdout))
}

/// CLIサブコマンド（file/trash等）を --media-root 付きで実行し、1行JSONレスポンスを取得する。
fn execute_cli_subcommand(db_path: &str, media_root: &str, sub_args: &[&str]) -> Value {
    let output = Command::new("cargo")
        .args(["run", "--bin", "kijuku-cli", "--", "--db", db_path, "--media-root", media_root])
        .args(sub_args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("CLIの起動に失敗");
    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(&stdout).expect(&format!("JSONパースに失敗: {}", stdout))
}

/// DB 専用サブコマンド（`--media-root` 不要・JSON を返さず人間向けメッセージを出すもの）を
/// 実行し、stdout 文字列を返す。`sync-db` 等の検証用。
fn execute_cli_db_subcommand(sub_args: &[&str]) -> String {
    let output = Command::new("cargo")
        .args(["run", "--bin", "kijuku-cli", "--"])
        .args(sub_args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("CLIの起動に失敗");
    String::from_utf8_lossy(&output.stdout).to_string()
}

#[test]
fn test_cli_migrate() {
    // db_path を TempDir 内に置くことで backup_dir（db_path の親/backup）も各テスト独立となり、
    // 並列実行時の /tmp/backup 共有競合を防ぐ（NamedTempFile は /tmp 直下になるため共有される）。
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db");
    let db_path = db_path.to_str().unwrap();

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
    // db_path を TempDir 内に置くことで backup_dir（db_path の親/backup）も各テスト独立となり、
    // 並列実行時の /tmp/backup 共有競合を防ぐ（NamedTempFile は /tmp 直下になるため共有される）。
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db");
    let db_path = db_path.to_str().unwrap();

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
fn test_cli_get_server_version() {
    // getServerVersion はリモート自動デプロイのバージョン比較用（TASK-69）。DB アクセス不要だが
    // build_backend が DB を開くため migrate 後に呼ぶ。
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db");
    let db_path = db_path.to_str().unwrap();

    execute_cli_command(db_path, json!({
        "operation": "migrate",
        "params": {}
    }));

    let response = execute_cli_command(
        db_path,
        json!({
            "operation": "getServerVersion",
            "params": {}
        }),
    );

    assert_eq!(response["success"], true);
    assert_eq!(response["data"]["version"], env!("CARGO_PKG_VERSION"));
}

#[test]
fn test_cli_create_media() {
    // db_path を TempDir 内に置くことで backup_dir（db_path の親/backup）も各テスト独立となり、
    // 並列実行時の /tmp/backup 共有競合を防ぐ（NamedTempFile は /tmp 直下になるため共有される）。
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db");
    let db_path = db_path.to_str().unwrap();

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
    // db_path を TempDir 内に置くことで backup_dir（db_path の親/backup）も各テスト独立となり、
    // 並列実行時の /tmp/backup 共有競合を防ぐ（NamedTempFile は /tmp 直下になるため共有される）。
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db");
    let db_path = db_path.to_str().unwrap();

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
    // db_path を TempDir 内に置くことで backup_dir（db_path の親/backup）も各テスト独立となり、
    // 並列実行時の /tmp/backup 共有競合を防ぐ（NamedTempFile は /tmp 直下になるため共有される）。
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db");
    let db_path = db_path.to_str().unwrap();

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
    // db_path を TempDir 内に置くことで backup_dir（db_path の親/backup）も各テスト独立となり、
    // 並列実行時の /tmp/backup 共有競合を防ぐ（NamedTempFile は /tmp 直下になるため共有される）。
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db");
    let db_path = db_path.to_str().unwrap();

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
    // db_path を TempDir 内に置くことで backup_dir（db_path の親/backup）も各テスト独立となり、
    // 並列実行時の /tmp/backup 共有競合を防ぐ（NamedTempFile は /tmp 直下になるため共有される）。
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db");
    let db_path = db_path.to_str().unwrap();

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
    // db_path を TempDir 内に置くことで backup_dir（db_path の親/backup）も各テスト独立となり、
    // 並列実行時の /tmp/backup 共有競合を防ぐ（NamedTempFile は /tmp 直下になるため共有される）。
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db");
    let db_path = db_path.to_str().unwrap();

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
    // db_path を TempDir 内に置くことで backup_dir（db_path の親/backup）も各テスト独立となり、
    // 並列実行時の /tmp/backup 共有競合を防ぐ（NamedTempFile は /tmp 直下になるため共有される）。
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db");
    let db_path = db_path.to_str().unwrap();

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
    // db_path を TempDir 内に置くことで backup_dir（db_path の親/backup）も各テスト独立となり、
    // 並列実行時の /tmp/backup 共有競合を防ぐ（NamedTempFile は /tmp 直下になるため共有される）。
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db");
    let db_path = db_path.to_str().unwrap();

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
fn test_cli_diff_backup() {
    // db_path を TempDir 内に置くことで backup_dir（db_path の親/backup）も各テスト独立となり、
    // 並列実行時の /tmp/backup 共有競合を防ぐ（NamedTempFile は /tmp 直下になるため共有される）。
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db");
    let db_path = db_path.to_str().unwrap();

    execute_cli_command(db_path, json!({"operation": "migrate", "params": {}}));
    execute_cli_command(db_path, json!({"operation": "backup", "params": {}}));

    // backup直後は差分なし
    let response = execute_cli_command(db_path, json!({
        "operation": "diffBackup",
        "params": {}
    }));
    assert_eq!(response["success"], true);
    let media = &response["data"]["summary"]["media"];
    assert_eq!(media["added"], 0);
    assert_eq!(media["removed"], 0);
    assert_eq!(media["changed"], 0);
}

#[test]
fn test_cli_set_backup_label_and_note() {
    // db_path を TempDir 内に置くことで backup_dir（db_path の親/backup）も各テスト独立となり、
    // 並列実行時の /tmp/backup 共有競合を防ぐ（NamedTempFile は /tmp 直下になるため共有される）。
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db");
    let db_path = db_path.to_str().unwrap();

    execute_cli_command(db_path, json!({"operation": "migrate", "params": {}}));
    execute_cli_command(db_path, json!({"operation": "backup", "params": {}}));

    let list = execute_cli_command(db_path, json!({"operation": "listBackups", "params": {}}));
    let id = list["data"][0]["id"].as_str().unwrap();

    // ラベル・メモ付与
    let r1 = execute_cli_command(db_path, json!({
        "operation": "setBackupLabel",
        "params": { "id": id, "label": "重要" }
    }));
    assert_eq!(r1["success"], true);

    let r2 = execute_cli_command(db_path, json!({
        "operation": "setBackupNote",
        "params": { "id": id, "note": "作業前の状態" }
    }));
    assert_eq!(r2["success"], true);

    // listBackups で label/note/labelSource を確認（サイドカー優先マージ）。
    // /tmp/backup は並列テストで共有されるため、id で自分のbackupを特定する。
    let list2 = execute_cli_command(db_path, json!({"operation": "listBackups", "params": {}}));
    let entry = list2["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["id"] == id)
        .expect("自分のbackupが見つからない");
    assert_eq!(entry["label"], "重要");
    assert_eq!(entry["note"], "作業前の状態");
    assert_eq!(entry["labelSource"], "sidecar");

    // getBackupMeta で取得
    let meta = execute_cli_command(db_path, json!({
        "operation": "getBackupMeta",
        "params": { "id": id }
    }));
    assert_eq!(meta["data"]["label"], "重要");
    assert_eq!(meta["data"]["note"], "作業前の状態");
}

#[test]
fn test_cli_restore() {
    // db_path を TempDir 内に置くことで backup_dir（db_path の親/backup）も各テスト独立となり、
    // 並列実行時の /tmp/backup 共有競合を防ぐ（NamedTempFile は /tmp 直下になるため共有される）。
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db");
    let db_path = db_path.to_str().unwrap();

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

    // 特定バックアップから復元。restore は (b) 制限操作（設計 §9.2）なので prod 直接経路
    // （--target prod・ProdRwScope + pre-stash gate）で実行。
    let restore_response = execute_cli_command_with_env(
        db_path,
        json!({
            "operation": "restore",
            "params": { "selector": { "type": "nth", "n": backup_index } }
        }),
        &[("KIJUKU_TARGET", "prod")],
    );

    assert_eq!(restore_response["success"], true);
    assert!(restore_response["data"]["path"].as_str().is_some());

    // 復元後にメディアが存在することを確認（prod 読込・設計 §5.1）
    let get_response = execute_cli_command_with_env(
        db_path,
        json!({
            "operation": "getMedia",
            "params": { "id": media_id }
        }),
        &[("KIJUKU_TARGET", "prod")],
    );
    assert_eq!(get_response["success"], true);
    assert_eq!(get_response["data"]["title"], "復元テスト作品");
}

/// stg セッションで (b) 制限操作が事前ガードで拒否される（設計 §9.2・TASK-59 P2-C4）。
/// prod 直接 `--target prod` での gate 付き実行を案内するメッセージを返す。
#[test]
fn test_cli_stg_rejects_b_operation() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db");
    let db_path = db_path.to_str().unwrap();
    execute_cli_command(db_path, json!({"operation": "migrate", "params": {}}));

    // デフォルト stg で (b) 操作 → 拒否
    for op in &["mediaMv", "purgeTrash", "restore"] {
        let resp = execute_cli_command(db_path, json!({"operation": op, "params": {}}));
        assert_eq!(resp["success"], false, "{} は stg で拒否されるべき", op);
        let err = resp["error"].as_str().unwrap();
        assert!(err.contains("(b)"), "(b) 拒否メッセージのべき: {:?}", resp);
        assert!(err.contains("prod"), "prod 直接を案内するべき: {:?}", resp);
    }
}

/// prod 直接経路で (b) restore の dry-run が prod を変更せず差分を返す（設計 §9.2/§15-13・TASK-59）。
#[test]
fn test_cli_prod_b_restore_dry_run() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db");
    let db_path = db_path.to_str().unwrap();
    execute_cli_command(db_path, json!({"operation": "migrate", "params": {}}));

    // media A 作成 → バックアップ（1件時点）→ media B 追加（prod 現状 = 2件）
    execute_cli_command(db_path, json!({
        "operation": "createMedia",
        "params": { "data": { "title": "A", "media_type": "comic" } }
    }));
    execute_cli_command(db_path, json!({"operation": "backup", "params": {}}));
    let second = execute_cli_command(db_path, json!({
        "operation": "createMedia",
        "params": { "data": { "title": "B", "media_type": "comic" } }
    }));
    let second_id = second["data"]["id"].as_i64().unwrap();

    // prod 直接で restore dryRun（最新バックアップ＝1件時点）→ 差分返却・prod 不変
    let resp = execute_cli_command_with_env(
        db_path,
        json!({"operation": "restore", "params": {"dryRun": true}}),
        &[("KIJUKU_TARGET", "prod")],
    );
    assert_eq!(resp["success"], true, "dryRun は成功するべき: {:?}", resp);
    assert_eq!(resp["data"]["dryRun"], true);

    // prod は変更されていない（追加 media B が残る）
    let after = execute_cli_command_with_env(
        db_path,
        json!({"operation": "getMedia", "params": {"id": second_id}}),
        &[("KIJUKU_TARGET", "prod")],
    );
    assert_eq!(after["success"], true, "dryRun 後も prod は変更なしのべき: {:?}", after);

    // (b) restore dryRun が監査ログに記録される（operation=b-restore, result=dryRun・設計 §10）。
    let audit_resp = execute_cli_command_with_env(
        db_path,
        json!({"operation": "listAuditLogs", "params": {"operation": "b-restore"}}),
        &[("KIJUKU_TARGET", "prod")],
    );
    assert_eq!(audit_resp["success"], true, "listAuditLogs: {:?}", audit_resp);
    let logs = audit_resp["data"].as_array().expect("audit logs array");
    assert!(
        logs.iter().any(|r| r["result"] == "dryRun"),
        "b-restore dryRun audit record: {:?}",
        audit_resp
    );
}

/// prod 直接経路で (b) FS層操作（purgeTrash）の dry-run が受理され prod 不変（設計 §9.2/§15-13）。
/// stg では拒否される (b) 操作（test_cli_stg_rejects_b_operation）が prod 直接 dry-run 経路で保護付き実行される。
#[test]
fn test_cli_prod_b_purge_trash_dry_run() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db");
    let db_path = db_path.to_str().unwrap();
    let media_dir = TempDir::new().unwrap();
    let media_root = media_dir.path().to_str().unwrap();
    execute_cli_command(db_path, json!({"operation": "migrate", "params": {}}));

    // prod 直接で purgeTrash dryRun → (b) gate（ProdRwScope + pre-stash）を通って受理・prod 不変
    let resp = execute_cli_command_full(
        db_path,
        Some(media_root),
        json!({"operation": "purgeTrash", "params": {"dryRun": true}}),
        &[("KIJUKU_TARGET", "prod")],
    );
    assert_eq!(resp["success"], true, "prod 直接 dryRun は成功するべき: {:?}", resp);
    // dryRun は削除対象一覧（配列）を返す
    assert!(resp["data"].is_array(), "dryRun は対象一覧（配列）を返すべき: {:?}", resp);
}

#[test]
fn test_cli_check_thumbnail_skipped_no_path() {
    // db_path を TempDir 内に置くことで backup_dir（db_path の親/backup）も各テスト独立となり、
    // 並列実行時の /tmp/backup 共有競合を防ぐ（NamedTempFile は /tmp 直下になるため共有される）。
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db");
    let db_path = db_path.to_str().unwrap();

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
    // db_path を TempDir 内に置くことで backup_dir（db_path の親/backup）も各テスト独立となり、
    // 並列実行時の /tmp/backup 共有競合を防ぐ（NamedTempFile は /tmp 直下になるため共有される）。
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db");
    let db_path = db_path.to_str().unwrap();

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
    // db_path を TempDir 内に置くことで backup_dir（db_path の親/backup）も各テスト独立となり、
    // 並列実行時の /tmp/backup 共有競合を防ぐ（NamedTempFile は /tmp 直下になるため共有される）。
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db");
    let db_path = db_path.to_str().unwrap();

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
    // db_path を TempDir 内に置くことで backup_dir（db_path の親/backup）も各テスト独立となり、
    // 並列実行時の /tmp/backup 共有競合を防ぐ（NamedTempFile は /tmp 直下になるため共有される）。
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db");
    let db_path = db_path.to_str().unwrap();

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
    // db_path を TempDir 内に置くことで backup_dir（db_path の親/backup）も各テスト独立となり、
    // 並列実行時の /tmp/backup 共有競合を防ぐ（NamedTempFile は /tmp 直下になるため共有される）。
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db");
    let db_path = db_path.to_str().unwrap();
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
    // db_path を TempDir 内に置くことで backup_dir（db_path の親/backup）も各テスト独立となり、
    // 並列実行時の /tmp/backup 共有競合を防ぐ（NamedTempFile は /tmp 直下になるため共有される）。
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db");
    let db_path = db_path.to_str().unwrap();
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
    // db_path を TempDir 内に置くことで backup_dir（db_path の親/backup）も各テスト独立となり、
    // 並列実行時の /tmp/backup 共有競合を防ぐ（NamedTempFile は /tmp 直下になるため共有される）。
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db");
    let db_path = db_path.to_str().unwrap();

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
    // db_path を TempDir 内に置くことで backup_dir（db_path の親/backup）も各テスト独立となり、
    // 並列実行時の /tmp/backup 共有競合を防ぐ（NamedTempFile は /tmp 直下になるため共有される）。
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db");
    let db_path = db_path.to_str().unwrap();

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
    // db_path を TempDir 内に置くことで backup_dir（db_path の親/backup）も各テスト独立となり、
    // 並列実行時の /tmp/backup 共有競合を防ぐ（NamedTempFile は /tmp 直下になるため共有される）。
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db");
    let db_path = db_path.to_str().unwrap();

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
    // db_path を TempDir 内に置くことで backup_dir（db_path の親/backup）も各テスト独立となり、
    // 並列実行時の /tmp/backup 共有競合を防ぐ（NamedTempFile は /tmp 直下になるため共有される）。
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db");
    let db_path = db_path.to_str().unwrap();

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

#[test]
fn test_cli_version_flag() {
    // --version はバージョン文字列を出力して終了する（stdin不要）
    let output = Command::new("cargo")
        .args(&["run", "--bin", "kijuku-cli", "--", "--version"])
        .output()
        .expect("CLIの起動に失敗");

    assert!(
        output.status.success(),
        "exit code: {:?}, stderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let expected = format!("kijuku-cli {}", env!("CARGO_PKG_VERSION"));
    assert!(
        stdout.contains(&expected),
        "expected '{}' in output, got: {}",
        expected,
        stdout
    );
}

#[test]
fn test_cli_file_cp_dry_run_and_apply() {
    // db_path を TempDir 内に置くことで backup_dir（db_path の親/backup）も各テスト独立となり、
    // 並列実行時の /tmp/backup 共有競合を防ぐ（NamedTempFile は /tmp 直下になるため共有される）。
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db");
    let db_path = db_path.to_str().unwrap();
    let media_dir = TempDir::new().unwrap();
    let media_root = media_dir.path().to_str().unwrap();
    std::fs::write(media_dir.path().join("a.txt"), "hello").unwrap();

    // dry-run（デフォルト）: applied=false・ファイル未作成
    let resp = execute_cli_subcommand(db_path, media_root, &["file", "cp", "a.txt", "b.txt"]);
    assert_eq!(resp["success"], true);
    assert_eq!(resp["data"]["applied"], false);
    assert!(!media_dir.path().join("b.txt").exists());

    // --apply: applied=true・ファイル作成
    let resp = execute_cli_subcommand(db_path, media_root, &["file", "cp", "a.txt", "b.txt", "--apply"]);
    assert_eq!(resp["success"], true);
    assert_eq!(resp["data"]["applied"], true);
    let copied = std::fs::read_to_string(media_dir.path().join("b.txt")).unwrap();
    assert_eq!(copied, "hello");
}

#[test]
fn test_cli_file_rejects_traversal() {
    // db_path を TempDir 内に置くことで backup_dir（db_path の親/backup）も各テスト独立となり、
    // 並列実行時の /tmp/backup 共有競合を防ぐ（NamedTempFile は /tmp 直下になるため共有される）。
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db");
    let db_path = db_path.to_str().unwrap();
    let media_dir = TempDir::new().unwrap();
    let media_root = media_dir.path().to_str().unwrap();
    std::fs::write(media_dir.path().join("a.txt"), "hello").unwrap();

    // パストラバーサル: dst に ../ を含めると拒否
    let resp = execute_cli_subcommand(db_path, media_root, &["file", "cp", "a.txt", "../escape.txt"]);
    assert_eq!(resp["success"], false);
    assert!(resp["error"].as_str().unwrap().contains("media_cpエラー"));
    assert!(!media_dir.path().parent().unwrap().join("escape.txt").exists());
}

#[test]
fn test_cli_trash_roundtrip() {
    // db_path を TempDir 内に置くことで backup_dir（db_path の親/backup）も各テスト独立となり、
    // 並列実行時の /tmp/backup 共有競合を防ぐ（NamedTempFile は /tmp 直下になるため共有される）。
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db");
    let db_path = db_path.to_str().unwrap();
    let media_dir = TempDir::new().unwrap();
    let media_root = media_dir.path().to_str().unwrap();
    std::fs::write(media_dir.path().join("target.txt"), "data").unwrap();

    // trash へ移動（論理削除）
    let resp = execute_cli_subcommand(db_path, media_root, &["trash", "move", "target.txt", "--reason", "cleanup"]);
    assert_eq!(resp["success"], true);
    let id = resp["data"].as_str().unwrap().to_string();
    assert!(!media_dir.path().join("target.txt").exists());

    // 一覧: 1件
    let resp = execute_cli_subcommand(db_path, media_root, &["trash", "list"]);
    assert_eq!(resp["success"], true);
    assert_eq!(resp["data"].as_array().unwrap().len(), 1);

    // 復元（元の位置へ）
    let resp = execute_cli_subcommand(db_path, media_root, &["trash", "restore", &id]);
    assert_eq!(resp["success"], true);
    assert!(media_dir.path().join("target.txt").exists());

    // purge（dry-run）: 対象なしで空配列
    let resp = execute_cli_subcommand(db_path, media_root, &["trash", "purge", "--dry-run"]);
    assert_eq!(resp["success"], true);
}

/// stdin `sync` 操作が prod(RO)→stg(RW) のフル複製を行う（設計 §4.2・TASK-50 C4）。
/// データ・スキーマバージョンが stg に正しくコピーされることを検証する。
#[test]
fn test_cli_sync_replicates_prod_to_stg() {
    let dir = TempDir::new().unwrap();
    let prod = dir.path().join("prod.db");
    let stg = dir.path().join("stg.db");
    let prod_path = prod.to_str().unwrap();

    // CLI 経由で prod を構築（migrate + データ）
    execute_cli_command(prod_path, json!({"operation": "migrate", "params": {}}));
    execute_cli_command(prod_path, json!({
        "operation": "createMedia",
        "params": { "data": { "title": "prod作品", "media_type": "comic" } }
    }));
    assert!(!stg.exists(), "sync 前は stg が存在しない");

    // sync 操作（stdin）: prod → stg を明示
    let resp = execute_cli_command(prod_path, json!({
        "operation": "sync",
        "params": { "from": prod_path, "to": stg.to_str().unwrap() }
    }));
    assert_eq!(resp["success"], true, "sync 成功: {:?}", resp);
    assert_eq!(resp["data"]["prodPath"], prod_path);
    assert_eq!(resp["data"]["stgPath"], stg.to_str().unwrap());

    // stg を開いて prod のデータ・スキーマが複製されているか
    assert!(stg.exists(), "sync 後は stg が存在する");
    let find = execute_cli_command(stg.to_str().unwrap(), json!({
        "operation": "findMedia",
        "params": { "filter": {}, "options": null }
    }));
    assert_eq!(find["success"], true);
    let arr = find["data"].as_array().unwrap();
    assert_eq!(arr.len(), 1, "prod のメディアが1件複製される");
    assert_eq!(arr[0]["title"], "prod作品");

    let ver = execute_cli_command(stg.to_str().unwrap(), json!({
        "operation": "getSchemaVersion",
        "params": {}
    }));
    assert_eq!(ver["data"]["version"], 6, "スキーマバージョンも一致する");
}

/// stdin `sync` 操作で from == to は誤設定としてエラー（設計 §4.2）。
#[test]
fn test_cli_sync_rejects_same_path() {
    let dir = TempDir::new().unwrap();
    let same = dir.path().join("same.db");
    let same_path = same.to_str().unwrap();
    // DB を実在させる（migrate のみ）
    execute_cli_command(same_path, json!({"operation": "migrate", "params": {}}));

    let resp = execute_cli_command(same_path, json!({
        "operation": "sync",
        "params": { "from": same_path, "to": same_path }
    }));
    assert_eq!(resp["success"], false);
    assert!(
        resp["error"].as_str().unwrap().contains("same path"),
        "same-path エラーのべき: {:?}",
        resp
    );
}

/// stdin `diffProdStg` 操作が prod(RO)/stg 差分を返す（設計 §4.4・TASK-53）。
/// stg で新規追加したメディアが added として検出されることで、CLI 経由でも
/// セマンティクス反転（added=stg新規）が正しく機能することを検証する。
#[test]
fn test_cli_diff_prod_stg() {
    let dir = TempDir::new().unwrap();
    let prod = dir.path().join("prod.db");
    let stg = dir.path().join("stg.db");
    let prod_path = prod.to_str().unwrap();
    let stg_path = stg.to_str().unwrap();

    // prod 構築 + データ1件
    execute_cli_command(prod_path, json!({"operation": "migrate", "params": {}}));
    execute_cli_command(prod_path, json!({
        "operation": "createMedia",
        "params": { "data": { "title": "prod作品", "media_type": "comic" } }
    }));

    // sync prod → stg
    let sync = execute_cli_command(prod_path, json!({
        "operation": "sync",
        "params": { "from": prod_path, "to": stg_path }
    }));
    assert_eq!(sync["success"], true, "sync 成功: {:?}", sync);

    // sync 直後は差分なし（self=stg, prod を別途指定）
    let zero = execute_cli_command(stg_path, json!({
        "operation": "diffProdStg",
        "params": { "prodDbPath": prod_path }
    }));
    assert_eq!(zero["success"], true, "diffProdStg（ゼロ差分）: {:?}", zero);
    let zm = &zero["data"]["summary"]["media"];
    assert_eq!(zm["added"], 0);
    assert_eq!(zm["removed"], 0);
    assert_eq!(zm["changed"], 0);

    // stg で新規追加（promote 対象の added）
    execute_cli_command(stg_path, json!({
        "operation": "createMedia",
        "params": { "data": { "title": "stg新規", "media_type": "comic" } }
    }));

    let resp = execute_cli_command(stg_path, json!({
        "operation": "diffProdStg",
        "params": { "prodDbPath": prod_path }
    }));
    assert_eq!(resp["success"], true, "diffProdStg 成功: {:?}", resp);
    let media = &resp["data"]["summary"]["media"];
    assert_eq!(media["added"], 1, "stg 新規1件が added として検出される");
    assert_eq!(media["removed"], 0);
    assert_eq!(media["changed"], 0);
}

/// promote（stg→prod）が gate 合格で反映され pre-stash を作る（設計 §4.5/§6.4・TASK-58）。
#[test]
fn test_cli_promote_gate_pass() {
    let dir = TempDir::new().unwrap();
    let prod = dir.path().join("prod.db");
    let stg = dir.path().join("stg.db");
    let prod_path = prod.to_str().unwrap();
    let stg_path = stg.to_str().unwrap();

    // prod 構築 + 1件 → sync prod→stg → stg に新規追加
    execute_cli_command(prod_path, json!({"operation": "migrate", "params": {}}));
    execute_cli_command(prod_path, json!({"operation": "createMedia", "params": {"data": {"title": "prod", "media_type": "comic"}}}));
    execute_cli_command(prod_path, json!({"operation": "sync", "params": {"from": prod_path, "to": stg_path}}));
    execute_cli_command(stg_path, json!({"operation": "createMedia", "params": {"data": {"title": "stg新規", "media_type": "comic"}}}));

    // promote stg→prod
    let resp = execute_cli_command(stg_path, json!({
        "operation": "promote",
        "params": {"prodDbPath": prod_path}
    }));
    assert_eq!(resp["success"], true, "promote 成功: {:?}", resp);
    assert_eq!(resp["data"]["observe"]["passed"], true, "gate passed: {:?}", resp);
    assert!(
        resp["data"]["preStashPath"].as_str().is_some(),
        "preStashPath 存在: {:?}",
        resp
    );

    // promote 後、prod と stg は同一（差分ゼロ）。
    let diff = execute_cli_command(stg_path, json!({
        "operation": "diffProdStg",
        "params": {"prodDbPath": prod_path}
    }));
    let media = &diff["data"]["summary"]["media"];
    assert_eq!(media["added"], 0, "promote 後 added=0: {:?}", diff);
    assert_eq!(media["removed"], 0);
    assert_eq!(media["changed"], 0);
}

/// promote の backupOpts（pre-stash 先カスタマイズ）が CLI wire で受け渡される（設計 §4.5/§7.2・TASK-62）。
/// `backupOpts.backupDir`（camelCase）で指定したディレクトリ配下に pre-stash が作られることで、
/// `BackupOptions` の serde（rename_all=camelCase）復号を含む wire を検証する。
#[test]
fn test_cli_promote_with_backup_opts() {
    let dir = TempDir::new().unwrap();
    let prod = dir.path().join("prod.db");
    let stg = dir.path().join("stg.db");
    let prod_path = prod.to_str().unwrap();
    let stg_path = stg.to_str().unwrap();
    let custom_backup_dir = dir.path().join("custom-backup");
    let custom_backup_str = custom_backup_dir.to_str().unwrap();

    execute_cli_command(prod_path, json!({"operation": "migrate", "params": {}}));
    execute_cli_command(prod_path, json!({"operation": "createMedia", "params": {"data": {"title": "prod", "media_type": "comic"}}}));
    execute_cli_command(prod_path, json!({"operation": "sync", "params": {"from": prod_path, "to": stg_path}}));
    execute_cli_command(stg_path, json!({"operation": "createMedia", "params": {"data": {"title": "stg新規", "media_type": "comic"}}}));

    // promote with backupOpts（camelCase wire）。pre-stash 先を custom-backup/ に指定。
    let resp = execute_cli_command(stg_path, json!({
        "operation": "promote",
        "params": {
            "prodDbPath": prod_path,
            "backupOpts": { "backupDir": custom_backup_str, "enabled": false }
        }
    }));
    assert_eq!(resp["success"], true, "promote 成功: {:?}", resp);
    assert_eq!(resp["data"]["observe"]["passed"], true, "gate passed: {:?}", resp);

    // pre-stash は backupOpts.backupDir 配下に作られる（serde rename_all=camelCase で復号できた証拠）。
    let pre_stash = resp["data"]["preStashPath"]
        .as_str()
        .expect("preStashPath 存在");
    assert!(
        pre_stash.starts_with(custom_backup_str),
        "pre-stash under custom backupDir: {}",
        pre_stash
    );
}

/// promote gate 不合格で prod が未更新（設計 §6.4・TASK-58）。
#[test]
fn test_cli_promote_gate_fail() {
    let dir = TempDir::new().unwrap();
    let prod = dir.path().join("prod.db");
    let stg = dir.path().join("stg.db");
    let prod_path = prod.to_str().unwrap();
    let stg_path = stg.to_str().unwrap();

    execute_cli_command(prod_path, json!({"operation": "migrate", "params": {}}));
    execute_cli_command(prod_path, json!({"operation": "createMedia", "params": {"data": {"title": "prod", "media_type": "comic"}}}));
    execute_cli_command(prod_path, json!({"operation": "sync", "params": {"from": prod_path, "to": stg_path}}));
    execute_cli_command(stg_path, json!({"operation": "createMedia", "params": {"data": {"title": "stg新規", "media_type": "comic"}}}));

    // gate 不合格（maxAdded=0 で stg の新規1件を拒否）
    let resp = execute_cli_command(stg_path, json!({
        "operation": "promote",
        "params": {
            "prodDbPath": prod_path,
            "options": {"gateConfig": {"maxAdded": 0, "maxRemoved": 0, "maxChanged": 0}}
        }
    }));
    assert_eq!(resp["success"], false, "promote gate 不合格: {:?}", resp);
    assert!(
        resp["error"].as_str().unwrap_or("").contains("Promote gate failed"),
        "error 文字列: {:?}",
        resp["error"]
    );

    // prod は未更新（stg との差分 added=1 のまま）
    let diff = execute_cli_command(stg_path, json!({
        "operation": "diffProdStg",
        "params": {"prodDbPath": prod_path}
    }));
    let media = &diff["data"]["summary"]["media"];
    assert_eq!(media["added"], 1, "prod 未更新で added=1 のまま: {:?}", diff);
}

/// `sync-db` サブコマンドが prod→stg のフル複製を行う（設計 §4.2・TASK-50 C4）。
#[test]
fn test_cli_sync_db_subcommand_replicates() {
    let dir = TempDir::new().unwrap();
    let prod = dir.path().join("prod.db");
    let stg = dir.path().join("stg.db");
    let prod_path = prod.to_str().unwrap();

    // CLI 経由で prod を構築
    execute_cli_command(prod_path, json!({"operation": "migrate", "params": {}}));
    execute_cli_command(prod_path, json!({
        "operation": "createMedia",
        "params": { "data": { "title": "prod作品", "media_type": "comic" } }
    }));

    let out = execute_cli_db_subcommand(&[
        "sync-db",
        "--from",
        prod_path,
        "--to",
        stg.to_str().unwrap(),
    ]);
    assert!(out.contains("sync 完了"), "stdout: {}", out);
    assert!(stg.exists(), "sync 後は stg が存在する");

    // stg の内容確認
    let find = execute_cli_command(stg.to_str().unwrap(), json!({
        "operation": "findMedia",
        "params": { "filter": {}, "options": null }
    }));
    assert_eq!(find["data"].as_array().unwrap().len(), 1);
    assert_eq!(find["data"][0]["title"], "prod作品");
}

/// prod（readonly）セッションで書込操作が事前ガードで拒否される（設計 §5.1・方式A・TASK-52）。
#[test]
fn test_cli_readonly_rejects_write_operation() {
    // db_path を TempDir 内に置くことで backup_dir（db_path の親/backup）も各テスト独立となり、
    // 並列実行時の /tmp/backup 共有競合を防ぐ（NamedTempFile は /tmp 直下になるため共有される）。
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db");
    let db_path = db_path.to_str().unwrap();

    // 事前に migrate（デフォルト stg/RW）
    execute_cli_command(db_path, json!({"operation": "migrate", "params": {}}));

    // KIJUKU_TARGET=prod（readonly）で createMedia → 事前ガードで拒否
    let resp = execute_cli_command_with_env(
        db_path,
        json!({"operation": "createMedia", "params": {"data": {"title": "x", "media_type": "comic"}}}),
        &[("KIJUKU_TARGET", "prod")],
    );
    assert_eq!(resp["success"], false);
    assert!(
        resp["error"].as_str().unwrap().contains("読込専用"),
        "readonly 書込拒否のべき: {:?}",
        resp
    );
}

/// prod（readonly）セッションで読込操作は成功する（設計 §5.1）。
#[test]
fn test_cli_readonly_allows_read_operation() {
    // db_path を TempDir 内に置くことで backup_dir（db_path の親/backup）も各テスト独立となり、
    // 並列実行時の /tmp/backup 共有競合を防ぐ（NamedTempFile は /tmp 直下になるため共有される）。
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db");
    let db_path = db_path.to_str().unwrap();

    execute_cli_command(db_path, json!({"operation": "migrate", "params": {}}));

    // KIJUKU_TARGET=prod（readonly）で getSchemaVersion（読込）→ 成功
    let resp = execute_cli_command_with_env(
        db_path,
        json!({"operation": "getSchemaVersion", "params": {}}),
        &[("KIJUKU_TARGET", "prod")],
    );
    assert_eq!(resp["success"], true, "読込操作は許可されるべき: {:?}", resp);
}

/// KIJUKU_READ_SOURCE=prod で prod RO 読込専用セッション（方式A・TASK-52）。
/// 読込は成功し、書込はガードで拒否される。
#[test]
fn test_cli_read_source_prod_readonly_session() {
    // db_path を TempDir 内に置くことで backup_dir（db_path の親/backup）も各テスト独立となり、
    // 並列実行時の /tmp/backup 共有競合を防ぐ（NamedTempFile は /tmp 直下になるため共有される）。
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db");
    let db_path = db_path.to_str().unwrap();

    execute_cli_command(db_path, json!({"operation": "migrate", "params": {}}));

    // read-source=prod で読込 getSchemaVersion → 成功
    let read_resp = execute_cli_command_with_env(
        db_path,
        json!({"operation": "getSchemaVersion", "params": {}}),
        &[("KIJUKU_READ_SOURCE", "prod")],
    );
    assert_eq!(
        read_resp["success"], true,
        "read-source=prod で読込成功のべき: {:?}",
        read_resp
    );

    // read-source=prod で書込 createMedia → ガードで拒否
    let write_resp = execute_cli_command_with_env(
        db_path,
        json!({"operation": "createMedia", "params": {"data": {"title": "x", "media_type": "comic"}}}),
        &[("KIJUKU_READ_SOURCE", "prod")],
    );
    assert_eq!(write_resp["success"], false);
    assert!(write_resp["error"].as_str().unwrap().contains("読込専用"));
}

// ==== import サブコマンド（TASK-68・TS cli.ts runImport パリティ） ====

/// import の基本（TSV）。title/media_type 必須。imported 件数と findMedia で登録を検証。
#[test]
fn test_cli_import_tsv_basic() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db");
    let db_path = db_path.to_str().unwrap();
    let media_dir = TempDir::new().unwrap();
    let media_root = media_dir.path().to_str().unwrap();

    let tsv_path = dir.path().join("data.tsv");
    fs::write(
        &tsv_path,
        "title\tmedia_type\tartist\tlanguage\n\
         コミック1\tcomic\t作者A\tja\n\
         ビデオ1\tvideo\t作者B\ten\n",
    )
    .unwrap();

    let resp = execute_cli_subcommand(
        db_path,
        media_root,
        &["import", tsv_path.to_str().unwrap()],
    );
    assert_eq!(resp["success"], true, "import 成功のべき: {:?}", resp);
    assert_eq!(resp["data"]["imported"], 2);
    assert_eq!(resp["data"]["tags"], 0);
    assert_eq!(resp["data"]["attributes"], 0);

    // findMedia で comic 1件ヒット
    let find = execute_cli_command(
        db_path,
        json!({"operation":"findMedia","params":{"filter":{"media_type":"comic"},"options":null}}),
    );
    assert_eq!(find["success"], true);
    assert_eq!(find["data"].as_array().unwrap().len(), 1);
    assert_eq!(find["data"][0]["title"], "コミック1");
    assert_eq!(find["data"][0]["artist"], "作者A");
}

/// import（JSON 配列）。
#[test]
fn test_cli_import_json() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db");
    let db_path = db_path.to_str().unwrap();
    let media_dir = TempDir::new().unwrap();
    let media_root = media_dir.path().to_str().unwrap();

    let json_path = dir.path().join("data.json");
    fs::write(
        &json_path,
        r#"[
          {"title":"タイトル1","media_type":"music","duration_sec":120},
          {"title":"タイトル2","media_type":"comic","flag_exist":true}
        ]"#,
    )
    .unwrap();

    let resp = execute_cli_subcommand(
        db_path,
        media_root,
        &["import", json_path.to_str().unwrap()],
    );
    assert_eq!(resp["success"], true, "{:?}", resp);
    assert_eq!(resp["data"]["imported"], 2);

    // JSON の数値・bool が正しく解釈されるか検証
    let find = execute_cli_command(
        db_path,
        json!({"operation":"findMedia","params":{"filter":{"media_type":"music"},"options":null}}),
    );
    assert_eq!(find["data"][0]["duration_sec"], 120);

    let find2 = execute_cli_command(
        db_path,
        json!({"operation":"findMedia","params":{"filter":{"media_type":"comic"},"options":null}}),
    );
    assert_eq!(find2["data"][0]["flag_exist"], true);
}

/// tags 列（カンマ区切り）→ タグ関連付け。件数と getMediaTags で検証。
#[test]
fn test_cli_import_with_tags() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db");
    let db_path = db_path.to_str().unwrap();
    let media_dir = TempDir::new().unwrap();
    let media_root = media_dir.path().to_str().unwrap();

    let tsv_path = dir.path().join("tags.tsv");
    fs::write(
        &tsv_path,
        "title\tmedia_type\ttags\nコミック1\tcomic\taction, 冒険\n",
    )
    .unwrap();

    let resp = execute_cli_subcommand(
        db_path,
        media_root,
        &["import", tsv_path.to_str().unwrap()],
    );
    assert_eq!(resp["success"], true, "{:?}", resp);
    assert_eq!(resp["data"]["tags"], 2);

    let find = execute_cli_command(
        db_path,
        json!({"operation":"findMedia","params":{"filter":{"title":"コミック1"},"options":null}}),
    );
    let id = find["data"][0]["id"].as_i64().unwrap();

    let tags = execute_cli_command(
        db_path,
        json!({"operation":"getMediaTags","params":{"media_id":id}}),
    );
    assert_eq!(tags["success"], true);
    let names: Vec<&str> = tags["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"action"));
    assert!(names.contains(&"冒険"));
}

/// --additional-columns → media_attributes へ格納。
#[test]
fn test_cli_import_with_additional_columns() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db");
    let db_path = db_path.to_str().unwrap();
    let media_dir = TempDir::new().unwrap();
    let media_root = media_dir.path().to_str().unwrap();

    let tsv_path = dir.path().join("attr.tsv");
    fs::write(
        &tsv_path,
        "title\tmedia_type\tid_old\tcustom\nコミック1\tcomic\tA001\tfoo\n",
    )
    .unwrap();

    let resp = execute_cli_subcommand(
        db_path,
        media_root,
        &[
            "import",
            tsv_path.to_str().unwrap(),
            "--additional-columns",
            "id_old,custom",
        ],
    );
    assert_eq!(resp["success"], true, "{:?}", resp);
    assert_eq!(resp["data"]["attributes"], 2);

    let find = execute_cli_command(
        db_path,
        json!({"operation":"findMedia","params":{"filter":{"title":"コミック1"},"options":null}}),
    );
    let id = find["data"][0]["id"].as_i64().unwrap();

    let attrs = execute_cli_command(
        db_path,
        json!({"operation":"getMediaAttributes","params":{"media_id":id}}),
    );
    assert_eq!(attrs["success"], true);
    let map: std::collections::HashMap<&str, &str> = attrs["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| (a["key"].as_str().unwrap(), a["value"].as_str().unwrap()))
        .collect();
    assert_eq!(map.get("id_old"), Some(&"A001"));
    assert_eq!(map.get("custom"), Some(&"foo"));
}

/// 不正な media_type はバッチ全体を失敗させる。
#[test]
fn test_cli_import_invalid_media_type() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db");
    let db_path = db_path.to_str().unwrap();
    let media_dir = TempDir::new().unwrap();
    let media_root = media_dir.path().to_str().unwrap();

    let tsv_path = dir.path().join("bad.tsv");
    fs::write(&tsv_path, "title\tmedia_type\nX\tunknown\n").unwrap();

    let resp = execute_cli_subcommand(
        db_path,
        media_root,
        &["import", tsv_path.to_str().unwrap()],
    );
    assert_eq!(resp["success"], false);
    assert!(resp["error"].as_str().unwrap().contains("media_type"));
}

/// 存在しないファイル。
#[test]
fn test_cli_import_missing_file() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db");
    let db_path = db_path.to_str().unwrap();
    let media_dir = TempDir::new().unwrap();
    let media_root = media_dir.path().to_str().unwrap();

    let missing = dir.path().join("no_such_file.tsv");
    let resp = execute_cli_subcommand(
        db_path,
        media_root,
        &["import", missing.to_str().unwrap()],
    );
    assert_eq!(resp["success"], false);
    assert!(resp["error"].as_str().unwrap().contains("ファイルが見つかりません"));
}

/// prod（readonly）では import 拒否。
#[test]
fn test_cli_import_prod_readonly_rejected() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db");
    let db_path = db_path.to_str().unwrap();
    let media_dir = TempDir::new().unwrap();
    let media_root = media_dir.path().to_str().unwrap();

    let tsv_path = dir.path().join("data.tsv");
    fs::write(&tsv_path, "title\tmedia_type\nX\tcomic\n").unwrap();

    let resp = execute_cli_subcommand(
        db_path,
        media_root,
        &[
            "--target",
            "prod",
            "import",
            tsv_path.to_str().unwrap(),
        ],
    );
    assert_eq!(resp["success"], false);
    assert!(resp["error"].as_str().unwrap().contains("readonly"));
}
