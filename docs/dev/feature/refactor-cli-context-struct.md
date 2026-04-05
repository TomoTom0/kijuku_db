# Rust CLI: コンテキスト構造体の導入

## 現状

`verbose: bool` など、CLIのグローバルオプションを多数の関数に個別の引数として受け渡している。

## 問題点

- 新しいグローバルオプション（例: `--dry-run`, `--config`）を追加するたびに、多くの関数のシグネチャを変更する必要がある
- 関数間で引数リストが長くなり可読性が低下する
- 変更箇所が広範囲に及ぶため、将来のメンテナンスコストが高い

## 改善案

CLIのコンテキストを保持する構造体を定義し、関数間で受け渡す：

```rust
struct CliContext {
    db_path: PathBuf,
    verbose: bool,
    // 将来の追加フィールドはここに集約
}
```

## 優先度

low

## 関連

- PR: #38
- Thread ID: PRRT_kwDOQt7xR8545fm0
- タスク: TASK-177
- 関連ファイル: rust-sdk/src/bin/cli.rs
