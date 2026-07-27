# Unreleased

## Breaking

### (b) 制限操作（mediaMv/purgeTrash/restore）を stg で拒否（TASK-59）

- 設計 §9 の操作階層化を実装。`mediaMv`/`purgeTrash`/`restore` を破壊的/全床上書きの **(b) stg 制限操作** に分類し、stg（既定 `--target stg`）では環境が**拒否**するようになった（設計 §9.2）
- 影響: これらの操作は stg では実行できなくなった。prod 直接 `--target prod` で dry-run+trash+pre-stash gate 付きで実行するか、stg のリセットは `sync`（prod→stg 再複製）を使用すること
- `deleteMedia` は DB-only のまま (a)（FS 物理削除は `moveToTrash`(a) → `purgeTrash`(b) の trash 流で扱い・新規 FS 削除ロジックなし）

## Fixed

### PR#56 レビュー指摘のパストラバーサル・TOCTOU・競合を修正（TASK-73/74/75/76）

- **TASK-73**: `purgeTrash` が呼出し側提供 ID（`../../important` や絶対パス）で `.trash` 外へ脱出して任意ディレクトリを削除できた脆弱性を修正。ID を生成エントリ名（単一コンポーネント・`.trash` の direct child）に検証（Rust `ensure_safe_trash_id` / TS `ensureSafeTrashId`）
- **TASK-74**: cp/mv/sync の dst 解決が lexical のみで、`media_root/link -> /outside` のような既存 symlink 配下の新規パスを通って root 外へ書き込めた脆弱性を修正。dst 用に `resolve_destination_within_root`（最近傍既存祖先の canonicalize + root 配下再検査）を追加して file_ops の dst 解決に適用（Rust/TS 両方）
- **TASK-75**: TS `replicateDb` が copy **後**に prod revision を算出していたため copy 中の prod 更新で記録 revision と実際の snapshot が乖離し、observe が drift を見逃して stale な promote を許す TOCTOU を修正。Rust `replicate_db` と同順序（copy 前算出）に統一
- **TASK-76**: `promote` が observe（gate 評価）→ ロック取得の順で、間に別 promoter が prod を更新すると stale な gate 結果で prod を上書きする競合を修正。prod 排他ロック取得後に gate を再評価し、不合格なら prod を触らず拒否（Rust/TS 両方）

**ファイル:** `rust-sdk/src/{trash,media_path,file_ops,lib}.rs`, `ts-sdk/src/{trash,media_path,file_ops,index}.ts`, 各 unit テスト

## Added

### 操作階層化 (a)/(b) と (b) 操作の prod 直接 gate（TASK-59 P2-C4）

本番DB保護 P2。設計 §9 の **(a) stg許可 / (b) stg制限** の操作階層化と、(b) 操作の prod 直接実行時の機械的 gate を実装（人間承認不要・環境が担保・§3.2/§5.2/§9.2）。

- **操作分類**（`classify_operation`・破壊度基準）: (b)=`mediaMv`/`purgeTrash`/`restore`。(a)=CRUD/tag/attr/hash/upload + `mediaCp`/`mediaSync`/`moveToTrash`/`restoreFromTrash`/`backup`/`setBackupLabel`/`setBackupNote`。`migrate` は §15-5 で P3（TASK-45）扱いまで現状維持（接続時 auto-migrate + pre_migrate_snapshot）
- **dispatch gate**（`cli.rs` stdin ハンドラ）: stg+(b) 拒否・prod RO 読込で (a)/(b) 書込拒否・`--target prod` で (b) は prod 直接経路へ（readonly backend を使わず `ProdRwScope` で prod RW を一時取得）
- **(b) gate** = dry-run + trash + pre-stash（操作種別で適用サブセット切替・§15-13 解決）: DB 層（`restore`）は `ProdRwScope` の排他ロック + pre-stash（§8 即時巻き戻し）→ prod RW 上書き。FS 層（`mediaMv`/`purgeTrash`）は dry-run ファースト + trash 経由（既存 `file_ops`/`trash`）
- `restore` に `dryRun` param 追加: `true` で復元差分（`diff_with_backup`）を返し prod 不変。Rust CLI/TS Remote で wire
- **TS parity**: `assertNotStgRestricted` で (b) メソッドを stg で拒否（Local 直接利用の構造的保護）。prod 直接 (b) は Remote（Rust CLI）経由が主経路

