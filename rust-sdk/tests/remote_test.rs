use kijuku_db::{RemoteConfig, RemoteKijukuDB};
use std::path::PathBuf;

#[test]
fn test_remote_config_default() {
    let config = RemoteConfig::default();

    assert_eq!(config.ssh_host, "localhost");
    assert_eq!(config.port, Some(22));
    assert!(config.username.is_empty());
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
        username: "testuser".to_string(),
        private_key_path: Some(PathBuf::from("/home/user/.ssh/id_rsa")),
        db_path: Some("/custom/path/db.db".to_string()),
        binary_path: Some("/custom/path/binary".to_string()),
    };

    assert_eq!(config.ssh_host, "example.com");
    assert_eq!(config.port, Some(2222));
    assert_eq!(config.username, "testuser");
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
        username: "test".to_string(),
        private_key_path: Some(PathBuf::from("/tmp/test_key")),
        db_path: None,
        binary_path: None,
    };

    let _remote_db = RemoteKijukuDB::new(config);
    // RemoteKijukuDBインスタンスが正常に作成されることを確認
}

#[test]
fn test_remote_config_clone() {
    let config1 = RemoteConfig {
        ssh_host: "host1.com".to_string(),
        port: Some(22),
        username: "user1".to_string(),
        private_key_path: Some(PathBuf::from("/path/to/key")),
        db_path: Some("/path/to/db".to_string()),
        binary_path: Some("/path/to/binary".to_string()),
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
        username: "user".to_string(),
        private_key_path: Some(PathBuf::from("/home/user/.ssh/id_ed25519")),
        db_path: None, // デフォルトパスが使われる
        binary_path: None, // デフォルトパスが使われる
    };

    assert_eq!(config.ssh_host, "remote.host");
    assert_eq!(config.port, None);
    assert_eq!(config.username, "user");
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
        username: ssh_user.unwrap(),
        private_key_path: Some(PathBuf::from(ssh_key.unwrap())),
        db_path: None,
        binary_path: None,
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
        username: ssh_user.unwrap(),
        private_key_path: Some(PathBuf::from(ssh_key.unwrap())),
        db_path: Some("/tmp/test_remote.db".to_string()),
        binary_path: None,
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
        username: ssh_user.unwrap(),
        private_key_path: Some(PathBuf::from(ssh_key.unwrap())),
        db_path: Some("/tmp/test_remote.db".to_string()),
        binary_path: None,
    };

    let _remote_db = RemoteKijukuDB::new(config);

    // ここで実際のCRUD操作をテスト
    // MediaInput を作成してcreate_mediaを呼び出すなど
}
