# バックアップ設計

## 概要

kijuku-db のバックアップ機能は以下の3つの柱で構成される。

1. **手動バックアップ**: 任意のタイミングでスナップショットを作成・復元
2. **自動バックアップ**: 操作後に一定間隔で自動取得
3. **粗密間引き（RetentionPolicy）**: 直近は細かく、古いほど間引いて保存

---

## 設定ファイル

### ファイルの場所

| 優先度 | パス | 備考 |
|---|---|---|
| 1（最低） | デフォルト値 | コード組み込み |
| 2 | `~/.local/config/kijuku-db/config.toml` | グローバル設定 |
| 3 | `{cwd}/kijuku-db-config.toml` | 実行場所の設定 |
| 4 | `{db階層}/kijuku-db-config.toml` | DB階層の設定（最優先ファイル） |
| 5（最高） | `--config {path}` 引数 | 手動指定 |

優先度が高いものが低いものを上書きしてマージする。
リモートDB使用時もDB階層の config.toml は読み込まれる。

### null によるリセット

各キーに `null` を指定すると、そのキーをデフォルト値にリセットできる。
下位の設定を意図的に無効化したい場合に使用する。

```toml
[backup.auto]
enabled = null  # グローバルの設定を無視してデフォルト値（true）に戻す
```

### 設定項目（config.toml）

```toml
[backup]
# バックアップ機能全体の有効/無効
enabled = true
# バックアップファイルの保存先ディレクトリ
# 省略時: {db階層}/backup/
backup_dir = "~/.local/share/kijuku-db/backup"

[backup.auto]
# 自動バックアップの有効/無効
enabled = true
# バックアップをトリガーする時間間隔（ミリ秒）
interval_ms = 3600000

[backup.tmp]
# restore前自動退避ファイルの保持期間（秒）。デフォルト: 7日
retention_secs = 604800

[backup.retention]
# 自動バックアップの粗密保持ポリシー（省略時はデフォルト）
# tiers は max_age_secs の昇順で定義する
[[backup.retention.tiers]]
max_age_secs = 3600      # ~1時間
keep_interval_secs = 0   # 全保持

[[backup.retention.tiers]]
max_age_secs = 86400     # ~1日
keep_interval_secs = 3600  # 1時間に1つ

[[backup.retention.tiers]]
max_age_secs = 604800    # ~1週間
keep_interval_secs = 86400  # 1日に1つ

[[backup.retention.tiers]]
max_age_secs = 2592000   # ~1ヶ月 (30日)
keep_interval_secs = 604800  # 1週間に1つ

[[backup.retention.tiers]]
max_age_secs = 7776000   # ~3ヶ月 (90日)
keep_interval_secs = 2592000  # 1ヶ月に1つ

[[backup.retention.tiers]]
max_age_secs = 31536000  # ~1年 (365日)
keep_interval_secs = 7776000  # 3ヶ月に1つ

[[backup.retention.tiers]]
max_age_secs = 9999999999  # 1年以降
keep_interval_secs = 31536000  # 1年に1つ
```

---

## バックアップ種別：フル vs 差分

### 使い分け方針

DBの規模に関わらず効率的なストレージ利用を実現するため、
取得頻度の高い直近バックアップには差分バックアップを用いる。

| tier | 期間 | バックアップ種別 | 理由 |
|---|---|---|---|
| recent | ~1時間 | **差分** | 頻度が最も高くストレージ効率が重要 |
| hourly | 1時間~1日 | **差分** | 同上 |
| daily | 1日~1週間 | **フル** | 差分の基底となる。1日1回取得 |
| weekly〜 | 1週間以降 | **フル** | 頻度が低くフルで十分 |

- 1日以内（recent + hourly）: 当日の daily フルバックアップを基底とした差分
- 1日以降（daily〜yearly）: フルバックアップ
- 手動バックアップ: 常にフル（明示的なスナップショット）

### 差分バックアップの仕組み

SQLite はページ（デフォルト 4096 bytes）単位でデータを管理する。
外部依存なしでページレベルの差分を実装する。

**差分ファイル形式**:
```
Header:
  base_id          (18 bytes)  基底フルバックアップのタイムスタンプID
  page_size        (4 bytes)   SQLite ページサイズ
  total_pages      (4 bytes)   基底バックアップ時点の総ページ数
  changed_pages    (4 bytes)   差分ページ数

For each changed page:
  page_number      (4 bytes)
  page_data        (page_size bytes)
```

**取得手順**:
1. 基底フル（当日の daily バックアップ）を特定
2. 現在のDBと基底フルをページごとに比較
3. 変更のあったページのみを差分ファイルに書き出す

**復元手順**:
1. 対応する基底フルをロード
2. 差分ファイルの変更ページを上書き適用
3. 結果が復元後のDBとなる

### 基底フルの削除制約

差分バックアップは基底フルへの参照を持つため、
**参照している差分が全て pruned になるまで基底フルは削除できない**。

RetentionPolicy が daily フルを削除対象と判定しても、
そのフルを参照する recent/hourly 差分が残っている場合は削除を保留する。

---

## バックアップディレクトリ構成

```
backup/
  tmp/                          # システムが自由に使用（restore前の自動退避など）
    {stem}.{timestamp}-pre_restore.db
  manual/                       # 手動バックアップ（常にフル）
    {stem}.{timestamp}.db
    {stem}.{timestamp}-{label}.db
  auto/                         # 自動バックアップ（フラット）
    {stem}.{timestamp}.db       # フルバックアップ (.db)
    {stem}.{timestamp}.diff     # 差分バックアップ (.diff)
  meta/
    auto-records.csv            # 間引き済み含む全自動バックアップ履歴
```

### ファイル名規則

