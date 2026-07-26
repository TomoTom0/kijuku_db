# copy_db_online と copy_db_to の委譲リファクタ

## 現状

`rust-sdk/src/backup.rs` に DB コピーの実装が2系統ある:

- `BackupManager::copy_db_online(src, dst)` / `try_copy_db_online(src, dst)`（関連関数・TASK-49 C3 で新設）
  - src を `SQLITE_OPEN_READ_ONLY` で開く。busy リトライは固定間隔 `[50,100,200,500,1000]ms`。
- `BackupManager::copy_db_to(&self, dst)` / `try_copy_db_to(&self, dst)`（インスタンスメソッド・従来）
  - src = `self.db_path`。busy リトライは `BackupManager` の `busy_timeout_ms` / `retry_intervals_ms` 設定を使用。

`try_copy_db_online` と `try_copy_db_to` は Online Backup 本体（`Backup::new` + `run_to_completion`）ほぼ同一だが、
C3 実装時はインスタンス側への委譲リファクタを見送り、重複実装を許容した（sync/promote の基盤を優先）。

## 問題点

- Online Backup 本体ロジックが重複しており、片方の修正がもう片方に反映されないリスクがある。
- リトライ間隔の来源が異なる（固定 vs 設定）ため、意図的とはいえ挙動の差を追跡しづらい。

## 改善案

`try_copy_db_to(&self, dst)` を `copy_db_online(&self.db_path, dst)` へ内部委譲する。

```rust
fn try_copy_db_to(&self, dst: &Path) -> Result<()> {
    Self::try_copy_db_online(&self.db_path, dst)
}
```

その上で、リトライ間隔を設定化するか固定に統一するかを設計§4.5/§5.3 と照らして決定する。
（運用上の prod RO reader と promote writer の競合は固定間隔で十分な可能性が高い）

## 優先度

low（重複はあるが両者正常動作しており機能的影響なし）

## 関連

- TASK-49 (P1-C3: copy_db_online 新設)
- TASK-50 (P1-C4: sync) — `KijukuDB::replicate_db` から `copy_db_online` を呼出
- 関連ファイル: `rust-sdk/src/backup.rs`