**ファイル:** `rust-sdk/src/bin/cli.rs`（`classify_operation`・dispatch gate・`handle_b_operation_prod`・restore dryRun）, `rust-sdk/tests/cli_integration_test.rs`, `ts-sdk/src/{index,remote}.ts`, `ts-sdk/test/unit/readonly-guard.test.ts`, `ts-sdk/test/integration/backup.test.ts`, `docs/design/db-protection.md`（§14.6/§15-13）, `docs/usage/cli/README.md`

### promote の backupOpts を CLI/TS wire で受け渡し可能にし pre-stash 先をカスタマイズ(TASK-62)

- promote の `backupOpts` が `None` 固定だった（`BackupOptions` 系が serde 未実装のため）を解除し、Rust CLI / TS Local / TS Remote の全経路で受け渡し可能にした
- `BackupOptions`/`RetentionPolicy`/`RetentionTier` に `Serialize`/`Deserialize`（`rename_all="camelCase"`・TS camelCase wire と整合）を追加。TS `BackupOptions` を Rust と完全 parity 化（`busyTimeoutMs`/`retryIntervalsMs` を追加）
- `backupOpts.backupDir` で pre-stash（promote 前 prod の退避・§8 即時復旧の戻し先）の作成先をカスタマイズ可能（§7.2）。pre-stash 自体は `backupOpts`・`enabled` にかかわらず常時実行
- CLI: stdin `promote` 操作の params に `backupOpts` を追加。TS CLI には `--backup-opts <json>` フラグを追加（`as` キャスト不使用の型ガードで安全に parse）

**ファイル:** `rust-sdk/src/{backup,bin/cli}.rs`, `ts-sdk/src/{backup,index,remote,cli}.ts`, `rust-sdk/tests/{promote_test,cli_integration_test}.rs`, `ts-sdk/test/integration/promote.test.ts`, `docs/usage/cli/README.md`

### kijuku-cli に --version / -V フラグを追加(TASK-20)

- `kijuku-cli --version`（または `-V`）で現在のバージョンを表示できるようにした（`CARGO_PKG_VERSION` を参照）
- これまで CLI にバージョン表示機能がなく、デプロイ後のバージョン確認が不可能だった

**ファイル:** `rust-sdk/src/bin/cli.rs`, `rust-sdk/tests/cli_integration_test.rs`

### CLI利用ガイドに --backend / bulk-load / D1 の記載を追加(TASK-21)

- v0.2.0 の D1バックエンド機能が `docs/usage/cli/README.md` に未記載だったのを修正
- グローバルオプション `--backend`（`local` / `d1`）と `bulk-load` サブコマンド（ローカル→D1 バルクロード、`--chunk-size` / `--verify-only`）を追記
- D1 利用の前提（事前の `wrangler login` + `D1_ACCOUNT_ID` / `D1_DATABASE_ID` 環境変数）も明記

**ファイル:** `docs/usage/cli/README.md`

### バックアップ復元判断支援（read-only参照・差分表示・事後ラベル/メモ）(TASK-25,26,27)

復元先を判断する手段がなく実用性に欠けていた backup 復元を見直し、3機能を追加:

**機能1: バックアップの read-only 参照基盤(TASK-25)**
- 差分バックアップ(.diff)を読み取り専用で開けるよう `with_backup_db` を拡張（基底フルから一時フルを再構成、コールバック終了後に一時ファイルと WAL 副産物を削除）
- `BackupSelector::by_id(id)` を追加（タイムスタンプ文字列でバックアップを直接指定）。CLI `restore --id`、stdin/Remote selector に `byId` を追加
- hash 系 `*_from_backup` を追加（既存 media/tag/attribute と対応化）

**機能2: バックアップとの差分表示(TASK-26)**
- `diff_with_backup(selector, options)` を追加（media/tags/media_tags/attributes/hashes を比較し added/removed/changed を算出）
- `added`=復元で復活、`removed`=復元で失われる、`changed`=復元で上書き
- `DiffOptions.detail` で `summaryOnly`/`limited{n}`/`full` を切替
- CLI `diff-backup` サブコマンド、stdin/Remote `diffBackup`

**機能3: 事後ラベル/メモ付与(TASK-27)**
- 既存バックアップに後からラベル・メモを付与（ファイル名は変更せずサイドカー `backup/meta/backup-meta.json`）
- `set_backup_label(id, label?)` / `set_backup_note(id, note?)` / `get_backup_meta(id)`
- `listBackups` はサイドカーを優先マージ（`labelSource`/`note` を返す）、間引き時に orphan エントリを掃除
- CLI `set-backup-label` / `set-backup-note` サブコマンド、stdin/Remote 対応

