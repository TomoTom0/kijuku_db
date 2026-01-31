use kijuku_db::{BackupManager, BackupOptions, DBOptions, KijukuDB, MediaInput, MediaType};
use rusqlite;
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

#[test]
fn test_backup_integration_with_kijukudb() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let backup_dir = temp_dir.path().join("backups");

    // バックアップオプション付きでデータベースを開く
    let options = DBOptions {
        timeout: None,
        readonly: false,
        verbose: false,
        backup: Some(BackupOptions {
            backup_dir: Some(backup_dir.to_string_lossy().to_string()),
            interval_ms: Some(1000), // 1秒間隔
            enabled: Some(true),
            ..Default::default()
        }),
    };

    let db = KijukuDB::open_with_options(&db_path, options).unwrap();

    // マイグレーションを実行
    db.migrate().unwrap();

    // バックアップマネージャーが初期化されていることを確認
    let backup_manager = db.get_backup_manager();
    assert!(backup_manager.is_some());

    let backup_manager = backup_manager.unwrap();

    // 手動バックアップを作成
    let backup_path = backup_manager.backup().unwrap();
    assert!(std::path::Path::new(&backup_path).exists());

    // バックアップ一覧を取得
    let backups = backup_manager.list_backups().unwrap();
    assert_eq!(backups.len(), 1);
}

#[test]
fn test_auto_backup_with_record_operation() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let backup_dir = temp_dir.path().join("backups");

    // 短い間隔でバックアップを設定
    let options = BackupOptions {
        backup_dir: Some(backup_dir.to_string_lossy().to_string()),
        interval_ms: Some(100), // 100ミリ秒間隔
        enabled: Some(true),
            ..Default::default()
    };

    // 実際のSQLiteデータベースを作成
    {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute("CREATE TABLE test (id INTEGER PRIMARY KEY, data TEXT)", []).unwrap();
        conn.execute("INSERT INTO test (data) VALUES (?1)", ["test data"]).unwrap();
    }

    let manager = BackupManager::new(&db_path, options).unwrap();

    // 最初の操作を記録（バックアップが作成される）
    manager.record_operation().unwrap();

    let backups = manager.list_backups().unwrap();
    assert_eq!(backups.len(), 1);

    // 間隔内の操作（バックアップは作成されない）
    manager.record_operation().unwrap();
    let backups = manager.list_backups().unwrap();
    assert_eq!(backups.len(), 1);

    // 間隔を超えて待機
    thread::sleep(Duration::from_millis(150));

    // 新しいバックアップが作成される
    manager.record_operation().unwrap();
    let backups = manager.list_backups().unwrap();
    assert_eq!(backups.len(), 2);
}

#[test]
fn test_manual_backup() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let backup_dir = temp_dir.path().join("backups");

    let options = BackupOptions {
        backup_dir: Some(backup_dir.to_string_lossy().to_string()),
        interval_ms: Some(3600000), // 1時間
        enabled: Some(true),
            ..Default::default()
    };

    // 実際のSQLiteデータベースを作成
    {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute("CREATE TABLE test (id INTEGER PRIMARY KEY, data TEXT)", []).unwrap();
        conn.execute("INSERT INTO test (data) VALUES (?1)", ["test data"]).unwrap();
    }

    let manager = BackupManager::new(&db_path, options).unwrap();

    // 手動バックアップを3回作成
    for _ in 0..3 {
        manager.backup().unwrap();
        thread::sleep(Duration::from_millis(10));
    }

    let backups = manager.list_backups().unwrap();
    assert_eq!(backups.len(), 3);

    // 最新のバックアップが最初にある（降順ソート）
    for i in 0..backups.len() - 1 {
        assert!(backups[i].created_at >= backups[i + 1].created_at);
    }
}

