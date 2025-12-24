# サンプルコード

kijuku-dbの使い方を示すサンプルコード集です。

## サンプル一覧

### 01-basic-crud.ts

基本的なCRUD操作のサンプルです。

実行される操作：
- データベースの初期化とマイグレーション
- メディアの作成（CREATE）
- メディアの取得（READ）
- メディアの更新（UPDATE）
- メディアの削除（DELETE）

実行方法：
```bash
bun run examples/01-basic-crud.ts
```

### 02-search-filter.ts

検索とフィルタリングのサンプルです。

実行される操作：
- タイトルでの検索
- メディアタイプでのフィルタ
- シリーズでの検索
- 作者での検索
- ソート順の指定
- ページネーション（limit/offset）
- タグでの検索

実行方法：
```bash
bun run examples/02-search-filter.ts
```

### 03-bulk-operations.ts

バルク操作とトランザクションのサンプルです。

実行される操作：
- JSONファイルからデータを読み込み
- 複数のメディアを一括作成
- トランザクションを使った複数操作
- メディアタイプ別の統計表示
- タグ一覧の表示

実行方法：
```bash
bun run examples/03-bulk-operations.ts
```

### 04-cli-usage.sh

CLIツールの使用例です。

実行される操作：
- データベースのマイグレーション
- JSONファイルからデータをインポート
- 様々な条件でのメディア検索

実行方法：
```bash
# まずts-sdkをビルドしてCLIを利用可能にします
cd ts-sdk
bun run build

# CLIをパスに追加（またはフルパスで実行）
export PATH="$PATH:$(pwd)/dist"

# サンプルスクリプトを実行
cd ..
./examples/04-cli-usage.sh
```

## デモデータ

`data/demo-media.json` にデモ用のメディアデータが用意されています。

内容：
- コミック: 6作品（ワンピース、ドラゴンボール、NARUTO、進撃の巨人、鬼滅の刃）
- ビデオ: 5作品（新海誠作品、ジブリ作品）
- 音楽: 4曲（アニメ主題歌）

合計15件のメディア情報が含まれています。

## サンプルの実行順序

初めて使用する場合は、以下の順序でサンプルを実行することをお勧めします：

1. `01-basic-crud.ts` - 基本操作を理解
2. `02-search-filter.ts` - 検索機能を理解
3. `03-bulk-operations.ts` - バルク操作とトランザクションを理解
4. `04-cli-usage.sh` - CLIツールの使い方を理解

## データベースファイルについて

各サンプルは `./data/example.db` にデータベースファイルを作成します（CLIサンプルは `./data/example-cli.db` を使用）。

サンプルを再実行する前にデータベースをクリアしたい場合は、以下のコマンドを実行してください：

```bash
rm -f ./data/example.db ./data/example-cli.db
```

## トラブルシューティング

### モジュールが見つからないエラー

```
Error: Cannot find module '../ts-sdk/src/index.js'
```

このエラーが発生した場合は、ts-sdkをビルドしてください：

```bash
cd ts-sdk
bun run build
cd ..
```

### CLIコマンドが見つからないエラー

CLIツールが見つからない場合は、以下のいずれかの方法で実行してください：

1. パスに追加：
```bash
export PATH="$PATH:$(pwd)/ts-sdk/dist"
```

2. フルパスで実行：
```bash
./ts-sdk/dist/cli.js migrate --db ./data/example.db
```

3. npm linkを使用（開発用）：
```bash
cd ts-sdk
npm link
cd ..
kijuku-cli migrate --db ./data/example.db
```