**ファイル:** `rust-sdk/src/{backup,lib,diff,bin/cli,remote,types,attribute,hash,tag}.rs`, `ts-sdk/src/{backup,index,diff,remote,types,attribute,hash,tag}.ts`, `docs/design/backup.md`

### 読込先切替 `--read-source` と readonly 書込拒否ガードを追加(TASK-52)

本番DB保護の読込先切替（TASK-43 P1 残A）。prod を readonly で安全に読める読込専用セッションを追加し、書込操作を構造的に拒否。

- CLI `--read-source prod|stg`（+ 環境変数 `KIJUKU_READ_SOURCE`）。優先順位: CLI > env > デフォルト `stg`。read-source 指定時は target に折り畳む（方式A）、`prod` で readonly open + migrate スキップ
- readonly セッションの書込拒否: CLI は `is_write_operation` で stdin の書込操作を事前拒否、TS SDK は `assertWritable` で32書込メソッドにガード（CLI を経由しないローカル `KijukuDB` 直接利用があるため TS 側ガードが必須）
- `effective_target()` で read_source をリモート伝達（リモート CLI に `--target` として渡し readonly を導出・設計 §3.5/§13）

**ファイル:** `rust-sdk/src/{remote,bin/cli}.rs`, `ts-sdk/src/{config,index}.ts`, `rust-sdk/tests/cli_integration_test.rs`, `ts-sdk/test/unit/target-resolution.test.ts`

### prod/stg 差分表示 `diff-prod-stg`（要約 + LLM explanation prompt）を追加(TASK-53)

本番DB保護の差分可視化（TASK-43 P1 残B）。prod(RO) と stg（現在DB）を比較し、promote 判断に必要な差分を stg 編集視点で表示する。

- `KijukuDB::diff_with_prod(prod_path, options)`: prod を readonly 別接続で開き snapshot 比較（設計 §15-8）。`compute_diff(current=prod, backup=stg)` でセマンティクス反転（added=stg新規=promoteでprod追加・removed=prodのみ=promoteでprod削除・changed=両方で異なる=promoteで上書き）
- CLI `diff-prod-stg` サブコマンド（`--prod`/`--detail` 既定 `limited=20`/`--summarize`/`--prompt`）。`--summarize` でテーブル別件数・分布、`--prompt` で LLM レビュー用 explanation prompt を stdout 出力（コピペ可能）
- 新データ構造 `ProdStgDiffSummary`（counts/totals/distribution）+ 純粋関数 `summarize_diff`/`build_diff_explanation_prompt`。分布は media（byType/byArtistTop/byFlagExist）・media_tags（byTagTop）・attributes（byKey）・hashes（byFilenameTop）、上位 N=10
- リモート operation `diffProdStg`、TS SDK ミラー（`diffWithProd`/`summarizeDiff`/`buildDiffExplanationPrompt`）。純粋関数はクライアント側計算（設計 §15-15 決定）

**ファイル:** `rust-sdk/src/{lib,diff,remote,bin/cli}.rs`, `ts-sdk/src/{types,diff,index,remote}.ts`, `rust-sdk/tests/{diff_prod_stg_test,cli_integration_test}.rs`, `ts-sdk/test/{unit/diff-summary,unit/diff-prompt,integration/diff-prod-stg}.test.ts`

### 機械的 promote gate `observe` を追加(TASK-54)

本番DB保護の変更後健全性確認（TASK-43 P1 残C）。stg 編集内容が prod に promote してよいか、客観的かつ機械的に判定する gate（人間 gate でない・設計 §3.4/§4.4）。

- `KijukuDB::observe(prod_path, options)`: `diff_with_prod` と同等の差分を計算し `summarize_diff` で要約した上で gate を評価。self は stg（RW）、prod は readonly 別接続（設計 §15-8）。gate 用 DB 検査（integrity/FK/schema_version/golden）は stg 接続を1ロックで実行
- 評価する gate（`diff::evaluate_gate` 純粋関数）: `PRAGMA integrity_check` == ok / 外部キー整合性（`foreign_key_check` 空 + FK 有効）/ prod-stg の `schema_version` 一致 / 件数・差分上限（`summary.totals` vs `GateConfig.max_added|max_removed|max_changed`）/ 運用者定義 golden assertion（SQL 結果を `expected_min`/`expected_max` で検証）
- `ObserveResult.passed` が全 gate 合格を表す（promote 可否の客観判定）。fast-promote は導入せず全変更フル gate 統一（安全性優先）
- CLI `observe` サブコマンド（`--prod`/`--detail` 既定 `summary`/`--max-added`/`--max-removed`/`--max-changed`/`--json`）。リモート operation `observe`、TS SDK ミラー（`observe`/`evaluateGate`/`GateConfig`/`GoldenAssertion`/`ObserveResult`）

