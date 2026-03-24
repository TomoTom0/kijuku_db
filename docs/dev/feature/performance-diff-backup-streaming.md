# 差分バックアップのメモリ効率改善（ページ単位ストリーミング）

## 現状

`create_diff_file`（Rust: `backup.rs` L944付近）と`createDiffFile`（TypeScript: `backup.ts` L750付近）、
および対応する`apply_diff_to_bytes`/`applyDiffToBytes`関数が、
基底DBと現在のDBの全体をメモリに読み込んでいる。

## 問題点

- 大容量のデータベースファイルに対してOutOfMemoryエラーが発生しうる
- Rust側: `fs::read`で全バイトを`Vec<u8>`に読み込む
- TypeScript側: `fs.readFileSync`で全内容を`Buffer`に読み込む

## 改善案

### Rust

ファイルをSQLiteページ（デフォルト4096バイト）単位で読み込み、ページごとに差分を計算・適用する。

```rust
let mut base_file = File::open(base_path)?;
let mut current_file = File::open(current_path)?;
let mut base_page_buf = vec![0u8; page_size];
let mut current_page_buf = vec![0u8; page_size];
// ループでページごとに読み込んで比較
```

### TypeScript

`fs.createReadStream`を使用してページ単位でストリーミング処理する。

## 優先度

low（現実的なDBサイズでは問題は発生しにくい。巨大なDBを扱う用途が出てきたタイミングで対応）

## 関連

- PR: #30
- Thread IDs: PRRT_kwDOQt7xR852S7Kr（Rust）、PRRT_kwDOQt7xR852S7Kt（TypeScript）
- タスク: TASK-152（Rust）、TASK-153（TypeScript）
- 関連ファイル: rust-sdk/src/backup.rs、ts-sdk/src/backup.ts
