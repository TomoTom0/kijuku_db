//! prod（本番DB）への一時 RW 書込スコープ（設計 §5.2・TASK-56 C1）。
//!
//! `promote`(stg→prod)・(b)管理操作・prod 復旧 が prod に書き込む際の共通基盤。
//! LLM の恒久 prod RW 接続は持たせず、各操作の内部でのみ本スコープ経由で prod を
//! 一時的に RW 扱いする（設計 §5.2）。
//!
//! - `ProdRwScope::acquire`: `<prod>.lock` の排他ロック取得（同一 prod の同時書込を直列化・§5.3）
//!   と pre-stash 強制（`create_pre_promote_snapshot`・§7.2・`enabled` と独立して常時実行）を完了する。
//! - `Drop`: 排他ロックを解放（`StgLock` と同一の RAII パターン・ファイルは削除しない）。
//!
//! 呼び出し側は本スコープを保持したまま `copy_db_online(_, scope.prod_path())`(promote)
//! または RW の `KijukuDB::open_with_options(scope.prod_path(), …)`((b)操作) で prod に書き込む。

use crate::backup::{BackupManager, BackupOptions};
use crate::error::{KijukuError, Result};
use crate::stg_session::lock_path;
use fs2::FileExt;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

/// prod への一時 RW 書込スコープ（設計 §5.2・TASK-56 C1）。
///
/// 取得時に `<prod>.lock` の排他ロック取得と pre-stash 強制を完了し、`Drop` で排他ロックを
/// 解放する（RAII）。**恒久 prod RW 接続は持たない**。呼び出し側は本スコープを保持したまま
/// prod に書き込む。
pub struct ProdRwScope {
    lock_file: File,
    prod_path: PathBuf,
    pre_stash_path: Option<PathBuf>,
}

impl ProdRwScope {
    /// prod の排他ロック取得 + pre-stash 強制（設計 §5.2/§7.2/§7.3）。
    ///
    /// * `prod_path` - 書込対象の prod DB ファイルパス
    /// * `backup_opts` - pre-stash 先の BackupManager 設定。`None` は
    ///   `BackupOptions { enabled: Some(false), .. }`（auto/manual/meta ディレクトリを作らず
    ///   `tmp/` のみ）と等価。`enabled` 値にかかわらず pre-stash は常時実行
    ///   （`create_pre_promote_snapshot` 本体に `enabled` gate なし・backup.rs:529-539）。
    ///
    /// 別セッションが同じ prod を保持中なら `KijukuError::ProdBusy`。
    /// pre-stash 失敗時はロックを解放してからエラーを返す（partial state を残さない）。
    pub fn acquire(prod_path: &Path, backup_opts: Option<BackupOptions>) -> Result<Self> {
        // 排他ロック取得（stg_session::StgLock::acquire と同一・設計 §5.3/§15-11）。
        // 同一 prod ファイル上の同時 promote/(b) を直列化する。
        let lock_file_path = lock_path(prod_path);
        if let Some(parent) = lock_file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let lock_file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&lock_file_path)?;
        lock_file.try_lock_exclusive().map_err(|_| KijukuError::ProdBusy {
            prod_path: prod_path.display().to_string(),
            holder_pid: None,
        })?;

        // pre-stash 強制（§7.2・enabled と独立して常時実行）。prod を触る前に前倒し（§7.3）。
        let bm_opts = backup_opts.unwrap_or_else(|| BackupOptions {
            enabled: Some(false),
            ..Default::default()
        });
        let pre_stash_path = match BackupManager::new(prod_path, bm_opts)
            .and_then(|bm| bm.create_pre_promote_snapshot())
        {
            Ok(path) => path.map(PathBuf::from),
            Err(e) => {
                // pre-stash 失敗時はロックを解放して partial state を残さない。
                let _ = lock_file.unlock();
                return Err(e);
            }
        };

        Ok(Self {
            lock_file,
            prod_path: prod_path.to_path_buf(),
            pre_stash_path,
        })
    }

    /// 書込対象の prod パス（`copy_db_online` の dst や RW `KijukuDB` の open に使用）。
    pub fn prod_path(&self) -> &Path {
        &self.prod_path
    }

    /// 今回の pre-stash パス（設計 §8 即時復旧の戻し先）。tmp 配下の
    /// `{stem}.{ts}-pre_promote.db`。呼び出し側は監査/復旧用に保持する。
    pub fn pre_stash_path(&self) -> Option<&Path> {
        self.pre_stash_path.as_deref()
    }
}

impl std::fmt::Debug for ProdRwScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProdRwScope")
            .field("prod_path", &self.prod_path)
            .field("pre_stash_path", &self.pre_stash_path)
            .finish()
    }
}

impl Drop for ProdRwScope {
    fn drop(&mut self) {
        // StgLock と同一: best-effort で解放（失敗は無視・OS もプロセス終了時に解放する）。
        // ファイルは削除せず残す（unlink 競合回避・設計 §15-11）。
        let _ = self.lock_file.unlock();
    }
}
