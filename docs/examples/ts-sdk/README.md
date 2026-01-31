# TypeScript SDK サンプル集

このディレクトリには、kijuku-db TypeScript SDKの使い方を示すサンプルコードが含まれています。

## サンプル一覧

### 基本編

1. **[01-getting-started.md](./01-getting-started.md)** - 最初のステップ
   - インストール方法
   - 基本的な初期化
   - 最も簡単な使い方

2. **[02-basic-crud.md](./02-basic-crud.md)** - CRUD操作
   - メディアの作成（Create）
   - メディアの取得（Read）
   - メディアの更新（Update）
   - メディアの削除（Delete）

3. **[03-search-and-filter.md](./03-search-and-filter.md)** - 検索とフィルタリング
   - タイトル・作者での検索
   - メディアタイプでのフィルタ
   - シリーズでの検索
   - 複合条件での検索

### 応用編

4. **[04-tag-management.md](./04-tag-management.md)** - タグ管理
   - タグの作成と取得
   - メディアへのタグ付け
   - タグでの検索
   - タグ一覧の取得

5. **[05-bulk-operations.md](./05-bulk-operations.md)** - バルク操作
   - 複数メディアの一括作成
   - トランザクションの使い方
   - パフォーマンスの最適化

<!--
6. **[06-advanced-queries.md](./06-advanced-queries.md)** - 高度なクエリ
   - ソート（昇順・降順）
   - ページネーション（limit/offset）
   - 複数条件の組み合わせ
   - パフォーマンスのヒント

### データ連携編

7. **[07-import-export.md](./07-import-export.md)** - インポート/エクスポート
   - JSON形式でのインポート
   - CSV形式でのインポート
   - データのエクスポート
   - バックアップ/リストア

8. **[08-remote-operations.md](./08-remote-operations.md)** - リモート操作
   - SSH経由でのDB操作
   - リモートDBのセットアップ
   - 自動デプロイ機能
-->

## 実行方法

各ドキュメント（.md）内のコードスニペットを実行する方法：

1. **コードをコピー**: 各ドキュメント内のTypeScriptコードブロックをコピー
2. **ファイルに保存**: 新しい.tsファイルを作成して貼り付け（例: `test.ts`）
3. **実行**: 以下のいずれかの方法で実行

```bash
# Node.jsで実行
node test.ts

# Bunで実行（高速）
bun test.ts

# TypeScriptで実行
tsx test.ts
```

または、自分のプロジェクト内で直接コードを使用することもできます。

## プロジェクトへの組み込み

外部プロジェクトでSDKを使用する場合は、以下のようにインポートします：

```typescript
import { KijukuDB } from 'kijuku-db';

const db = new KijukuDB('./data/myapp.db');
db.migrate();

// あとは自由に使えます
const media = db.createMedia({
  title: '作品名',
  media_type: 'comic',
});
```

## ヘルプとサポート

- [API仕様書](../../api.md) - 全メソッドの詳細な仕様
- [README.md](../../../README.md) - プロジェクト概要
- [SDK利用ガイド](../../usage/sdk/ts/README.md) - TypeScript SDK詳細ガイド

## トラブルシューティング

### データベースファイルが見つからない

```typescript
// 相対パスまたは絶対パスを指定
const db = new KijukuDB('./data/kijuku.db');
// または
const db = new KijukuDB('/absolute/path/to/kijuku.db');
```

### マイグレーションエラー

```typescript
// 初回はマイグレーションが必要です
db.migrate();
```

### 型エラー

```typescript
// MediaInput型を明示的に指定
import { MediaInput } from 'kijuku-db';

const input: MediaInput = {
  title: '作品名',
  media_type: 'comic', // 'comic' | 'video' | 'music'
};
```
