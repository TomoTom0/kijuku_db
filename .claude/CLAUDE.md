# kijuku_db プロジェクト固有の設定

## ビルド・デプロイ

### ローカルデプロイ（推奨）

```bash
./scripts/dev/deploy-local.sh
```

このスクリプトは以下を実行します:
- Rust SDKのリリースビルド (`cargo build --release`)
- バイナリを `~/.local/kijuku-db/bin/` にコピー
- `~/.local/bin/kijuku-cli` にシンボリックリンクを作成

ドキュメントはビルド時にバイナリに埋め込まれるため、ドキュメントを更新した場合は必ずデプロイスクリプトを実行してください。

### 個別ビルド

```bash
# Rust SDK
cd rust-sdk && cargo build --release

# TypeScript SDK
cd ts-sdk && pnpm run build
```

### テスト

```bash
# Rust SDK
cd rust-sdk && cargo test

# TypeScript SDK
cd ts-sdk && pnpm run test
```

## プロジェクト構成

- `rust-sdk/`: Rust SDK（kijuku-cli バイナリを含む）
- `ts-sdk/`: TypeScript SDK
- `docs/`: ドキュメント（ビルド時にRustバイナリに埋め込まれる）

## 設計原則

### SDKが先、CLIはSDKの薄いラッパー

機能は必ずSDK（`KijukuDB` メソッド）として実装し、CLIはそれを呼び出すだけ。

- 新機能はまず Rust SDK の `KijukuDB` に実装する
- TypeScript SDK の `KijukuDB`（ローカル）と `RemoteKijukuDB`（リモート）にも同様に公開する
- CLI サブコマンドは SDK メソッドを呼ぶだけの薄いラッパーとして実装する
- SDK に存在しない機能が CLI にあるのは実装漏れであり、即座に修正する

## タスク管理

**tmコマンドを使用してタスクを管理する**

### タスク起票の方針

1. **goalの設定**:
   - 既に起票されているタスクにgoalが未設定の場合、作業開始前に `tm update <ID> --goal "..."` でgoalを設定する
   - Claude Codeが新規起票する際は必ず `--goal` を指定する
2. **積極的な起票**: 軽微でない作業は必ずタスクとして起票する
3. **事後記録**: 起票せずに作業完了した場合、関連タスクがなければ記録用に起票し即完了する
4. **フィルタの活用**: タスク検索時は必ずステータスやバージョンのフィルタを使用する（例: `tm ls -s wip`, `tm ls --tbd -s done`）

### 基本的な使い方

```bash
# タスク一覧を表示（フィルタを積極的に活用）
tm list                    # デフォルト: todo, wipのみ
tm ls -s wip              # wipのみ
tm ls -s done             # 完了タスク
tm ls --tbd               # CHANGELOG未記載タスク
tm ls --tbd -s done       # CHANGELOG未記載の完了タスク

# 既存タスクにgoalを設定
tm update <タスクID> --goal "達成すべきゴール"

# 新しいタスクを作成（goalを必ず指定）
tm new "タスクのタイトル" --body "詳細説明" --goal "達成すべきゴール" --status todo

# 記録用タスク（作業完了後に起票）
tm new "実施した作業" --body "作業内容" --goal "達成したゴール" --status done

# タスクのステータスを変更
tm update <タスクID> --status wip

# タスクを完了としてマーク（changelog用の情報をbodyに記載）
tm finish <タスクID> --body "変更内容の説明"

# バージョンを設定（changelog記載後）
tm release <タスクID> --version 0.5.5

# タスクをクローズ（キャンセル・不要になった場合）
tm close <タスクID> --body "理由"
```

### タスクの進行順序（order）

**やむを得ない場合を除き、タスク起票時は原則orderを設定する**

- 起票前に `tm list` で既存タスクのorderを確認し、作業順序に合った値を `--order` で指定する

```bash
# 進行順序が必要な場合のみorderを設定
tm new "Task 1" --order 1
tm new "Subtask" --order 1-1
tm new "Task 2" --order 2

# 既存タスクのorderを更新
tm update <タスクID> --order 1-2

# orderが不要になったら解除
tm update <タスクID> --order null

# ソート順の指定
tm list --sort order    # order昇順（デフォルト、orderなしは後ろ）
tm list --sort id       # ID順
tm list --sort created  # 作成日時順
```

**orderの特徴**:
- フォーマット: `1`, `2`, `1-1`, `1-2`, `1-2-1` など（ハイフン区切りで親子関係を表現）
- 小数入力可能（例: `1.5`）で、保存時に自動正規化
- `todo`, `wip` ステータスのみorderを保持、それ以外では自動的に `null`
- orderが未設定のタスクは `tm list` で後ろに表示される

### タスクステータス

- `todo`: 未着手のタスク
- `wip`: 作業中のタスク（Work In Progress）
- `pending`: 保留中のタスク
- `long`: 長期タスク
- `done`: 完了したタスク
- `closed`: クローズされたタスク

### リリース準備ワークフロー

1. **CHANGELOG未記載タスクの確認**: `tm ls --tbd` で version='tbd' のタスクを全て表示
2. **タスク詳細の確認**: `tm get <ID>` で各タスクのbodyから情報を取得
3. **CHANGELOGへの記載**: 各タスクの情報を`docs/changelog/unreleased.md`に追記
4. **バージョンの確定**: `tm release <ID> --version x.y.z` で各タスクにバージョンを設定
5. **リリース確認**: `tm ls --released --version x.y.z` でリリース済みタスクを確認

