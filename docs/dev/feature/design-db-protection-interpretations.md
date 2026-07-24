# DB保護設計: 実装時の解釈の余地（TASK-44 仕上げで確認）

設計書 `docs/design/db-protection.md` の実装照合（TASK-44 P2 仕上げ）で検出された、
設計解釈の余地・実害なしの懸念点。いずれもデフォルト設定では発現せず、本番DB保護の有効性には影響しない。
照合の結果、重大な乖離はなく、promote/(b)gate/pre-stash/復旧の中核機能は設計書に忠実に実装済み。

## D1: backup_manager=None 時の migrate 前 snapshot スキップ

### 現状
migrate() は `if let Some(bm) = backup_manager { bm.create_pre_migrate_snapshot() }` で、
`DBOptions.backup=None` で開いた場合は snapshot がスキップされる（Rust `lib.rs:378-384` / TS `index.ts:257-262`）。

### 問題点
設計 §7.3「migrate 前 snapshot 必須」。ただし prod 読込経路はそもそも migrate をスキップ（§5.1）。
stg/admin 経路で明示的に backup を無効化して開いた場合のみ snapshot が取れない。

### 改善案
- 設計書に「snapshot 必須は backup 有効前提」と注記、または
- backup 無効時でも migrate 前に最低限のファイル退避を強制

### 優先度
low（実運用で stg/admin を backup 無効で開くことは前提外）

## D2: pre-stash と通常 backup tmp の保持期間が同じ

### 現状
`backup/tmp/` 配下の pre-stash（pre_promote/pre_restore/pre_migrate）と通常 backup テンポラリが、
同じ `tmp_retention_secs`（デフォルト7日・`backup.rs:1138-1162` / `backup.ts:951-967`）で管理される。

### 問題点
設計 §7.4「pre-stash は通常 tmp より長く保持」。現状は同じ期間。pre-stash の復旧窓が通常 tmp と同一。

### 改善案
- pre-stash 用に別の保持期間（より長い）を設ける

### 優先度
low（7日で十分な復旧窓・実害なし）

## D3: dry-run は呼出側選択（設計 §9.2 vs §15-13 の表現揺れ）

### 現状
(b) 操作の `dryRun`/`dry_run` は呼出側パラメータ。trash + pre-stash は構造的に強制（`cli.rs:1895-1938`）。

### 問題点
設計 §9.2「dry-run + trash + pre-stash を環境が強制」vs §15-13「dry-run ファースト（呼出側選択）」で
表現揺れ。trash + pre-stash は強制だが dry-run は呼出側選択。

### 改善案
- 設計書 §9.2 を「trash + pre-stash は強制・dry-run は推奨（§15-13）」に整理

### 優先度
low（保護は trash + pre-stash で担保・実害なし）

## D4: Rust CLI に promote サブコマンドがない

### 現状
Rust CLI の `Commands` enum に `Promote` がない。`observe`/`DiffProdStg`/`Restore`/`ListPreStashes` は
サブコマンドがあるが、`promote`（と sync/discard）は stdin operation のみ（`cli.rs:2016/2179`）。
TS cli.ts には `runPromote` がある。

### 問題点
CLI の非対称。ただし設計上「状態変更操作（promote/sync/discard）は RemoteKijukuDB 経由（SSH → stdin）」が
主経路のため、意図的設計の可能性。

### 改善案
- TASK-64「CLI統合 C1: Rust CLI へ SSH remote 移植」で promote サブコマンド追加を検討、または
- 設計書 §14.1 の「stg sync/promote/discard を CLI コマンド」との整合を確認

### 優先度
medium（TASK-64 で扱う）

## 関連
- 設計書: `docs/design/db-protection.md`
- タスク: TASK-44（本番DB保護 P2）・TASK-64（CLI統合 C1）
- 関連ファイル: `rust-sdk/src/{lib,backup,bin/cli}.rs` / `ts-sdk/src/{index,backup,remote}.ts`
