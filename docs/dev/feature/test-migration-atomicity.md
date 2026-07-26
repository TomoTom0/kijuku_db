# migration v4/v5 の原子性テストが未実装

## 現状

TASK-43 P1-C2 で TS 側の migration（`ts-sdk/src/migration.ts`）case 4/5 を `db.transaction()` で wrap し、スキーマ再作成 DDL 群と `INSERT INTO schema_version` を原子的に適用するようにした（設計 §7.3）。`PRAGMA foreign_keys = OFF/ON` は tx 外に置き、Rust `rust-sdk/src/migration.rs`（L113/L189 の `unchecked_transaction()`）と同じ境界に揃えた。

しかし、この wrap の原子性（DDL 群の途中で失敗した際に rollback されること）を検証する自動テストが未実装。TASK-51 C5 で「migration v4/v5 原子性（失敗注入 rollback）」として計画していたが、後述の理由で実装を見送った。

## 問題点

`schema/schema.sql` が v1〜v6 の `schema_version` INSERT を全て含む（L129-134）。このため新規 DB は `migrate()` 1回で常に v6 になり、**case 4/5 の段階的移行コードは新規 DB では一切実行されない**。

case 4/5 を走らせるには `currentVersion=3` の DB が必要だが、現在の schema.sql（v6 最終形: uuid NOT NULL UNIQUE 含む）からは v3 DB を再現できない。case 4 は「uuid 列の追加 + media テーブル再作成」を行うが、schema.sql で作った DB は既に uuid を持つため、`ALTER TABLE media ADD COLUMN uuid` が失敗する。

v3 DB を用意するには過去版 schema.sql の復元、または v3 相当スキーマをコードで構築するテストヘルパが必要で、メンテナンス負荷が高い。Rust 側も `include_str!("../schema.sql")`（migration.rs L19）で同構造のため同じ制約。

## 改善案

1. v3 相当のスキーマを構築するテストヘルパ（`buildLegacyV3Db()` 的）を用意し、case 4/5 への移行で意図的に失敗を注入（例: media の path 重複で `media_new.path UNIQUE` 違反）して rollback を検証する。
2. または `applyMigration` を export して直接 case 4 を呼べるようにし、v3 media テーブルに対する再作成の原子性を検証する。

いずれも「v3 DB の再現」が前提。なお、better-sqlite3 / rusqlite の transaction が例外で rollback することは `ts-sdk/test/integration/transaction.test.ts` で汎用検証済みであり、case 4/5 が `db.transaction()` を使うことでその保証が適用されることは論理的に保証される。本負債は case 4/5 固有の DDL 失敗時の rollback を実行時検証したい場合に対応する。

## 優先度

medium

## 関連

- タスク: TASK-43 P1-C5（TASK-51）
- 設計: docs/design/db-protection.md §7.3
- 関連ファイル: ts-sdk/src/migration.ts（case 4: L77-162, case 5: L164-199）, rust-sdk/src/migration.rs（L113, L189）, schema/schema.sql（L129-134）
