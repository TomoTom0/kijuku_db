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

`mise run deploy` でリモートにもバイナリを配置したい場合、`.env`ファイルに設定:

```bash
REMOTE_SSH_HOST=testserver
```

（テストコード側の接続先は `RemoteConfig`（`sshHost`・`dbPath` 等）で指定するため、`.env` はデプロイ先用）

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

// プールされたSSH接続を閉じる（one-shotスクリプトでは必須。
// 未呼び出しだと TCP ソケットがイベントループを保持し、プロセスが終了しない）
await db.disconnect();
```

**期待される結果:**
- 全ての操作がエラーなく完了する
- 初回RPCでSSH接続が確立され、以降の連続RPCで同じ接続が再利用される（`db.connectCount` が1のまま）
- `disconnect()` の後、プロセスが正常に終了する

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

await db.disconnect();
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

await db.disconnect();
```

**期待される結果:**
- タグが正常に作成され、メディアに関連付けられる
- `getMediaTags`で追加したタグが取得できる

### テスト4: 自動バイナリデプロイ（バージョン比較ベース・TASK-69）

クライアント接続時にリモート CLI が常に最新へ自動更新される。実体 `~/.local/kijuku-db/bin/kijuku-cli` + symlink `~/.local/bin/kijuku-cli` 構成。毎 RPC の先頭で `getServerVersion` → `local > remote` 比較 →（古ければ）デプロイ。

共通セットアップ:
```typescript
const db = new RemoteKijukuDB({
  sshHost: 'testserver',
  dbPath: '~/.local/share/kijuku/test.db',
});
// 各シナリオのone-shot実行では、RPC後に await db.disconnect() で
// プールされたSSH接続を閉じること（未呼び出しだとプロセスが終了しない）
```

#### シナリオ1: 初回デプロイ

**準備:** リモートの `~/.local/bin/kijuku-cli` と `~/.local/kijuku-db/bin/` を削除。

**テスト:** 任意の remote RPC（例: `createMedia`）を実行。

**期待される結果:**
- 初回実行時に `~/.local/kijuku-db/bin/kijuku-cli`（実体）と `~/.local/bin/kijuku-cli`（symlink）が作成される
- メディア作成が正常に完了する

**確認:**
```bash
ssh testserver 'ls -lh ~/.local/kijuku-db/bin/kijuku-cli ~/.local/bin/kijuku-cli'
ssh testserver 'kijuku-cli --version'
```

#### シナリオ2: バージョンアップ

**準備:** ローカルのバージョンを進めて `mise run deploy`（ビルド＋配置）→ リモートは旧バージョンのまま。

**テスト:** 任意の remote RPC を実行。

**期待される結果:** リモート CLI が新バージョンへ更新され、`ssh testserver 'kijuku-cli --version'` がローカルと一致する。

#### シナリオ3: バージョン同等（skip）

**準備:** リモートがローカルと同バージョン（シナリオ2 実行後の状態）。

**テスト:** 任意の remote RPC を実行。

**期待される結果:** デプロイは走らず RPC が高速に完了する。バイナリ mtime が変化しない（`ssh testserver 'stat -c %Y ~/.local/kijuku-db/bin/kijuku-cli'` で前後比較）。

#### シナリオ4: ダウングレード保護

**準備:** リモートの方が新しい（ローカルを旧バージョンでビルド・リモートは新バージョン）。

**テスト:** 任意の remote RPC を実行。

**期待される結果:** デプロイは走らず（`local > remote` でない）、リモートの新しいバイナリが維持され RPC が成功する。

#### シナリオ5: 古いバイナリからの後方互換（自動回復）

**準備:** リモートに TASK-69 前のバイナリ（`getServerVersion` 未対応）を配置。

**テスト:** 任意の remote RPC を実行。

**期待される結果:** `getServerVersion` が失敗（null）→ デプロイ実行 → 次回 RPC は新バイナリで成功（自動回復）。初回だけ二重 RPC（デプロイ + 本_rpc）が走る。

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

await db.disconnect();
```

**期待される結果:**
- 適切なエラーメッセージが返される
- `disconnect()` でSSH接続が正常に切断される

### テスト6: prod/stg 構成（target・stgDbPath・sync/discard）

```typescript
const db = new RemoteKijukuDB({
  sshHost: 'testserver',
  dbPath: '~/.local/share/kijuku/test.db',          // prod DB パス
  stgDbPath: '~/.local/share/kijuku/test.stg.db',   // 省略時は dbPath から <stem>.stg.db を導出
});

// prod(RO) → stg(RW) のフル複製（sync・サーバ側でコピー実行）
const synced = await db.sync();
console.log('sync:', synced.prodPath, '->', synced.stgPath);

