# エラーハンドリング・環境変数・パフォーマンス

## エラーハンドリング

## SQLiteエラー

better-sqlite3は同期APIのため、エラーは直接スローされます。

**主なエラーコード:**

| エラー | 説明 | 対処方法 |
|-------|------|---------|
| `SQLITE_CANTOPEN` | ファイルを開けない | パス確認、権限確認 |
| `SQLITE_READONLY` | 読み取り専用DBへの書き込み | `readonly: false`で開く |
| `SQLITE_CONSTRAINT` | 制約違反（UNIQUE, NOT NULL等） | データを確認 |
| `SQLITE_ERROR` | 一般的なSQLエラー | SQLクエリを確認 |

**エラーハンドリング例:**

```typescript
try {
  const media = db.createMedia({
    title: 'テスト',
    media_type: 'comic',
    path: '/duplicate/path.cbz',
  });
} catch (error) {
  if (error instanceof Error) {
    if (error.message.includes('UNIQUE constraint failed')) {
      console.error('パスが重複しています');
    } else if (error.message.includes('NOT NULL constraint failed')) {
      console.error('必須フィールドが不足しています');
    } else {
      console.error('エラー:', error.message);
    }
  }
}
```

## トランザクションエラー

```typescript
try {
  db.transaction(() => {
    db.createMedia({ title: 'A', media_type: 'comic' });
    throw new Error('意図的なエラー');
  });
} catch (error) {
  console.error('トランザクションがロールバックされました');
}
```

---

## 環境変数

SDK の target/DBパス解決（`resolveTarget`・設計 §13）が読み込む環境変数:

| 変数名 | 説明 | デフォルト |
|--------|------|-----------|
| `KIJUKU_READ_SOURCE` | 読み取り元（`prod` / `stg`）。指定時は `KIJUKU_TARGET` より優先される | 未指定 |
| `KIJUKU_TARGET` | 接続対象（`prod` / `stg`） | `stg` |
| `KIJUKU_DB_PATH` | prod 側のDBファイルパス（target=prod 時） | なし（要明示指定） |
| `KIJUKU_STG_DB_PATH` | stg 側のDBファイルパス（target=stg 時） | なし（要明示指定） |

**解決優先順位**（CLI 引数が常に勝つ・`ts-sdk/src/config.ts` の `resolveTarget`）:

- read-source: `cliReadSource` > `KIJUKU_READ_SOURCE`(env) > 未指定
- target: readSource（指定なら優先） > `cliTarget` > `KIJUKU_TARGET`(env) > `stg`
- dbPath: `cliDb` > target 別 env（prod=`KIJUKU_DB_PATH` / stg=`KIJUKU_STG_DB_PATH`）。
  **いずれも無い場合はエラー**（DB配置は明示指定必須・暗黙の既定パスは持たない）。明示指定された相対パスは利用者の選択としてそのまま使われる

target=prod は readonly・マイグレーションスキップ、target=stg は RW として導出される。

> **Rust CLI の場合:** 同じ環境変数を読み込みます（`--db` / `--target` / `--read-source` フラグが指定された場合はフラグが優先されます）。
>
> ```bash
> kijuku-cli --db ./kijuku.db --verbose update-exist
> ```

---

## パフォーマンスに関する注意

## インデックスの活用

以下のフィールドにはインデックスが設定されています：
- `title_id`, `artist_id`（完全一致検索用）
- `media_type`
- `series`
- `source`
- `media_type, created_at`（複合インデックス）

これらのフィールドでの検索は高速です。

## 部分一致検索の注意

`title`, `artist`, `series`での部分一致検索（LIKE `%value%`）はインデックスを使用しないため、大量データでは遅くなる可能性があります。

## トランザクションの活用

複数の書き込み操作を行う場合は、`transaction()`を使用することでパフォーマンスが向上します。

```typescript
// 遅い
for (let i = 0; i < 1000; i++) {
  db.createMedia({ title: `Media ${i}`, media_type: 'comic' });
}

// 速い
db.transaction(() => {
  for (let i = 0; i < 1000; i++) {
    db.createMedia({ title: `Media ${i}`, media_type: 'comic' });
  }
});

// さらに速い
db.bulkCreateMedia(
  Array.from({ length: 1000 }, (_, i) => ({
    title: `Media ${i}`,
    media_type: 'comic',
  }))
);
```

---

## 参考リンク

- [README.md](../README.md) - プロジェクト概要
- [設計ドキュメント](../design/decisions.md) - 設計決定事項
- [サンプルコード](../../examples/README.md) - 使用例