#[test]
fn test_backup_list() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let backup_dir = temp_dir.path().join("backups");

    let options = BackupOptions {
        backup_dir: Some(backup_dir.to_string_lossy().to_string()),
        interval_ms: Some(1000),
        enabled: Some(true),
            ..Default::default()
    };

    // 実際のSQLiteデータベースを作成
    {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute("CREATE TABLE test (id INTEGER PRIMARY KEY, data TEXT)", []).unwrap();
        conn.execute("INSERT INTO test (data) VALUES (?1)", ["test data"]).unwrap();
    }

    let manager = BackupManager::new(&db_path, options).unwrap();

    // バックアップが存在しない場合は空のリストを返す
    let backups = manager.list_backups().unwrap();
    assert_eq!(backups.len(), 0);

    // バックアップを作成
    manager.backup().unwrap();

    let backups = manager.list_backups().unwrap();
    assert_eq!(backups.len(), 1);

    // バックアップ情報を確認
    // 新しいファイル名形式: {db_stem}.backup-{yyyymmddhhmmss-mmm}.db
    let backup = &backups[0];
    assert!(backup.name.starts_with("test.backup-"));
    assert!(backup.name.ends_with(".db"));
    assert!(backup.path.exists());
}

#[test]
fn test_backup_timestamps() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let backup_dir = temp_dir.path().join("backups");

    let options = BackupOptions {
        backup_dir: Some(backup_dir.to_string_lossy().to_string()),
        interval_ms: Some(1000),
        enabled: Some(true),
            ..Default::default()
    };

    // 実際のSQLiteデータベースを作成
    {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute("CREATE TABLE test (id INTEGER PRIMARY KEY, data TEXT)", []).unwrap();
        conn.execute("INSERT INTO test (data) VALUES (?1)", ["test data"]).unwrap();
    }

    let manager = BackupManager::new(&db_path, options).unwrap();

    // 最初はバックアップ時刻がない
    assert!(manager.get_last_backup_time().is_none());
    assert!(manager.get_last_operation_time().is_none());

    // 操作を記録
    manager.record_operation().unwrap();

    // バックアップ時刻と操作時刻が記録される
    assert!(manager.get_last_backup_time().is_some());
    assert!(manager.get_last_operation_time().is_some());

    // 次回バックアップまでの時間を取得
    let time_until_next = manager.get_time_until_next_backup();
    assert!(time_until_next.is_some());
    assert!(time_until_next.unwrap() <= 1000);
}

#[test]
fn test_backup_disabled() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let backup_dir = temp_dir.path().join("backups");

    let options = BackupOptions {
        backup_dir: Some(backup_dir.to_string_lossy().to_string()),
        interval_ms: Some(1000),
        enabled: Some(false), // 無効化
        ..Default::default()
    };

    // 実際のSQLiteデータベースを作成
    {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute("CREATE TABLE test (id INTEGER PRIMARY KEY, data TEXT)", []).unwrap();
        conn.execute("INSERT INTO test (data) VALUES (?1)", ["test data"]).unwrap();
    }

    let manager = BackupManager::new(&db_path, options).unwrap();

    // バックアップが無効なのでエラーになる
    let result = manager.backup();
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().to_string(), "Error: Backup is disabled");

    // record_operationはエラーにならない（単に何もしない）
    let result = manager.record_operation();
    assert!(result.is_ok());
}

#[test]
fn test_kijukudb_without_backup() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // バックアップオプションなしでデータベースを開く
    let db = KijukuDB::open(&db_path).unwrap();

    // バックアップマネージャーが存在しない
    assert!(db.get_backup_manager().is_none());
}