// 既定の target は stg（書込は導出・指定された stg DB に対して行われる）
await db.createMedia({ title: 'stg側で作成', media_type: 'comic' });

// stg を破棄して prod から再構築（discard・stg 側の変更は失われる）
await db.discard();

await db.disconnect();
```

**期待される結果:**
- sync/discard がリモート側の prod/stg パスで実行され、両パスが返る
- 書込は stg にのみ反映され、prod（dbPath）は変更されない
- `target: 'prod'` を指定したインスタンスでは書込操作がエラーになる（readonly + migrate skip）

### テスト7: diffWithProd・observe・promote（gate評価と stg→prod 反映）

```typescript
// prod と stg（現在DB）の差分（promote 判断用）
const diff = await db.diffWithProd({});
console.log(diff.summary);

// 機械的 promote gate 評価（スキーマ不一致・外部キー違反・件数差分上限等）
const result = await db.observe();
console.log('gate:', result.passed);

// gate 通過後に stg→prod へ反映（gate 不合格時は Error）
const outcome = await db.promote();
console.log('pre-stash:', outcome.preStashPath);

await db.disconnect();
```

**期待される結果:**
- diffWithProd の差分要約（media/tags/mediaTags/attributes/hashes）が取得できる
- observe の gate 結果（`passed`・各 `checks`）が取得できる
- promote 後に prod へ stg の変更が反映され、pre-stash（`tmp/` 配下の prod スナップショット）パスが返る
- gate 不合格の状態で promote するとエラーになる

### テスト8: ファイル操作・trash（mediaRoot サンドボックス）

```typescript
const db = new RemoteKijukuDB({
  sshHost: 'testserver',
  dbPath: '~/.local/share/kijuku/test.db',
  mediaRoot: '~/media',  // ファイル操作APIのサンドボックス境界（未設定だとエラーで拒否される）
});

// dry-run ファースト（既定 apply=false で計画のみ返る）
const plan = await db.mediaCp('a/sample.jpg', 'a/copy.jpg');
console.log(plan.steps);

// trash（論理削除）: 移動・一覧・復元・物理削除
await db.moveToTrash('a/old.jpg', 'delete');
const entries = await db.listTrash();
await db.restoreFromTrash(entries[0].id);
await db.purgeTrash(undefined, true);  // dry-run で物理削除計画のみ確認

await db.disconnect();
```

**期待される結果:**
- `apply` 省略時はファイル変更が行われず、操作計画（steps）が返る
- trash への移動・一覧・復元が media root 配下で完結する
- media root 外のパス指定はエラーで拒否される

### テスト9: バックアップ・pre-stash・長操作タイムアウト・監査ログ

```typescript
// バックアップ（backup/restore/sync/discard/diff*/observe/promote は長操作で、
// 省略時は DB サイズから適応的にタイムアウトが算出される）
const path = await db.backup('manual-test');
console.log(await db.listBackups());

// pre-stash（promote・prod直接(b)操作直前の prod スナップショット）一覧
console.log(await db.listPreStashes());

// タイムアウトの明示指定（例: 30分）も可能
await db.restore({ type: 'latest' }, 30 * 60_000);

await db.disconnect();
```

監査ログは TS SDK の `RemoteKijukuDB` には公開されていないため、CLI（`--db host:path`）で確認する:

```bash
kijuku-cli --db testserver:~/.local/share/kijuku/test.db audit-logs --operation promote
```

**期待される結果:**
- バックアップがリモートの `backup/` 配下に作成され、一覧に表示される
- promote・prod直接(b)操作の後に pre-stash と監査ログが記録される
- 長操作がタイムアウトせずに完了する（大規模 DB でも適応的タイムアウトが機能する）

## チェックリスト

- [ ] 基本的なCRUD操作が動作する
- [ ] 検索・フィルタリングが動作する
- [ ] タグ操作が動作する
- [ ] 初回実行時に自動バイナリデプロイが行われる
- [ ] エラー時に適切なメッセージが表示される
- [ ] 連続RPCでSSH接続が再利用され（`connectCount` が1のまま）、`disconnect()` で切断される
- [ ] one-shotスクリプトが `disconnect()` 後に正常終了する
- [ ] 異なる設定オプション（binaryPath・stgDbPath・target・mediaRoot）で動作する
- [ ] sync/discard で prod→stg の複製・再構築が動作する
- [ ] diffWithProd・observe・promote の gate 評価と stg→prod 反映が動作する
- [ ] media root 配下のファイル操作（cp/mv/sync）と trash が動作する
- [ ] バックアップ・pre-stash・監査ログが記録・参照できる

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
