# Rust RemoteKijukuDB の get_media_tags_bulk が N+1

## ステータス

**解決済み（PR #56・リモートプロトコルへ `getMediaTagsBulk` 操作追加）**

## 現状（旧実装の問題）
Rust SDK の `RemoteKijukuDB`（`rust-sdk/src/remote.rs`）は `KijukuBackend::get_media_tags_bulk` を trait のデフォルト実装（`get_media_tags` のループ）で使用する。1メディアごとにリモートへクエリを発行するため N+1 になる。

## 問題点
- リモート（SSH）接続でメディア数分のクエリ/往復が発生し、メディア一覧のタグ取得で性能劣化する。
- Local（`KijukuDB`）と D1（`D1KijukuDB`）は JOIN 1発の効率的実装で上書き済みだが、Rust の `RemoteKijukuDB` だけが未対応。

## 改善案
- `RemoteKijukuDB` のリモートプロトコルに一括タグ取得操作を追加し、リモート側で JOIN 1発で取得した結果を1往復で返す。
- TS SDK の `RemoteKijukuDB` は既に `operation: 'getMediaTagsBulk'` でリモート CLI の `handle_get_media_tags_bulk`（JOIN）を呼ぶ効率的な経路を実装済み。Rust 側も同等のプロトコル拡張を行う。

## 解決内容（PR #56・253e469）

- `RemoteKijukuDB` に `get_media_tags_bulk` を実装（`rust-sdk/src/remote.rs` の公開メソッド + `KijukuBackend` trait 実装）。`operation: 'getMediaTagsBulk'` でリモート CLI を1往復で呼び、trait のデフォルト実装（`get_media_tags` の N+1 ループ）を上書きした。
- リモート CLI 側は `rust-sdk/src/bin/cli/main.rs` の `getMediaTagsBulk` dispatch が `handle_get_media_tags_bulk`（JOIN 1発）で応答する。改善案の「プロトコル拡張」を TS SDK と同じ形で採用したもの。

## 優先度
low

## 関連
- TASK: TASK-16
- 関連ファイル: `rust-sdk/src/remote.rs`, `rust-sdk/src/backend.rs`
