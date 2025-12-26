use crate::{KijukuError, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

/// バックアップ進捗情報
#[derive(Debug, Clone)]
pub struct BackupProgress {
    pub total_pages: i32,
    pub remaining_pages: i32,
}

/// バックアップ設定オプション
#[derive(Debug, Clone)]
pub struct BackupOptions {
    /// バックアップファイルの保存先ディレクトリ
    pub backup_dir: String,
    /// バックアップをトリガーする時間間隔（ミリ秒）
    /// デフォルト: 3600000 (1時間)
    pub interval_ms: Option<u64>,
    /// バックアップ機能の有効/無効
    /// デフォルト: true
    pub enabled: Option<bool>,
    /// 保持する最大バックアップ数
    /// Noneの場合は無制限
    pub max_backups: Option<usize>,
    /// 保持する最大日数
    /// Noneの場合は無制限
    pub max_age_days: Option<u64>,
}

impl Default for BackupOptions {
    fn default() -> Self {
        Self {
            backup_dir: String::from("./backups"),
            interval_ms: Some(3600000), // 1時間
            enabled: Some(true),
            max_backups: None,
            max_age_days: None,
        }
    }
}

/// バックアップ情報
#[derive(Debug, Clone)]
pub struct BackupInfo {
    pub name: String,
    pub path: PathBuf,
    pub created_at: SystemTime,
}

/// バックアップマネージャークラス
pub struct BackupManager {
    last_backup_time: Arc<Mutex<Option<u64>>>,
    last_operation_time: Arc<Mutex<Option<u64>>>,
    backup_dir: String,
    interval_ms: u64,
    enabled: bool,
    db_path: PathBuf,
    max_backups: Option<usize>,
    max_age_days: Option<u64>,
}

impl BackupManager {
    /// 新しいバックアップマネージャーを作成
    pub fn new<P: AsRef<Path>>(db_path: P, options: BackupOptions) -> Result<Self> {
        let backup_dir = options.backup_dir.clone();
        let interval_ms = options.interval_ms.unwrap_or(3600000);
        let enabled = options.enabled.unwrap_or(true);
        let max_backups = options.max_backups;
        let max_age_days = options.max_age_days;

        // バックアップディレクトリが存在しない場合は作成
        if enabled && !Path::new(&backup_dir).exists() {
            fs::create_dir_all(&backup_dir)?;
        }

        Ok(Self {
            last_backup_time: Arc::new(Mutex::new(None)),
            last_operation_time: Arc::new(Mutex::new(None)),
            backup_dir,
            interval_ms,
            enabled,
            db_path: db_path.as_ref().to_path_buf(),
            max_backups,
            max_age_days,
        })
    }

    /// 操作を記録し、必要に応じてバックアップを実行
    pub fn record_operation(&self) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| KijukuError::Other(e.to_string()))?
            .as_millis() as u64;

        {
            let mut last_op = self.last_operation_time.lock().unwrap();
            *last_op = Some(now);
        }

        // 前回のバックアップからの経過時間をチェック
        let should_backup = {
            let last_backup = self.last_backup_time.lock().unwrap();
            match *last_backup {
                None => true,
                Some(last) => now - last >= self.interval_ms,
            }
        };

        if should_backup {
            self.backup()?;
        }

        Ok(())
    }

    /// 手動でバックアップを実行
    pub fn backup(&self) -> Result<String> {
        if !self.enabled {
            return Err(KijukuError::Other("Backup is disabled".to_string()));
        }

        // タイムスタンプを生成
        let now = SystemTime::now();
        let timestamp = now
            .duration_since(UNIX_EPOCH)
            .map_err(|e| KijukuError::Other(e.to_string()))?;

        let datetime = chrono::DateTime::<chrono::Utc>::from(now);
        let timestamp_str = datetime.format("%Y-%m-%dT%H-%M-%S-%3f").to_string();

        let backup_file_name = format!("kijuku-backup-{}.db", timestamp_str);
        let backup_path = Path::new(&self.backup_dir).join(&backup_file_name);

        // SQLite Online Backup APIを使用して安全にバックアップ
        let mut dst_conn = rusqlite::Connection::open(&backup_path)?;
        let src_conn = rusqlite::Connection::open_with_flags(
            &self.db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        let backup = rusqlite::backup::Backup::new(&src_conn, &mut dst_conn)?;
        backup.run_to_completion(5, std::time::Duration::from_millis(250), None)?;

        // 最後のバックアップ時刻を更新
        {
            let mut last_backup = self.last_backup_time.lock().unwrap();
            *last_backup = Some(timestamp.as_millis() as u64);
        }

        // 古いバックアップを削除
        self.cleanup_old_backups()?;

        Ok(backup_path.to_string_lossy().to_string())
    }

    /// バックアップ一覧を取得
    pub fn list_backups(&self) -> Result<Vec<BackupInfo>> {
        let backup_dir = Path::new(&self.backup_dir);
        if !backup_dir.exists() {
            return Ok(Vec::new());
        }

        let mut backups = Vec::new();

        for entry in fs::read_dir(backup_dir)? {
            let entry = entry?;
            let path = entry.path();
            let file_name = entry.file_name();
            let file_name_str = file_name.to_string_lossy();

            if file_name_str.starts_with("kijuku-backup-") && file_name_str.ends_with(".db") {
                let metadata = fs::metadata(&path)?;
                let created_at = metadata.modified().unwrap_or(SystemTime::now());

                backups.push(BackupInfo {
                    name: file_name_str.to_string(),
                    path: path.clone(),
                    created_at,
                });
            }
        }

        // 作成日時の降順でソート
        backups.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        Ok(backups)
    }

    /// 最後のバックアップ時刻を取得
    pub fn get_last_backup_time(&self) -> Option<SystemTime> {
        let last_backup = self.last_backup_time.lock().unwrap();
        last_backup.map(|ms| {
            UNIX_EPOCH + std::time::Duration::from_millis(ms)
        })
    }

    /// 最後の操作時刻を取得
    pub fn get_last_operation_time(&self) -> Option<SystemTime> {
        let last_op = self.last_operation_time.lock().unwrap();
        last_op.map(|ms| {
            UNIX_EPOCH + std::time::Duration::from_millis(ms)
        })
    }

    /// 次回バックアップまでの残り時間（ミリ秒）を取得
    pub fn get_time_until_next_backup(&self) -> Option<u64> {
        let last_backup = self.last_backup_time.lock().unwrap();
        match *last_backup {
            None => Some(0), // 次の操作で即座にバックアップ
            Some(last) => {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .ok()?
                    .as_millis() as u64;
                let elapsed = now - last;
                let remaining = if elapsed < self.interval_ms {
                    self.interval_ms - elapsed
                } else {
                    0
                };
                Some(remaining)
            }
        }
    }

    /// 古いバックアップファイルを削除
    pub fn cleanup_old_backups(&self) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }

        let backups = self.list_backups()?;

        // 降順でソートされている（新しい順）
        let now = SystemTime::now();
        let mut to_delete = Vec::new();

        // 保持期間による削除
        if let Some(max_age_days) = self.max_age_days {
            let max_age = std::time::Duration::from_secs(max_age_days * 24 * 60 * 60);

            for backup in &backups {
                if let Ok(age) = now.duration_since(backup.created_at) {
                    if age > max_age {
                        to_delete.push(backup.path.clone());
                    }
                }
            }
        }

        // 保持数による削除
        if let Some(max_backups) = self.max_backups {
            if backups.len() > max_backups {
                // 新しい順にソートされているので、max_backups以降を削除
                for backup in backups.iter().skip(max_backups) {
                    if !to_delete.contains(&backup.path) {
                        to_delete.push(backup.path.clone());
                    }
                }
            }
        }

        // ファイルを削除
        for path in to_delete {
            fs::remove_file(&path)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;
    use tempfile::TempDir;

    #[test]
    fn test_backup_options_default() {
        let options = BackupOptions::default();
        assert_eq!(options.backup_dir, "./backups");
        assert_eq!(options.interval_ms, Some(3600000));
        assert_eq!(options.enabled, Some(true));
    }

    #[test]
    fn test_backup_manager_creation() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        // 空のDBファイルを作成
        fs::write(&db_path, b"").unwrap();

        let backup_dir = temp_dir.path().join("backups");
        let options = BackupOptions {
            backup_dir: backup_dir.to_string_lossy().to_string(),
            interval_ms: Some(1000),
            enabled: Some(true),
            max_backups: None,
            max_age_days: None,
        };

        let manager = BackupManager::new(&db_path, options);
        assert!(manager.is_ok());
    }

    #[test]
    fn test_manual_backup() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        // 実際のSQLiteデータベースを作成
        {
            let conn = rusqlite::Connection::open(&db_path).unwrap();
            conn.execute("CREATE TABLE test (id INTEGER PRIMARY KEY, data TEXT)", []).unwrap();
            conn.execute("INSERT INTO test (data) VALUES (?1)", ["test data"]).unwrap();
        }

        let backup_dir = temp_dir.path().join("backups");
        let options = BackupOptions {
            backup_dir: backup_dir.to_string_lossy().to_string(),
            interval_ms: Some(1000),
            enabled: Some(true),
            max_backups: None,
            max_age_days: None,
        };

        let manager = BackupManager::new(&db_path, options).unwrap();
        let backup_path = manager.backup().unwrap();

        assert!(Path::new(&backup_path).exists());
        assert!(backup_path.contains("kijuku-backup-"));
        assert!(backup_path.ends_with(".db"));
    }

    #[test]
    fn test_list_backups() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        // 実際のSQLiteデータベースを作成
        {
            let conn = rusqlite::Connection::open(&db_path).unwrap();
            conn.execute("CREATE TABLE test (id INTEGER PRIMARY KEY, data TEXT)", []).unwrap();
            conn.execute("INSERT INTO test (data) VALUES (?1)", ["test data"]).unwrap();
        }

        let backup_dir = temp_dir.path().join("backups");
        let options = BackupOptions {
            backup_dir: backup_dir.to_string_lossy().to_string(),
            interval_ms: Some(1000),
            enabled: Some(true),
            max_backups: None,
            max_age_days: None,
        };

        let manager = BackupManager::new(&db_path, options).unwrap();

        // 2つのバックアップを作成
        manager.backup().unwrap();
        thread::sleep(Duration::from_millis(10));
        manager.backup().unwrap();

        let backups = manager.list_backups().unwrap();
        assert_eq!(backups.len(), 2);

        // 降順でソートされているか確認
        assert!(backups[0].created_at >= backups[1].created_at);
    }

    #[test]
    fn test_get_last_backup_time() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        // 実際のSQLiteデータベースを作成
        {
            let conn = rusqlite::Connection::open(&db_path).unwrap();
            conn.execute("CREATE TABLE test (id INTEGER PRIMARY KEY, data TEXT)", []).unwrap();
            conn.execute("INSERT INTO test (data) VALUES (?1)", ["test data"]).unwrap();
        }

        let backup_dir = temp_dir.path().join("backups");
        let options = BackupOptions {
            backup_dir: backup_dir.to_string_lossy().to_string(),
            interval_ms: Some(1000),
            enabled: Some(true),
            max_backups: None,
            max_age_days: None,
        };

        let manager = BackupManager::new(&db_path, options).unwrap();

        assert!(manager.get_last_backup_time().is_none());

        manager.backup().unwrap();

        assert!(manager.get_last_backup_time().is_some());
    }
}