**ファイル:** `rust-sdk/src/{lib,diff,remote,bin/cli}.rs`, `ts-sdk/src/{types,diff,index,remote}.ts`, `rust-sdk/tests/observe_test.rs`

### stg 排他（global advisory lock）と sync 元 prod revision 記録を追加(TASK-55)

本番DB保護 P1 の最終ピース（TASK-43・設計 §15-11）。複数セッション/LLM の stg 同時編集による衝突と、prod が同期後に更新された stale な stg の promote を防止する。排他方式は global advisory lock（session別stg は不採用）。

- **排他ロック**: `StgLock::acquire(stg_path)`（RAII・新モジュール `stg_session`）。書込系が `<stg>.lock` の排他ロックを取得。Rust は `fs2` の advisory lock（プロセス終了/クラッシュで OS が自動解放・stale なし）。TS は PID ベースロックファイル（`O_EXCL` 作成 + holder PID 生存確認で stale 回収・新規依存なし）。二重取得は `KijukuError::StgBusy` / `StgBusyError`
- **sync（revision 記録）**: `replicate_db`/`replicateDb` が先頭でロック取得し、コピー後に sync 元 prod の指紋 `ProdRevision`（schema_version + media/tags/media_tags/media_attributes/media_hashes の件数/max-id）を `<stg>.meta.json` に原子書き込み（tmp→rename・backup-meta パターン）。`compute_prod_revision` で算出
- **stg 編集セッション**: `KijukuDB::acquire_stg_lock()` / `acquireStgLock()` がインスタンス lifetime でロック保持（`close()`/Drop で解放）。CLI `build_backend` は writable（!readonly）セッションで自動取得
- **revision 活用（observe gate）**: observe が新 gate `prod_sync_revision` を追加。`<stg>.meta.json` の記録 revision と現 prod revision を比較し、drift（sync 後の prod 更新）があれば不合格 → re-sync 要求。meta なし（旧 stg）は後方互換でスキップ

**ファイル:** `rust-sdk/src/{stg_session,error,lib,bin/cli}.rs`, `ts-sdk/src/{stg-session,index}.ts`, `rust-sdk/Cargo.toml`, `rust-sdk/tests/stg_session_test.rs`, `ts-sdk/test/integration/stg-lock.test.ts`

### リモート SSH RPC に適応的タイムアウトを導入（TS parity）(TASK-67)

長時間操作（backup/sync 等）で固定タイムアウトが切れてハング・誤爆していた問題を、DB サイズからタイムアウトを適応的に算出する方式（TS `calcBackupTimeoutMs` の Rust parity 移植）で解消。

- **適応的タイムアウト**（`calc_backup_timeout_ms`）: rusqlite バックアップ設定（750,000ページ/バッチ・10sスリープ）と HDD 50MB/s 想定から算出。`copyTime + sleepTime` に MARGIN=2、最低 60s。`u32::MAX` で飽和（オーバーフローなし）。TS `calcBackupTimeoutMs` と定数・計算とも厳密一致
- **適用対象**（長操作8種）: `backup`/`restore`/`sync`/`discard`/`diffBackup`/`diffProdStg`/`observe`/`promote`。`restore` は現在DB退避+復元の2段階で ×2（TS remote.ts L1162 parity）。`sync`/`discard` は prod パス基準
- **タイムアウト解決優先順位**: 呼び出し元の明示（`--timeout-ms`） > 適応的算出（長操作） > デフォルト 30s（短操作・TS parity）。upload/download は固定 120s
- **DB サイズ取得**（`remote_db_size`）: `stat -c %s` で取得（失敗時 0 → 60s）。stat 自体のハング対策にセッションタイムアウトを事前設定
- CLI: 長操作8種に `--timeout-ms <MS>` フラグを追加（ローカル DB では無視・リモート DB でのみ有意）