#[test]
fn test_backup_with_actual_database() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let backup_dir = temp_dir.path().join("backups");

    let options = DBOptions {
        timeout: None,
        readonly: false,
        verbose: false,
        backup: Some(BackupOptions {
            backup_dir: Some(backup_dir.to_string_lossy().to_string()),
            interval_ms: Some(1000),
            enabled: Some(true),
            ..Default::default()
        }),
    };

    let db = KijukuDB::open_with_options(&db_path, options).unwrap();
    db.migrate().unwrap();

    // メディアを作成
    let input = MediaInput {
        title: "テストメディア".to_string(),
        media_type: MediaType::Comic,
        ..Default::default()
    };

    db.create_media(&input).unwrap();

    // バックアップを作成
    let backup_manager = db.get_backup_manager().unwrap();
    let backup_path = backup_manager.backup().unwrap();

    // バックアップファイルが存在することを確認
    assert!(std::path::Path::new(&backup_path).exists());

    // バックアップファイルからデータベースを開いて内容を確認
    let backup_db = KijukuDB::open(&backup_path).unwrap();
    let media = backup_db.get_media(1);
    assert!(media.is_some());
    assert_eq!(media.unwrap().title, "テストメディア");
}

#[test]
fn test_db_backup_method() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let backup_dir = temp_dir.path().join("backups");

    // バックアップオプション付きでデータベースを開く
    let options = DBOptions {
        backup: Some(BackupOptions {
            backup_dir: Some(backup_dir.to_string_lossy().to_string()),
            enabled: Some(true),
            ..Default::default()
        }),
        ..Default::default()
    };

    let db = KijukuDB::open_with_options(&db_path, options).unwrap();
    db.migrate().unwrap();

    // メディアを作成
    db.create_media(&MediaInput {
        title: "バックアップテスト".to_string(),
        media_type: MediaType::Comic,
        ..Default::default()
    }).unwrap();

    // backup()メソッドを直接呼び出す
    let backup_path = db.backup().unwrap();
    assert!(backup_path.is_some());

    let backup_path = backup_path.unwrap();
    assert!(std::path::Path::new(&backup_path).exists());

    // バックアップファイルからデータベースを開いて内容を確認
    let backup_db = KijukuDB::open(&backup_path).unwrap();
    let all_media = backup_db.find_media(&Default::default(), None).unwrap();
    assert_eq!(all_media.len(), 1);
    assert_eq!(all_media[0].title, "バックアップテスト");
}

#[test]
fn test_db_backup_method_without_backup_manager() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // バックアップマネージャーなしでデータベースを開く
    let db = KijukuDB::open(&db_path).unwrap();
    db.migrate().unwrap();

    // backup()メソッドを呼び出すと None が返る
    let result = db.backup().unwrap();
    assert!(result.is_none());
}

#[test]
fn test_db_list_backups_method() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let backup_dir = temp_dir.path().join("backups");

    // バックアップオプション付きでデータベースを開く
    let options = DBOptions {
        backup: Some(BackupOptions {
            backup_dir: Some(backup_dir.to_string_lossy().to_string()),
            enabled: Some(true),
            ..Default::default()
        }),
        ..Default::default()
    };

    let db = KijukuDB::open_with_options(&db_path, options).unwrap();
    db.migrate().unwrap();

    // 最初はバックアップがない
    let backups = db.list_backups().unwrap();
    assert_eq!(backups.len(), 0);

    // バックアップを作成
    db.backup().unwrap();

    // バックアップ一覧を取得
    let backups = db.list_backups().unwrap();
    assert_eq!(backups.len(), 1);
    assert!(backups[0].name.contains(".backup-"));
    assert!(backups[0].path.exists());

    // 2つ目のバックアップを作成
    thread::sleep(Duration::from_millis(100)); // ファイル名が重複しないように待機
    db.backup().unwrap();

    // バックアップ一覧を取得
    let backups = db.list_backups().unwrap();
    assert_eq!(backups.len(), 2);
}

#[test]
fn test_db_list_backups_method_without_backup_manager() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // バックアップマネージャーなしでデータベースを開く
    let db = KijukuDB::open(&db_path).unwrap();
    db.migrate().unwrap();

    // list_backups()メソッドを呼び出すと空のリストが返る
    let backups = db.list_backups().unwrap();
    assert_eq!(backups.len(), 0);
}
