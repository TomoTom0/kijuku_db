# thumbnail テストの PATH 環境変数操作の DI 化

## 現状
`rust-sdk/src/thumbnail.rs` の2テスト（`test_update_thumbnail_success_generates_and_updates_db`, `test_update_thumbnail_video_success`）が、偽の ffmpeg/convert バイナリを認識させるため `std::env::set_var("PATH", ...)` でプロセス全体の PATH を一時的に書き換えている。

`set_var` はプロセスの environ をグローバルに変更するため、テストの並列実行でデータ競合（Rust 1.78+ では unsafe）が起き、非決定な失敗を招く。

現状は `PATH_TEST_LOCK: Mutex` で該当テスト同士を直列化し、フル並列実行時の失敗を防いでいる（PR#49 で対応）。

## 問題点
- `Mutex` 直列化は thumbnail テスト同士の競合は防ぐが、`set_var` 自体のプロセス全体への影響（他スレッドが environ を読む瞬間のデータ競合）は理論上残る。
- `ThumbnailOptions` が `Serialize/Deserialize + Default` の公開APIのため、ffmpeg/convert パスの DI 用フィールドを安易に追加すると、テスト専用設定が公開APIに混入する。

## 改善案
- サムネイル生成のコア関数（`Command::new("ffmpeg")` / `"convert"` を呼ぶ箇所）を、ffmpeg/convert のフルパスを引数で受け取る形にリファクタリング。
- 公開API（`ThumbnailOptions`）を汚さず、内部 trait またはテスト専用のビルダー/コンストラクタでパスを注入。
- テストはフルパスを直接渡し、`set_var` を完全に廃止。

## 優先度
low（現状の Mutex 直列化で実用的には安定。製品コードへの影響なし）

## 関連
- PR: #49
- 関連ファイル: rust-sdk/src/thumbnail.rs
- 関連タスク: TASK-14
