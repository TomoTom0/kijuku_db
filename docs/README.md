# ドキュメント構成

kijuku_dbプロジェクトのドキュメント一覧と管理方針。

## ディレクトリ構成

```
docs/
├── README.md                  # このファイル（ドキュメント構成の管理）
├── api.md                     # API仕様書（全エンドポイント・型定義）
├── DATABASE_SETUP.md          # DBセットアップガイド
├── PERFORMANCE.md             # パフォーマンスガイド
├── TESTING.md                 # テスト戦略・実行方法
├── manual-testing-remote.md   # リモートDB手動テスト手順
├── changelog/
│   └── unreleased.md          # リリース前の変更履歴
├── design/
│   ├── decisions.md           # 設計上の意思決定記録
│   ├── init/                  # 初期設計ドキュメント
│   └── update-exist*.md       # update-exist機能の設計
├── dev/
│   └── feature/               # 技術的負債・将来の改善案
├── examples/
│   └── ts-sdk/                # TypeScript SDKサンプルコード説明
└── usage/
    ├── cli/
    │   └── README.md          # kijuku-cli 利用ガイド（サブコマンド一覧）
    └── sdk/
        ├── README.md          # SDK選択ガイド
        ├── ts/README.md       # TypeScript SDK利用ガイド
        └── rust/README.md     # Rust SDK利用ガイド
```

## 常時更新が必要なドキュメント

### 必須更新

| ドキュメント | 更新タイミング |
|-------------|--------------|
| `docs/changelog/unreleased.md` | 機能追加・バグ修正・破壊的変更のたびに追記 |
| `docs/api.md` | APIインターフェース（型・エンドポイント・フィールド）変更時 |

### 条件付き更新

| ドキュメント | 更新タイミング |
|-------------|--------------|
| `docs/DATABASE_SETUP.md` | マイグレーション追加・DBスキーマ変更時 |
| `docs/PERFORMANCE.md` | パフォーマンスに影響する実装変更時 |
| `docs/TESTING.md` | テスト構成・テスト実行方法変更時 |
| `docs/usage/cli/README.md` | CLIサブコマンドの追加・変更・削除時 |
| `docs/usage/sdk/ts/README.md` | TypeScript SDKの公開API・型定義・使用方法変更時 |
| `docs/usage/sdk/rust/README.md` | Rust SDKの公開API・型定義・使用方法変更時 |
| `docs/examples/ts-sdk/` | TypeScript SDKのサンプルコードが古くなった時 |
| `docs/design/decisions.md` | 重要な設計判断を行った時 |
| `docs/dev/feature/` | PRレビューで即対応しない技術的負債を記録する時 |

## 更新不要なケース

- 内部実装のみの変更（公開APIに影響しない場合）
- タイポ修正・コメント修正
- テストコードのみの変更

## CHANGELOGへの記載ルール

`docs/changelog/unreleased.md`には以下のセクションで記載：

- **Added**: 新機能
- **Fixed**: バグ修正
- **Changed**: 既存機能の変更（破壊的でない）
- **Breaking**: 破壊的変更

各エントリにはタスクID（例: `TASK-135`）を付記する。