- `{stem}`: DBファイル名から拡張子を除いた部分（例: `kijuku`）
- `{timestamp}`: `YYYYMMDDHHMMSS-mmm`（18文字固定）。バックアップのIDを兼ねる
- `{label}`: 手動バックアップの任意ラベル。英数字・アンダースコア・ハイフンのみ
- 拡張子: フルは `.db`、差分は `.diff`

例:
```
auto/kijuku.20260323000000-000.db    # daily フル（差分の基底）
auto/kijuku.20260323010000-000.diff  # hourly 差分（基底: 20260323000000-000）
auto/kijuku.20260323020000-000.diff  # hourly 差分（基底: 20260323000000-000）
manual/kijuku.20260323131045-789.db
manual/kijuku.20260323131045-789-before_import.db
```

種別はディレクトリで、フル/差分は拡張子で区別する。

`tmp/` の自動退避ファイルは用途を識別しやすくするため `-pre_restore` サフィックスを付ける。

### meta/auto-records.csv

自動バックアップの履歴を記録する。間引き済みのエントリも保持し、
「いつ何が取得されて、いつ削除されたか」を追跡できる。

```csv
id,created_at,tier,type,base_id,size_bytes,status,pruned_at
20260323000000-000,2026-03-23T00:00:00.000Z,daily,full,,8388608,kept,
20260323010000-000,2026-03-23T01:00:00.000Z,hourly,diff,20260323000000-000,48576,kept,
20260323020000-000,2026-03-23T02:00:00.000Z,hourly,diff,20260323000000-000,12288,pruned,2026-03-24T00:00:00.000Z
20260322000000-000,2026-03-22T00:00:00.000Z,daily,full,,8200000,kept,
```

| カラム | 説明 |
|---|---|
| `id` | タイムスタンプ（ファイル名と対応） |
| `created_at` | バックアップ作成日時（ISO 8601） |
| `tier` | 作成時点の tier（recent/hourly/daily/weekly/monthly/quarterly/yearly） |
| `type` | `full` または `diff` |
| `base_id` | diff の場合、基底フルバックアップの ID（full の場合は空） |
| `size_bytes` | バックアップファイルサイズ |
| `status` | `kept`（ファイル存在）または `pruned`（間引き済み） |
| `pruned_at` | 間引き日時（ISO 8601）、kept の場合は空 |

---

## 保持ポリシー（デフォルト）

| 経過時間 | 保持間隔 | tier名 | バックアップ種別 |
|---|---|---|---|
| ~1時間 | 全保持 | recent | 差分 |
| 1時間~1日 | 1時間に1つ | hourly | 差分 |
| 1日~1週間 | 1日に1つ | daily | フル |
| 1週間~1ヶ月 | 1週間に1つ | weekly | フル |
| 1ヶ月~3ヶ月 | 1ヶ月に1つ | monthly | フル |
| 3ヶ月~1年 | 3ヶ月に1つ | quarterly | フル |
| 1年~ | 1年に1つ | yearly | フル |

手動バックアップ（`manual/`）は保持ポリシーによる間引きの対象外。

---

## バックアップ読み込み・復元 API

### バックアップを参照（DBは変更しない）

バックアップファイルを読み取り専用で開き、通常のクエリを実行する。
差分バックアップが指定された場合、内部で基底フルに差分を適用してから開く。

```rust
// Rust
db.get_media_from_backup(id, &selector)
db.find_media_from_backup(&filter, options, &selector)
db.get_all_tags_from_backup(&selector)
db.get_media_tags_from_backup(media_id, &selector)
db.get_media_attribute_from_backup(media_id, key, &selector)
db.get_media_attributes_from_backup(media_id, &selector)
```

### 復元

復元前に現在の状態を `tmp/` へ自動バックアップしてから上書きする。
`tmp/` 内のファイルは一定期間（デフォルト: 7日）後に自動削除される。

```
restore 実行時の処理順序:
  1. 現在のDBを tmp/{stem}.{timestamp}-pre_restore.db に保存
  2. 差分バックアップが指定された場合: 基底フル + 差分を合成してDBを構築
     フルバックアップが指定された場合: そのまま適用
  3. 結果を現在のDBに上書き
  4. (次回 cleanup 実行時) tmp/ 内の期限切れファイルを削除
```

```rust
// Rust: SQLite Online Backup API で接続を維持したまま復元
db.restore(&selector)

// TypeScript: WAL checkpoint → close → ファイルコピー → reopen
db.restore(selector)
```

### BackupSelector

バックアップを選択する条件。auto/manual/tmp の横断検索が可能。
差分バックアップも通常通り選択でき、復元時は自動的に基底フルを参照する。

```rust
BackupSelector::Latest           // 最新
BackupSelector::Nth(n)           // 新しい順でn番目（0が最新）
BackupSelector::Before(time)     // 指定日時より前の最新
BackupSelector::After(time)      // 指定日時より後の最古
BackupSelector::ClosestTo(time)  // 指定日時に最も近い
```

---

## 実装状況

| 機能 | 状態 |
|---|---|
| 手動バックアップ（ラベルあり/なし） | 実装済み |
| 自動バックアップ（interval_ms） | 実装済み |
| RetentionPolicy による間引き | 実装済み |
| バックアップからの読み込み | 実装済み |
| restore（pre_restore 自動退避） | 実装済み |
| 差分バックアップ（recent/hourly tier） | 実装済み |
| ディレクトリ構成（tmp/manual/auto/meta/） | 実装済み |
| config.toml の読み込み・マージ | 実装済み |
| auto-records.csv の書き込み | 実装済み |
| BackupSelector のディレクトリスコープ指定 | 実装済み |
| tmp/ 自動削除（retention_secs） | 実装済み |
