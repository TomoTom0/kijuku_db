use kijuku_db::{file_ops::FileOpOptions, trash::TrashOperation, RemoteConfig, RemoteKijukuDB};
use std::path::{Path, PathBuf};

#[test]
fn test_remote_config_default() {
    let config = RemoteConfig::default();

    assert_eq!(config.ssh_host, "localhost");
    assert_eq!(config.port, Some(22));
    assert!(config.username.is_none());
    assert_eq!(
        config.db_path,
        Some("~/.local/share/kijuku/kijuku.db".to_string())
    );
    assert_eq!(
        config.binary_path,
        Some("~/.local/bin/kijuku-cli".to_string())
    );
}

#[test]
fn test_remote_config_custom() {
    let config = RemoteConfig {
        ssh_host: "example.com".to_string(),
        port: Some(2222),
        username: Some("testuser".to_string()),
        private_key_path: Some(PathBuf::from("/home/user/.ssh/id_rsa")),
        db_path: Some("/custom/path/db.db".to_string()),
        binary_path: Some("/custom/path/binary".to_string()),
        media_root: None,
        ..Default::default()
    };

    assert_eq!(config.ssh_host, "example.com");
    assert_eq!(config.port, Some(2222));
    assert_eq!(config.username, Some("testuser".to_string()));
    assert_eq!(
        config.private_key_path,
        Some(PathBuf::from("/home/user/.ssh/id_rsa"))
    );
    assert_eq!(config.db_path, Some("/custom/path/db.db".to_string()));
    assert_eq!(config.binary_path, Some("/custom/path/binary".to_string()));
}

#[test]
fn test_remote_kijukudb_creation() {
    let config = RemoteConfig {
        ssh_host: "localhost".to_string(),
        port: Some(22),
        username: Some("test".to_string()),
        private_key_path: Some(PathBuf::from("/tmp/test_key")),
        db_path: None,
        binary_path: None,
        media_root: None,
        ..Default::default()
    };

    let _remote_db = RemoteKijukuDB::new(config);
    // RemoteKijukuDBインスタンスが正常に作成されることを確認
}

#[test]
fn test_remote_config_clone() {
    let config1 = RemoteConfig {
        ssh_host: "host1.com".to_string(),
        port: Some(22),
        username: Some("user1".to_string()),
        private_key_path: Some(PathBuf::from("/path/to/key")),
        db_path: Some("/path/to/db".to_string()),
        binary_path: Some("/path/to/binary".to_string()),
        media_root: None,
        ..Default::default()
    };

    let config2 = config1.clone();

    assert_eq!(config1.ssh_host, config2.ssh_host);
    assert_eq!(config1.port, config2.port);
    assert_eq!(config1.username, config2.username);
    assert_eq!(config1.private_key_path, config2.private_key_path);
    assert_eq!(config1.db_path, config2.db_path);
    assert_eq!(config1.binary_path, config2.binary_path);
}

#[test]
fn test_remote_config_partial_options() {
    let config = RemoteConfig {
        ssh_host: "remote.host".to_string(),
        port: None, // デフォルトのポート22が使われる
        username: Some("user".to_string()),
        private_key_path: Some(PathBuf::from("/home/user/.ssh/id_ed25519")),
        db_path: None, // デフォルトパスが使われる
        binary_path: None, // デフォルトパスが使われる
        media_root: None,
        ..Default::default()
    };

    assert_eq!(config.ssh_host, "remote.host");
    assert_eq!(config.port, None);
    assert_eq!(config.username, Some("user".to_string()));
    assert_eq!(config.db_path, None);
    assert_eq!(config.binary_path, None);
}

// 注意: 以下のテストは実際のSSHサーバーが必要なため、
// 通常はスキップされます。環境変数KIJUKU_TEST_SSH_HOSTが設定されている場合のみ実行されます。

