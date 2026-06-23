# リモートDB操作の手動テスト手順

SSH接続のモック化が複雑なため、リモートDB操作機能は手動で統合テストを実施します。

## 前提条件

1. SSHでアクセス可能なリモートサーバーが存在すること
2. `~/.ssh/config`にリモートサーバーの設定が追加されていること
3. ローカルにRustバイナリがビルドされていること

## セットアップ

### 1. Rustバイナリのビルドとデプロイ

```bash
mise run deploy
```

これにより、`~/.local/bin/kijuku-cli`にバイナリが配置されます。

### 2. .ssh/config の設定例

```
Host testserver
    HostName example.com
    User testuser
    Port 22
    IdentityFile ~/.ssh/id_rsa
```

### 3. 環境変数の設定（オプション）

`.env`ファイルを作成:

```bash
REMOTE_SSH_HOST=testserver
REMOTE_DB_PATH=~/.local/share/kijuku/test.db
```

## テストケース

### テスト1: 基本的なCRUD操作

```typescript
import { RemoteKijukuDB } from 'kijuku-db';

const db = new RemoteKijukuDB({
  sshHost: 'testserver',
  dbPath: '~/.local/share/kijuku/test.db',
});

// メディア作成
const media = await db.createMedia({
  title: 'リモートテスト作品',
  media_type: 'comic',
  artist: 'テスト作者',
});
console.log('作成:', media);

// メディア取得
const fetched = await db.getMedia(media.id);
console.log('取得:', fetched);

// メディア更新
await db.updateMedia(media.id, { description: '更新されました' });
console.log('更新完了');

// メディア削除
await db.deleteMedia(media.id);
console.log('削除完了');
```

**期待される結果:**
- 全ての操作がエラーなく完了する
- SSH接続が自動的に確立・切断される

### テスト2: 検索機能

```typescript
// 複数のメディアを作成
await db.bulkCreateMedia([
  { title: '作品1', media_type: 'comic', artist: '作者A' },
  { title: '作品2', media_type: 'video', artist: '作者B' },
  { title: '作品3', media_type: 'comic', artist: '作者A' },
]);

// 検索
const results = await db.findMedia({ media_type: 'comic' });
console.log(`検索結果: ${results.length}件`);
```

**期待される結果:**
- コミックタイプのメディアが2件取得できる

### テスト3: タグ操作

```typescript
// タグ作成
const tag = await db.createTag('テストタグ');
console.log('タグ作成:', tag);

// メディアにタグを追加
const media = await db.createMedia({
  title: 'タグテスト作品',
  media_type: 'comic',
});
await db.addTagToMedia(media.id, tag.id);

// メディアのタグを取得
const tags = await db.getMediaTags(media.id);
console.log('メディアのタグ:', tags);
```

**期待される結果:**
- タグが正常に作成され、メディアに関連付けられる
- `getMediaTags`で追加したタグが取得できる

### テスト4: 自動バイナリデプロイ

**準備:**
1. リモートサーバーの`~/.local/bin/kijuku-cli`を削除（または別の場所に移動）

**テスト:**
```typescript
const db = new RemoteKijukuDB({
  sshHost: 'testserver',
  dbPath: '~/.local/share/kijuku/test.db',
});

// 初回実行（自動デプロイが行われる）
const media = await db.createMedia({
  title: '自動デプロイテスト',
  media_type: 'comic',
});
console.log('作成成功:', media);
```

**期待される結果:**
- 初回実行時に自動的にバイナリがリモートに転送される
- メディア作成が正常に完了する

**確認:**
```bash
ssh testserver 'ls -lh ~/.local/bin/kijuku-cli'
```

バイナリが存在し、実行権限があることを確認

### テスト5: エラーハンドリング

```typescript
// 存在しないメディアを取得
try {
  await db.getMedia(99999);
  console.error('エラーが発生すべき');
} catch (error) {
  console.log('期待通りのエラー:', error.message);
}

// 無効なデータで作成
try {
  await db.createMedia({
    title: '',  // 空のタイトル
    media_type: 'invalid_type' as any,
  });
  console.error('エラーが発生すべき');
} catch (error) {
  console.log('期待通りのエラー:', error.message);
}
```

**期待される結果:**
- 適切なエラーメッセージが返される
- SSH接続は正常に切断される

## チェックリスト

- [ ] 基本的なCRUD操作が動作する
- [ ] 検索・フィルタリングが動作する
- [ ] タグ操作が動作する
- [ ] 初回実行時に自動バイナリデプロイが行われる
- [ ] エラー時に適切なメッセージが表示される
- [ ] SSH接続が適切に確立・切断される
- [ ] 複数回の操作で問題が発生しない
- [ ] 異なる設定オプション（workDir、binaryPath）で動作する

## トラブルシューティング

### SSH接続エラー

- `~/.ssh/config`の設定を確認
- SSH鍵の権限を確認（600または400）
- 手動でSSH接続できることを確認: `ssh testserver`

### バイナリ転送エラー

- ローカルの`~/.local/bin/kijuku-cli`が存在することを確認
- リモートの対象ディレクトリに書き込み権限があることを確認

### JSON解析エラー

- リモートのRustバイナリのバージョンが最新であることを確認
- バイナリを再ビルド・再デプロイ

## 注意事項

- テスト用のDBファイルを使用すること（本番データを破壊しないように）
- テスト後はテストデータを削除すること
- SSH接続の認証情報を適切に管理すること
