# Unreleased

## Breaking

## Added

- UUIDでのメディア取得APIを追加（Rust/TS両SDK・全バックエンド）: `KijukuDB::get_media_by_uuid` / `getMediaByUuid`、`KijukuBackend` trait、RemoteKijukuDB（stdin操作 `getMediaByUuid`）、Web GUI `GET /api/media/uuid/:uuid`、`GET /api/media` の `uuid` クエリパラメータ
- `MediaFilter` に `uuid`（完全一致）・`uuid_in`（IN句・999件超チャンク分割）を追加（Rust/TS両SDK）

## Changed

## Fixed