#[test]
#[ignore] // デフォルトではスキップ
fn test_ssh_connection() {
    // このテストは実際のSSHサーバーへの接続をテストします
    // 環境変数を使って接続情報を設定してください:
    // KIJUKU_TEST_SSH_HOST=example.com
    // KIJUKU_TEST_SSH_USER=testuser
    // KIJUKU_TEST_SSH_KEY=/path/to/key

    let ssh_host = std::env::var("KIJUKU_TEST_SSH_HOST").ok();
    let ssh_user = std::env::var("KIJUKU_TEST_SSH_USER").ok();
    let ssh_key = std::env::var("KIJUKU_TEST_SSH_KEY").ok();

    if ssh_host.is_none() || ssh_user.is_none() || ssh_key.is_none() {
        eprintln!("Skipping SSH connection test: environment variables not set");
        return;
    }

    let config = RemoteConfig {
        ssh_host: ssh_host.unwrap(),
        port: Some(22),
        username: Some(ssh_user.unwrap()),
        private_key_path: Some(PathBuf::from(ssh_key.unwrap())),
        db_path: None,
        binary_path: None,
        media_root: None,
        ..Default::default()
    };

    let remote_db = RemoteKijukuDB::new(config);

    // ここで実際のSSH接続テストを実行
    // 注意: このテストは実際のSSH環境でのみ動作します
    let _version = remote_db.get_schema_version();
    // assert!(version.is_ok());
}

#[test]
#[ignore] // デフォルトではスキップ
fn test_remote_migrate() {
    // 実際のSSH環境でのマイグレーションテスト
    // 環境変数が設定されている場合のみ実行

    let ssh_host = std::env::var("KIJUKU_TEST_SSH_HOST").ok();
    let ssh_user = std::env::var("KIJUKU_TEST_SSH_USER").ok();
    let ssh_key = std::env::var("KIJUKU_TEST_SSH_KEY").ok();

    if ssh_host.is_none() || ssh_user.is_none() || ssh_key.is_none() {
        return;
    }

    let config = RemoteConfig {
        ssh_host: ssh_host.unwrap(),
        port: Some(22),
        username: Some(ssh_user.unwrap()),
        private_key_path: Some(PathBuf::from(ssh_key.unwrap())),
        db_path: Some("/tmp/test_remote.db".to_string()),
        binary_path: None,
        media_root: None,
        ..Default::default()
    };

    let remote_db = RemoteKijukuDB::new(config);

    let _result = remote_db.migrate();
    // assert!(result.is_ok());
}

#[test]
#[ignore] // デフォルトではスキップ
fn test_remote_crud_operations() {
    // 実際のSSH環境でのCRUD操作テスト
    // 環境変数が設定されている場合のみ実行

    let ssh_host = std::env::var("KIJUKU_TEST_SSH_HOST").ok();
    let ssh_user = std::env::var("KIJUKU_TEST_SSH_USER").ok();
    let ssh_key = std::env::var("KIJUKU_TEST_SSH_KEY").ok();

    if ssh_host.is_none() || ssh_user.is_none() || ssh_key.is_none() {
        return;
    }

    let config = RemoteConfig {
        ssh_host: ssh_host.unwrap(),
        port: Some(22),
        username: Some(ssh_user.unwrap()),
        private_key_path: Some(PathBuf::from(ssh_key.unwrap())),
        db_path: Some("/tmp/test_remote.db".to_string()),
        binary_path: None,
        media_root: None,
        ..Default::default()
    };

    let _remote_db = RemoteKijukuDB::new(config);

    // ここで実際のCRUD操作をテスト
    // MediaInput を作成してcreate_mediaを呼び出すなど
}