**ファイル:** `rust-sdk/src/remote.rs`, `rust-sdk/src/bin/cli/{main,db_client}.rs`, `docs/usage/cli/README.md`

### リモート CLI バイナリの自動デプロイをバージョン比較ベース化（TS parity）(TASK-69)

リモート DB 運用（`--db host:path`）で、クライアント接続時にリモートの `kijuku-cli` を常に最新へ自動更新。手動 scp / `mise run deploy` のリモート配置運用を廃止。

- **サーバ側**: `getServerVersion` operation 追加（`env!("CARGO_PKG_VERSION")` を返す・DB アクセス不要・prod/stg 両バックエンドで応答）
- **バージョン比較**: `MAJOR.MINOR.PATCH` を自前パーサ（`parse_semver`/`needs_deploy`・クレート依存追加なし）で比較。`local > remote`（厳密大なり）の時のみデプロイ（ダウングレード保護・equal skip）。リモート未取得/古いバイナリ（operation 未対応）はデプロイで自動回復（フェイルセーフ）
- **毎RPC自動**: `execute_remote_command_timed`（Rust）/ `executeRemoteCommand`（TS）の先頭で getServerVersion→比較→（古ければ）デプロイ。TS 現行の「無条件（存在チェックのみ）」からバージョン比較へ移行
- **デプロイ手順**: `deploy-local.sh` 移植（mkdir + SFTP 転送 + chmod + symlink）。実体 `~/.local/kijuku-db/bin/kijuku-cli` + symlink `~/.local/bin/kijuku-cli`（**破壊的**: TS 現行の symlink なし構成から変更）
- **media_root 外配置**: `upload_to_absolute_path`（Rust）/ `uploadFile`（TS）で media_root サンドボックスを回避する SFTP 直接書き込み
- **TS**: 新設 `version.ts`（`SDK_VERSION` 定数・手動 bump 対象に追加）・`uploadFile` タイムアウト 60s→120s（Rust `FILE_TRANSFER_TIMEOUT_MS` parity）

**ファイル:** `rust-sdk/src/{remote.rs,bin/cli/main.rs}`, `rust-sdk/tests/cli_integration_test.rs`, `ts-sdk/src/{version.ts,remote.ts}`, `ts-sdk/test/unit/remote-version.test.ts`, `docs/api.md`, `docs/usage/{cli/README.md,sdk/{rust,ts}/README.md}`, `docs/manual-testing-remote.md`

### リモート SSH Session を接続プールで再利用（TASK-70）

`RemoteKijukuDB` が RPC ごとに新規 SSH 接続（TCP+handshake+認証）を張っていたのを、`Arc<Mutex<Option<PooledSession>>>` で単一 Session をキャッシュ・再利用する方式に変更。連続 RPC（`import_media` のレコードごとの tag/attribute 操作等・stdin プロトコルは remote で禁止のため実質これが唯一の多段 RPC 経路）のレイテンシを改善。TS parity は別タスク。

- **Session キャッシュ**: `with_session` が初回 RPC で接続を確立してプールし、以降の RPC は再利用（再 handshake 省略）。clone 間で `Arc` 共有。ssh2 は同一 Session 上の channel を内部 Mutex で直列化するため、単一 Session の再利用で直列 RPC は安全（真の並行には複数 Session が必要・範囲外）
- **binary_ensured キャッシュ**: `ensure_remote_binary`（毎 RPC 先頭の `getServerVersion` ラウンドトリップ）を初回のみにガードし、連続 RPC の余分な RPC を削減
- **セッション系エラーで slot 無効化**: `KijukuError::Ssh` バリアントを新設し、TCP/channel/exec/read 等のセッション破壊エラーをアプリケーションエラー（exit code/JSON parse）と型で区別。`Ssh` のみ slot を無効化し次回再接続（フェイルセーフ）。アプリケーションエラーでは Session を保持
- **keepalive**: `set_keepalive(true, 30)` を設定（ssh2 0.9.6 では定期送信に別途 `keepalive_send` ポーリングが必要・本タスクではサーバ ClientAliveInterval + slot 無効化でフェイルセーフを担保）
- **診断 API**: `disconnect()`（明示的に SSH BYE 送信・slot 無効化）と `connect_count()`（接続回数・再利用検証用）を公開。`Drop` は実装しない（`Arc<Mutex>` と相性悪く・プロセス終了でソケットは閉じる）
- **`parking_lot::Mutex`**: poisoning なし（panic で全 clone が永久死ぬのを回避）

