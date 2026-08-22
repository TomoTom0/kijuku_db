# Unreleased

## Breaking

## Added

- UUIDでのメディア取得APIを追加（Rust/TS両SDK・全バックエンド）: `KijukuDB::get_media_by_uuid` / `getMediaByUuid`、`KijukuBackend` trait、RemoteKijukuDB（stdin操作 `getMediaByUuid`）、Web GUI `GET /api/media/uuid/:uuid`、`GET /api/media` の `uuid` クエリパラメータ
- `MediaFilter` に `uuid`（完全一致）・`uuid_in`（IN句）を追加（Rust/TS両SDK）

## Changed

## Fixed

- `uuid_in` / `id_in` / `exclude_ids` / `tag_ids` のIN句を `json_each` による1パラメータ化に変更（Rust/TS両SDK）。従来のチャンク分割はIN句をOR結合して単一ステートメントに全件バインドしており、SQLiteのパラメータ数上限（999）やD1のバインド上限（100）を超えるリストでエラーになる実装だった（レビュー指摘により判明）