#[test]
#[ignore] // 実 SSH 環境（KIJUKU_TEST_SSH_* + KIJUKU_TEST_MEDIA_ROOT）が必要
fn test_remote_media_file_operations() {
    let ssh_host = std::env::var("KIJUKU_TEST_SSH_HOST").ok();
    let ssh_user = std::env::var("KIJUKU_TEST_SSH_USER").ok();
    let ssh_key = std::env::var("KIJUKU_TEST_SSH_KEY").ok();
    let media_root = std::env::var("KIJUKU_TEST_MEDIA_ROOT").ok();

    if ssh_host.is_none()
        || ssh_user.is_none()
        || ssh_key.is_none()
        || media_root.is_none()
    {
        return;
    }

    let config = RemoteConfig {
        ssh_host: ssh_host.unwrap(),
        port: Some(22),
        username: ssh_user,
        private_key_path: ssh_key.map(PathBuf::from),
        db_path: Some("/tmp/test_remote.db".to_string()),
        binary_path: None,
        media_root,
        ..Default::default()
    };
    let remote = RemoteKijukuDB::new(config);

    // media_cp（dry-run: apply=false で計画のみ取得・FS 不変）
    let opts = FileOpOptions {
        apply: false,
        update_db: false,
    };
    let _ = remote.media_cp("e2e_src.txt", "e2e_dst.txt", &opts);

    // trash 系の呼び出し
    let _ = remote.move_to_trash("e2e_trash.txt", TrashOperation::Delete, None);
    let _ = remote.list_trash();

    // upload/download ラウンドトリップ（KIJUKU_TEST_LOCAL_FILE に有効なローカルファイルを指定時のみ）
    if let Ok(local_file) = std::env::var("KIJUKU_TEST_LOCAL_FILE") {
        let _ = remote.upload(Path::new(&local_file), "e2e/uploaded.txt");
        let _ = remote.download("e2e/uploaded.txt", Path::new("/tmp/kijuku_e2e_downloaded.txt"));
    }
}

// --- SSH Session 接続プール（TASK-70） ---

#[test]
#[ignore] // 実 SSH 環境（KIJUKU_TEST_SSH_*）が必要
fn test_remote_session_reuse_across_rpcs() {
    // TASK-70: 連続 RPC で SSH Session が再利用されること（connect_count が増えない）を検証
    let ssh_host = std::env::var("KIJUKU_TEST_SSH_HOST").ok();
    let ssh_user = std::env::var("KIJUKU_TEST_SSH_USER").ok();
    let ssh_key = std::env::var("KIJUKU_TEST_SSH_KEY").ok();

    if ssh_host.is_none() || ssh_user.is_none() || ssh_key.is_none() {
        return;
    }

    let config = RemoteConfig {
        ssh_host: ssh_host.unwrap(),
        port: Some(22),
        username: Some(ssh_user.unwrap()),
        private_key_path: Some(PathBuf::from(ssh_key.unwrap())),
        db_path: Some("/tmp/test_remote.db".to_string()),
        binary_path: None,
        media_root: None,
        ..Default::default()
    };
    let remote = RemoteKijukuDB::new(config);

    // 初回 RPC で接続確立（結果の Err は問わない・connect_count 検証が主目的）
    let _ = remote.get_schema_version();
    let count_after_first = remote.connect_count();
    assert!(
        count_after_first >= 1,
        "初回 RPC で SSH 接続が確立されるべき"
    );

    // 2回目の RPC は Session を再利用し接続回数は増えない
    let _ = remote.get_schema_version();
    assert_eq!(
        remote.connect_count(),
        count_after_first,
        "2回目の RPC は Session を再利用し接続回数は増えないべき"
    );
}

#[test]
#[ignore] // 実 SSH 環境（KIJUKU_TEST_SSH_*）が必要
fn test_remote_disconnect_then_reconnect() {
    // TASK-70: disconnect() 後に slot が無効化され、次 RPC で再接続することを検証
    let ssh_host = std::env::var("KIJUKU_TEST_SSH_HOST").ok();
    let ssh_user = std::env::var("KIJUKU_TEST_SSH_USER").ok();
    let ssh_key = std::env::var("KIJUKU_TEST_SSH_KEY").ok();

    if ssh_host.is_none() || ssh_user.is_none() || ssh_key.is_none() {
        return;
    }

    let config = RemoteConfig {
        ssh_host: ssh_host.unwrap(),
        port: Some(22),
        username: Some(ssh_user.unwrap()),
        private_key_path: Some(PathBuf::from(ssh_key.unwrap())),
        db_path: Some("/tmp/test_remote.db".to_string()),
        binary_path: None,
        media_root: None,
        ..Default::default()
    };
    let remote = RemoteKijukuDB::new(config);

    let _ = remote.get_schema_version();
    let count_before = remote.connect_count();

    // disconnect で slot 無効化
    let _ = remote.disconnect();

    // 次 RPC で再接続（connect_count 増加）
    let _ = remote.get_schema_version();
    assert_eq!(
        remote.connect_count(),
        count_before + 1,
        "disconnect 後の RPC は再接続されるべき"
    );
}
