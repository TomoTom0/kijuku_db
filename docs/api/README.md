# kijuku-db API仕様書

kijuku-db（TypeScript SDK / Rust SDK / kijuku-cli）のAPI正本。エリア別に分割しているため、実装変更時は該当エリアのファイルを更新する（更新タイミングの規則は [../README.md](../README.md) 参照）。

| ファイル | 内容 |
|---------|------|
| [kijuku-db.md](./kijuku-db.md) | ローカル `KijukuDB`（コンストラクタ・マイグレーション・メディアCRUD・タグ・属性・ハッシュ・トランザクション・サムネイル・ファイル存在チェック・ファイル操作・trash・その他） |
| [backup-sync.md](./backup-sync.md) | バックアップ・DB複製（sync/discard）・差分・promote gate・監査ログ（prod/stg 運用） |
| [remote.md](./remote.md) | `RemoteKijukuDB`（SSH リモート・session pool・stg運用・ファイル転送・バックアップメタ） |
| [rust.md](./rust.md) | Rust SDK（初期化・メソッド対応表） |
| [web-gui.md](./web-gui.md) | Web GUIサーバー（`startServer`・HTTP APIエンドポイント） |
| [types.md](./types.md) | 型定義（正本） |
| [errors.md](./errors.md) | エラーハンドリング・環境変数・パフォーマンスに関する注意 |

SDKの選択・利用ガイドは [../usage/sdk/](../usage/sdk/) を、CLIの利用ガイドは [../usage/cli/README.md](../usage/cli/README.md) を参照。