**ファイル:** `rust-sdk/src/{error.rs,remote.rs}`, `rust-sdk/tests/remote_test.rs`, `docs/usage/sdk/rust/README.md`

## Fixed

### `find_backup_by_id_in_scope` がスコープ引数を無視して Auto 固定を返すバグを修正(TASK-25)

- 検索スコープを Manual/Tmp で指定しても返り値の `scope` が常に `Auto` になっていた
- 公開化（`pub(crate)`）に伴い修正。実害は基底フル探索（Auto）に限られていたが一般性のバグ

**ファイル:** `rust-sdk/src/backup.rs`

### 差分バックアップ復元時の一時ファイル WAL 副産物（-wal/-shm）が残存するバグを修正(TASK-25)

- 差分(.diff)から一時フルDBを再構成して開く際、WAL モードの副産物 `*-wal`/`*-shm` が削除されず `backup/tmp/` に残留していた
- `with_backup_db` と `restore` の両方で `.db`/`-wal`/`-shm` の3ファイルを削除するよう修正

**ファイル:** `rust-sdk/src/{backup,lib}.rs`, `ts-sdk/src/{backup,index}.ts`

### バックアップの作成順ソートを mtime から id ベースに変更（差分復元の非決定失敗修正）(TASK-17)

- バックアップ一覧の作成順ソートが `stat.mtime` 基準だったため、同ミリ秒に作成されたフル(.db)と差分(.diff)が同 mtime になり `latest` 選択が非決定で古いフル(空状態)を選ぶことがあった。結果として差分復元で件数が 0 になる間欠的失敗の原因
- ソート基準をファイル名タイムスタンプ(id = `YYYYMMDDHHMMSS-mmm`, `lastBackupTimestampMs` で単調一意保証)の文字列比較に変更し決定論化
- TS SDK `backup.ts`(`listBackupsFiltered` / `findTodayFullBackup`) + Rust SDK `backup.rs`(`list_backups` / `find_today_full_backup`) の両方を修正
- 回帰テスト追加: TS `backup.test.ts` に `fs.utimesSync` でフル/差分の mtime を同一化し `latest` が差分を選ぶことを検証するテスト（CI フルスイートでも検出可能）

**ファイル:** `ts-sdk/src/backup.ts`, `rust-sdk/src/backup.rs`, `ts-sdk/test/integration/backup.test.ts`

## Changed

### RemoteKijukuDB（TS SDK）の SSH セッションを接続プールで再利用（TASK-71）

Rust SDK（TASK-70）の SSH Session 接続プールを TS SDK に parity 移植。従来 TS `RemoteKijukuDB` は各公開メソッドの `finally` で毎回 SSH 接続を切断（実質作り捨て）していたのを、初回 RPC で確立した接続をキャッシュして再利用する方式に変更。連続 RPC のレイテンシを改善。

- **Session キャッシュ（`withSession`）**: 単一 slot + Promise chain で RPC 全体を直列化。未接続時のみ接続確立（`connectCount++`）。`SshSessionError`（exec 失敗・読み取りタイムアウト・SFTP サブシステム確立失敗）で slot を無効化し次回再接続（フェイルセーフ）。アプリケーションエラー（exit≠0・JSON パース失敗・ファイル作成/転送失敗・転送タイムアウト）では Session を保持（Rust `Ssh`/`Io`/`Other` に対応）
- **`binaryEnsured` キャッシュ**: `getServerVersion` + デプロイ確認を初回のみにガードし連続 RPC のオーバーヘッドを削減（Rust `binary_ensured` parity）
- **適応的タイムアウト中央集約**: `executeRemoteCommand` が `longOpTarget` で長操作8種のタイムアウトを DB サイズから算出（restore は ×2・sync/discard は prod パス）。各メソッドに分散していたタイムアウト算出を Rust `execute_on_session` と同じ構造へ統一
- **公開 API 追加**: `disconnect()`（明示切断・実行中 RPC の完了を待つ）と `connectCount`（接続回数・診断用）。Rust `disconnect()`/`connect_count()` と parity

**ファイル:** `ts-sdk/src/remote.ts`, `ts-sdk/test/unit/remote-session-pool.test.ts`, `ts-sdk/test/e2e/sdk-remote.test.ts`, `docs/usage/sdk/ts/README.md`, `docs/examples/ts-sdk/05-bulk-operations.md`
