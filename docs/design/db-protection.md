# 本番 DB 保護設計（prod/stg 並行 + promote-only 書込・環境による構造的保護）

> 関連タスク: TASK-41「本番DB保護: LLM破壊的操作対策(read replica/stg DB)の検討」
> 関連ドキュメント: [path 設計](./path-design.md)、[backup](./backup.md)、[設計決定事項](./decisions.md)
> スコープ: ローカル SQLite バックエンド（NAS 上）を `RemoteKijukuDB`（SSH 経由）で操作する経路。D1 等の他バックエンドは当面スコープ外。
> レビュー履歴: Codex CLI レビュー（高5件・中5件・低1件・実コード検証済み）+ ユーザーレビュー（操作階層化・環境保護・prod復旧・diff要約・設定層）を反映。

## 0. 設計の基本思想（環境による構造的保護）

本設計の目的は、低性能 LLM の**無意識の prod 破壊**（幻覚・指示誤認による破壊操作）を、**環境/構造が自動的に防ぐ**ことである。**人間が都度承認・目視する（人間 gate）ことが目的ではない**。LLM の自律性を殺がず、環境が prod への破壊を届かせない構造を整える。悪意ある内部者はスコープ外（§2）。

保護は3つの柱で構成する:
1. **構造的保護**: prod は通常 readonly・LLM 接続は stg・prod RW は管理操作の内部のみ。
2. **操作階層化**: 操作を (a) stg 許可 / (b) stg 制限 に切り分け（§9）。
3. **システム gate**: observe gate・dry-run・trash・pre-stash を環境が強制（人間 gate でない）。監査は事後追跡のみ。

---

## 1. 目的と背景

### 1.1 脅威

低性能 LLM が時折、本番 DB に**無意識で破壊的な処理**（`DROP/DELETE/UPDATE` 相当・`restore` 全床上書き・`purge_trash` 物理削除・`migrate` スキーマ再構築）を行う。加えて、**書き込みを適用してみないと分からない不具合**（書き込み後のクエリ結果・整合性・振る舞いが壊れるケース）がある。path 設計（TASK-40）の移行処理等でも同懸念が高まる。

### 1.2 書き込み前保全（prevention）単独の限界

readonly / `--apply` / restore 前退避などの書き込み前保全は「破壊を防ぐ・戻せる」が、**書き込み後の振る舞いの正しさを検証できない**。よって保護は「prod を触らずに書き込み後の挙動を観察できる平行環境（stg）」を中心に据える。

### 1.3 既存の保護と backup（実態ベース）

| 既存保護 | 実態 | 本設計との関係 |
|---------|------|--------------|
| バックアップ（`backup.rs`・3本柱） | `record_operation()`（`backup.rs:341-362`）が CUD 操作後に呼ばれるが、**都度ではなく初回または `interval_ms`（既定 `3_600_000`ms = 1時間・`backup.rs:307`）経過時のみ** snapshot 取得 | 「危険操作前 snapshot 必須」に再定義（§7.3） |
| trash（論理削除） | ファイルは `.trash/` へ移動、`purge_trash` まで物理削除なし | (b) 操作として環境 gate で保護（§9） |
| dry-run / `--apply` | `file cp/mv/sync`・`trash purge`・`update-exist/thumbnail` は dry-run 既定 | システム gate の土台 |
| SSH 公鍵認証 | remote は公開鍵認証のみ（`remote.rs:177-184`） | 通信路保護・維持 |

**決定的な欠落**: CLI に対話型確認がない（`execute_command`・`cli.rs:1363`）。stdin 経路は全コマンド前に必ず `migrate()` する（`cli.rs:1140-1145`）。本設計は prod への直接書込を禁止し、prod 読込経路では migrate をスキップ（§5.1）、stg → prod の `promote` のみで反映する。

### 1.4 アクセス経路

本番 DB は NAS 上（`~/.local/share/kijuku/kijuku.db`、`remote.rs:31-43`）。LLM/CLI は `RemoteKijukuDB` → SSH → リモート `kijuku-cli` stdin プロトコル → `execute_command`（`cli.rs:1363`）で触る。対象 DB・実行場所は CLI 引数と環境変数で指定（§13）。

### 1.5 設計の目標

1. **prod 不変性**: prod は `promote`（および (b) 管理操作）以外一切書き換わらない（構造的保証）。
2. **書込後バグの捕捉**: prod を触らず stg で機械的 gate により検証（(a) 操作）。
3. **環境による保護**: 人間の都度承認ではなく、環境/構造で無意識破壊を防止。
4. **LLM 自律性**: stg 上で自由に (a) 操作。prod 動作は明示フラグのみ。
5. **既存機構の再利用**: backup.rs snapshot/Online Backup を転用。SDK SQL は変更しない。

---

## 2. 脅威モデルと前提

### 2.1 保護対象
- **LLM の無意識の誤操作**（幻覚・指示誤認・過剰な自主性）と**書き込み後の振る舞いバグ**。
- **悪意ある内部者・SSH 鍵漏洩はスコープ外**（別レイヤ）。これが「人間 gate 不要・環境保護」の根拠（悪意を防ぐのではなく無意識を防ぐ）。

### 2.2 stdin プロトコルは操作名ベース・ただし migrate は必須実行
`execute_command`（`cli.rs:1363-1478`）は操作名でディスパッチし、raw SQL 操作は存在しない。ただし stdin 経路は全コマンド前に `backend.migrate()`（`cli.rs:1140-1145` → `lib.rs:1120-1122`）を必須実行する。migrate は schema 変更を伴う RW 操作のため、prod readonly 接続では失敗する。よって **prod 読込経路では migrate をスキップ**（§5.1）。stg 上では任意の操作を走らせても prod は無傷。

### 2.3 backup は間隔付き auto backup（前提）
`record_operation` は `interval_ms`（既定1時間）経過時に snapshot 取得（毎書き込みではない・§1.3）。本設計は backup を置き換えず、stg 層と危険操作前 snapshot 必須化（§7.3）を追加する。

---

## 3. 設計方針：prod/stg 並行 + promote-only + 操作階層化

### 3.1 2 DB ファイル並行

```
kijuku.db        (prod)  アプリ読み取り元・promote/(b) 以外は不変
kijuku.stg.db    (stg)   LLM が RW で書き込むライブ作業コピー
```

両者は独立した SQLite ファイル。通常の stg 書込と prod 読込は別ファイルのためロック競合しない。WAL は同一 prod ファイル上の reader と promote writer の競合緩和のみ（§5.3）。

### 3.2 prod 書込禁止・promote と (b) 管理操作のみ

**prod への直接書込は禁止**。prod を書き換える経路は:
- **`promote`**: stg → prod への (a) 操作の反映。gate 強制。
- **(b) 管理操作**: 既存 FS 移動・削除・`purgeTrash`・`migrate`・`restore` 全床上書き。prod 直接だが dry-run + trash + pre-stash 強制（§9）。

いずれも prod RW 接続を各操作の**内部でのみ一時取得**する（LLM の恒久 prod RW 接続は持たない）。**人間の都度承認は必須でない**（環境 gate が強制・監査は事後追跡）。

### 3.3 ライフサイクル

```
1. sync    : prod DB → stg DB（フル複製）
2. LLM書込 : LLM は stg に (a) 操作（CRUD/hash/新規upload）。prod は無傷
3. observe : stg にクエリし、機械的 gate で書込後の挙動を検証
4. diff    : prod を RO で開き差分確認（要約+LLM prompt・§4.4）
5. promote : gate 合格で stg → prod へ反映（専用コピー API・pre-stash 付き）
   discard : ダメなら stg を再 sync
```

### 3.4 post-write bug の捕捉（核心）: 機械的 promote gate

observe は LLM の目視でなく、**機械的 gate** で判定する。**fast-promote（軽量 gate）は導入しない・全変更フル gate 統一**（安全性優先・統一フロー）。以下を満たさなければ promote 不可:

- `PRAGMA integrity_check` / 外部キー整合性
- 件数・差分の上限（想定外の大量変動を検出）
- 主要クエリの golden assertion
- stg/prod の `schema_version` 一致

### 3.5 読込先選択と設定層

読込は `source: "prod" | "stg"`（既定 `stg`）。詳細な指定手段（CLI 引数 > 環境変数 > デフォルト・デフォルト stg・prod は明示フラグ）は §13。

---

## 4. 実現形：stg DB ファイル方式

### 4.1 prod/stg 両ファイル
- prod: `kijuku.db`（`RemoteConfig.db_path` 既定）。
- stg: `kijuku.stg.db`（新設・`RemoteConfig.stg_db_path` 既定 = prod の `.stg.db`）。

### 4.2 sync（prod → stg）: Online Backup API のフル複製
`backup.rs` の Online Backup API を stg ファイル生成に転用。

### 4.3 LLM 書込: db_path を stg（SDK SQL 変更なし）
LLM の `RemoteKijukuDB` は `db_path = kijuku.stg.db` で起動（RW）。SDK の全 SQL は変更不要。`build_remote_command`（`remote.rs:79`）が stg パスを渡す。

### 4.4 observe / diff（要約 + LLM explanation prompt）
- **observe**: stg クエリ + §3.4 の機械 gate。
- **diff**: prod を別 `SQLITE_OPEN_READ_ONLY` 接続（または `mode=ro`+`query_only=ON` ATTACH）で開き snapshot 比較。`compute_diff`/`diff_media`（`diff.rs:49,153-180`）を適用。
- **diff-summarize（新）**: テーブル別件数・分布・サンプリングで差分を圧縮表示。量の多い差分の人間確認を支援。
- **LLM explanation prompt 生成（新）**: 差分データを構造化して LLM に渡し「変更内容・影響・リスク・promote 可否の根拠」を説明させる prompt を自動生成。機械 gate（客観判定）と併用し、人間/LLM の判断を補助。

### 4.5 promote（stg → prod）: 専用コピー API
既存 `restore()`（`backup.rs:456-505`）は BackupManager の BackupSelector → live DB で、任意 stg→prod コピー API ではない。pre-stash も `enabled=true` 時のみ。よって:
- 新設 `copy_db_online(src, dst)`（Online Backup API・明示コピー）。
- pre-stash は `enabled` と独立して**常に強制**（`tmp/{stem}.{ts}-pre_promote.db`）。
- promote のみ prod を RW open（gate 強制・§6.4）。

### 4.6 discard
stg 破棄・再 sync。prod は一切触られない。

### 4.7 ファイル実体（mediaRoot）の扱い

ファイル実体（メディアファイル）は DB レコードと密結合（`mediaMv` = FS 移動 + DB path 更新）。**操作階層化（§9）で切り分ける**。COW 的 stg mediaRoot（変更分コピーで FS 再現）は**導入しない**（過剰）。stg は DB + 新規 upload のみで軽量。

- **ケース1（既存media・配置済み）の登録**: (a)。prod mediaRoot の既存ファイルを stg DB が RO 参照 → `computeMediaHash` 計算 → stg DB。promote で DBレコードのみ。FS 操作なし。
- **ケース2（新規media・未配置）の登録**: (a)。新規ファイルを stg ステージング領域に upload → `createMedia` + hash（staging 読込）→ promote で prod mediaRoot へ配置 + DBレコード。stg ステージングは新規/変更分のみ（prod フルコピーでない）。
- **既存ファイルの移動・削除**: (b)。stg では制限。prod 直接だが dry-run + trash + pre-stash 強制（§9）。

---

## 5. prod 書込禁止の強制（構造的保護・人間 gate なし）

### 5.1 prod readonly open + no-migrate
promote/(b) 以外の全接続は prod を `SQLITE_OPEN_READ_ONLY` で open。**かつ prod 読込経路では `migrate()` をスキップ**（stdin `cli.rs:1140-1145` に対し、prod 読込接続では migrate を呼ばない分岐）。`DBOptions.readonly`（`types.rs:320`→`lib.rs:132-136`・TS `types.ts:236`→`index.ts:83`）を prod 既定に昇格。

### 5.2 prod RW は管理操作の内部のみ（人間 gate なし）
prod RW 接続は `promote` と (b) 管理操作の**内部でのみ一時取得**。LLM の恒久 prod RW 接続はない。各操作はシステム gate（observe gate・dry-run・trash・pre-stash）を強制。**人間の都度承認は必須でない**（環境が安全を担保・監査は事後追跡・§10）。

### 5.3 WAL 化（同一 prod ファイル競合緩和）+ busy_timeout
通常の stg 書込と prod 読込は別ファイルのため競合しない。真の競合は同一 prod ファイル上の RO reader と promote/(b) writer。これを緩和するため Rust open 時に `PRAGMA journal_mode=WAL`（現状未設定・`lib.rs:130-161`）+ **busy_timeout を open 時に実装**（現状 `DBOptions.timeout`・`types.rs:319` は open 未使用）。WAL は reader を完全ブロックしないため retry/メンテ窓も別途設計。**TS は readonly 接続で journal_mode 設定をスキップ**（`index.ts:81-89` は無条件実行を修正）。

---

## 6. ライフサイクル運用

### 6.1 書込セッション
「sync → stg 編集 → observe/diff → promote/discard」のセッション単位。

### 6.2 読込先の選択（§3.5・§13 の運用）
読込先は `source: "prod" | "stg"`（既定 `stg`）。書込は常に stg。

### 6.3 sync 頻度・stg 鮮度
書込セッション開始時に sync。prod は promote/(b) 以外不変のため、sync 後 promote まで stg は「sync 時点の prod + LLM 編集」を正確に表す（ロスト更新なし）。

### 6.4 promote のトリガ（gate 強制・人間実行でない）
promote は §3.4 の機械 gate 合格を条件に反映する。**人間が実行・承認する必須要件とはしない**（環境 gate が安全を担保）。LLM が promote を起動しても、gate が強制チェックし不合格なら拒否・合格なら実行。prod RW は promote API 内部でのみ一時取得（構造的保護は維持）。監査（§10）で事後追跡。

---

## 7. backup との統合

### 7.1 stg の auto-backup（継続）
stg の書き込みにも `record_operation`（interval 制）が働き、stg の backup trail が作られる。

### 7.2 promote/(b) 前 pre-stash（強制）
promote・(b) は prod 上書き前に `tmp/...pre_promote.db` へ退避。**`enabled` と独立して常に強制**。

### 7.3 backup 連携の脆弱点是正（prod を触る前に前倒し）
- **migrate 前 snapshot 必須**: Rust/TS とも migrate は BackupManager 連携なし（`lib.rs:1120-1122`・`index.ts:167`）。
- **migrate は stg/admin/(b) 専用**: CLI stdin の自動 migrate（`cli.rs:1140-1144`）は prod 読込経路でスキップ（§5.1）。
- **ts migrate v4/v5 非トランザクション是正**（`migration.ts:141-194`）: Rust（`migration.rs`）と同じ transaction 境界に。
- **pre-stash を `enabled` と独立して強制**（`backup.rs:466-475`）。
- **retention が manual backup を削除しない**（`backup.rs:846-922`）。
- **pre_restore/promote 退避の期限延長**（`backup.rs:925-949`）。

### 7.4 backup 構成（集約）
- **prod**: auto（interval）+ manual + promote/(b) 前 pre-stash。
- **stg**: auto（interval）。
- **retention**: manual は対象外（手動削除のみ）・pre-stash は通常 tmp より長く保持。
- prod 復元の経路は §8。

---

## 8. prod 不具合時の復旧

promote/(b) 後、app/server 稼働中に発覚する不具合の復旧経路（3段階）:

1. **即時**: pre-stash 戻し（promote/(b) 直後・分単位）。`tmp/...pre_promote.db` → prod。
2. **短期**: 直近 interval backup からの復元。
3. **長期**: backup trail からの point-in-time 復元。

- 保持期間: pre-stash は数日・backup trail は retention（§7.4）。
- 実行経路: (b) 管理操作として prod RW を一時取得（§5.2）。dry-run + pre-stash 強制。
- 監査: 復旧操作も §10 の監査ログに記録。
- **発見経路**: `list_backups` は pre-stash を除外するため、promote/(b)操作が返す `pre_stash_path` を呼出側が失った場合は `list_pre_stashes()`（CLI `list-pre-stashes`・TS Remote `listPreStashes`）で `backup/tmp/` 配下の pre-stash を発見できる。戻り値の `path` を `BackupSelector::by_path`（TS Remote は `restore({ type: 'byPath', path })`）で `restore` に渡して即時戻しする。

---

## 9. 破壊的操作の扱い: 操作階層化（切り分けと制御）

操作を (a) stg 許可 / (b) stg 制限 に切り分ける。保護は「人間が都度見る」ためでなく、「環境が (b) を stg で弾き、(a) だけを stg 経由で検証」するため。

### 9.1 (a) stg 許可操作（LLM 自律・promote で prod）
- DBレコード CRUD・タグ・属性
- hash 計算（`computeMediaHash`/`addMediaHash`）
- 新規 upload（stg ステージング・§4.7 ケース2）

### 9.2 (b) stg 制限操作（prod 直接・システム gate 強制）
- 既存 FS 移動（`mediaMv`・upload 以外）
- 削除（`deleteMedia` の FS・`purgeTrash`）
- `migrate`・`restore` 全床上書き

(b) は stg では**環境が制限（拒否）**。必要なら prod 直接だが、**dry-run + trash + pre-stash を環境が強制**（人間 gate でない）。post-write bug 捕捉（observe）は (a) のみ。(b) の安全は pre-stash による即時巻き戻し（§8）で担保。bulk 操作は計画 + 整合性チェックに留め、post-write 捕捉は監査で補完（§15）。

### 9.3 なぜ stg で (b) を再現しないか
既存ファイルの移動・削除を stg で再現するには COW 的 stg mediaRoot（対象コピー）が必要だが過剮。(b) は LLM の無意識破壊の代表で、**stg で制限し prod 直接のシステム gate に回す**方が、環境保護の思想に合い、stg を軽量に保てる。

---

## 10. 監査ログ（prod DB 本体に置かない・事後追跡）

sync/promote/discard・(b) 操作・observe/diff 結果を記録。**監査は人間の承認フローでなく、事後追跡用**。
- 記録内容: timestamp・操作・対象範囲・実行者・diff/gate サマリ。
- **prod DB 本体に入れない**（ファイル全体 promote で破壊的上書きされるため）。append-only サイドカー（`backup/meta/` 配下等）または prod 外別ストア。promote 後に prod 側へ追記。
- backup metadata も現状サイドカー（`backup/meta/backup-meta.json`・`auto-records.csv`・`backup.rs:951-1023`）。promote（DB ファイル上書き）の影響を受けない（§15-2）。

---

## 11. 評価した代替案

- **純 prevention**: 部分採用。書込後バグを捉えられないため単独では不十分。
- **同一ファイル二重 table**: 棄却。SDK 全 SQL の table ルーティング + スキーマ2倍維持が大改修。
- **read replica（受動的）**: 部分採用のうえ拡張。stg は live 書込環境 + observe + promote。
- **confirm プロンプト**: 棄却。stdin over SSH で技術不可。
- **LLM 自己 promote**: **見直し・条件付き許容**（ユーザーレビュー）。脅威モデルが「無意識破壊」で悪意はスコープ外（§2.1）のため、gate 強制の自己 promote を許容。prod RW は promote API 内部のみ（構造的保護維持）。人間実行を必須としない。
- **COW 的 stg mediaRoot（ファイル実体コピーで FS 再現）**: 棄却（ユーザーレビュー）。過剰。ファイル実体は操作階層化（§9）で切り分け。

---

## 12. スコープ

ローカル SQLite バックエンド（NAS 上）を `RemoteKijukuDB` で操作する経路。D1 等・Web GUI server（既に読み取り専用）は当面スコープ外。

---

## 13. 設定と環境変数（実行環境による対象制御）

対象 DB・実行場所を CLI 引数と環境変数で指定する。「環境が整えることで無意識破壊を防ぐ」の具体手段。**CLI 引数が常に勝つ**。環境変数はフォールバック。

### 13.1 指定手段と優先順位

**優先順位**: CLI 引数 > 環境変数 > コンフィグ（.env）> デフォルト

**対象 DB（prod/stg）**:
- CLI: `--db <path>`・`--target prod|stg`・`--read-source prod|stg`（追加フラグ）
- 環境変数: `KIJUKU_DB_PATH`・`KIJUKU_STG_DB_PATH`・`KIJUKU_TARGET`・`KIJUKU_READ_SOURCE`

**実行場所（リモート先）**:
- 環境変数: `REMOTE_SSH_HOST`（既存）・`KIJUKU_REMOTE_HOST`・接続設定類

### 13.2 デフォルト stg・prod は明示フラグ（保護の核心）

- **デフォルト（CLI も環境変数も指定なし）**: **stg**。
- **prod 動作**: 明示フラグ（`--target prod` 等）または環境変数 `KIJUKU_TARGET=prod`。CLI 引数が勝つ。
- **保護の論理**: LLM の**無意識**操作は prod フラグを付けないので stg で止まる。prod に行くには明示が必要（LLM が幻覚で破壊操作を流しても stg 上）。環境変数で CLI を無効化せず、CLI 尊重を保ったまま「デフォルト stg」で保護。

### 13.3 LLM 環境 vs 管理環境
- **LLM 環境**: 環境変数で stg をデフォルト設定。LLM の通常操作は stg。prod は明示フラグが要るため無意識には届かない。
- **管理環境**: 環境変数を付けず、CLI 引数で prod を明示指定可能。

---

## 14. 影響を受ける箇所

### 14.1 prod/stg 切替・readonly + no-migrate
- `remote.rs:12-43`（`RemoteConfig` に `stg_db_path`・`db_path` を stg 既定）・`remote.rs:79`（`build_remote_command`・stg パス・`--readonly` for prod）
- `cli.rs:1140-1144`（**prod 読込経路は migrate スキップ**）・`cli.rs:491,616,875,896`（`--db`=stg・prod readonly・`stg sync/promote/discard`・`--read-source`・`--target`）・`cli.rs:1363`（読込先ルーティング・prod RO 接続）
- `types.rs:317-341`・`ts-sdk/types.ts:234-253`・`remote.ts:44-51`・`cli.ts:115,208`

### 14.2 sync/promote（専用コピー API）
- `backup.rs`（**新設 `copy_db_online(src,dst)`**・pre-stash を `enabled` と独立して強制）
- `ts-sdk/backup.ts`（同上）

### 14.3 diff（prod RO + 要約 + LLM prompt）
- `diff.rs:49,153-180`（`compute_diff`/`diff_media` 2 ファイル比較）・`lib.rs:906-930`（prod RO 接続 or `mode=ro`+`query_only=ON`）
- **新規**: diff-summarize・LLM explanation prompt 生成

### 14.4 WAL + busy_timeout
- `lib.rs:130-161`（WAL・**busy_timeout 実装**）・`ts-sdk/index.ts:81-89`（readonly 時 WAL スキップ）

### 14.5 backup 連携是正（§7.3・前倒し）
- `migration.rs`・`migration.ts:141-194`（transaction 統一）・`lib.rs:1120-1122`・`index.ts:167`（migrate 前 snapshot）・`backup.rs:466-475,846-922,925-949`・`backup.ts`

### 14.6 操作階層化 (a)/(b)
- 分類は**破壊度基準**（可逆/追加的操作=(a)、不可逆/全床上書き=(b)）で決定（TASK-59 P2-C4）:
  - **(b) stg 制限**: `mediaMv`（FS移動・不可逆上書き）・`purgeTrash`（FS物理削除）・`restore`（全床上書き）。stg で環境が拒否・prod 直接（`--target prod`）で dry-run+trash+pre-stash gate 強制（§9.2）。`deleteMedia` の FS 部は trash 統合（`moveToTrash`(a) → `purgeTrash`(b)）で扱い、`deleteMedia` 本体は DB-only (a)。
  - **(a) stg 許可**: `crud`（create/update/delete/bulk）・`tag`・`attr`・`hash`・`upload`・`mediaCp`/`mediaSync`・`moveToTrash`/`restoreFromTrash`・`backup`/`setBackupLabel`/`setBackupNote`。
- `migrate` は §15-5 で「stg 経由詳細→P3」とされるため **P3（TASK-45）まで (b) gate 化を繰延**（現状: 接続時 auto-migrate + `create_pre_migrate_snapshot`）。

### 14.7 設定層（§13）
- CLI 引数解析（`--target`・`--read-source` 等の追加フラグ）・環境変数読込（`KIJUKU_*`）・優先順位解決

### 14.8 影響を受けない（安全）箇所
- CRUD/hash/upload（stg 上では現状どおり・SQL 変更なし）・search・server API（既に読み取り専用）

---

## 15. 未決定の詳細（実装時に詰める）

1. **promote の粒度**: ファイル全体コピー（既定）か表選択的か。→ P2。
2. **backup metadata の扱い**: 現状サイドカー（`backup/meta/`）。promote は DB ファイルのみ対象のため metadata は影響しない。監査も prod DB 外（§10）。→ P2。
3. **sync の自動/手動**: → P1。
4. **stg ファイル命名/配置**: `kijuku.stg.db`（prod 同ディレクトリ）か。→ P0。
5. **migrate の stg 経由詳細**: TASK-40 v7 と協調。→ P3。
6. **監査ログの実体**: prod DB 外 append-only サイドカーか。→ P4。
7. **prod readonly 化の既存運用への影響**: app/server 経路。→ P0。
8. **読込先切替の内部実装**: prod RO 別接続（推奨）か `mode=ro`+`query_only=ON` ATTACH か。→ P1。
9. **読込の既定**: stg 既定（保留中変更可視）でよいか。→ P1。
10. **observe gate のしきい値**: integrity/FK/件数上限/golden/schema_version の各基準（§3.4）。→ P1。
11. **複数 LLM/セッションの stg 排他**（→ 決定・TASK-55）: **global advisory lock** 方式（session別stg は不採用）。`<stg>.lock` の排他ロックを書込系（sync / stg 編集セッション）が取得（Rust=fs2 advisory lock / TS=PID ベースロックファイル）。sync 元 prod revision 指紋（`ProdRevision` = schema_version + 各テーブル件数/max-id）を `<stg>.meta.json` に記録し、observe の `prod_sync_revision` gate で drift 検出。promote 対象は単一 stg（ロック保持セッション）。
12. **prod 読込経路の migrate スキップ実装**: `cli.rs:1140-1144` を prod 読込時に飛ばす分岐。→ P0。
13. **(b) 操作の gate 強度**（→ 決定・TASK-59 P2-C4）: (b) gate = dry-run + trash + pre-stash。操作種別で適用サブセットを切替:
    - **DB 層（`restore`）**: `ProdRwScope`（`<prod>.lock` 排他ロック + pre-stash 強制・§7.2）→ prod RW 一時オープン → 上書き。pre-stash が §8 即時巻き戻しを担保。`dryRun` で復元差分（`diff_with_backup`）を返し prod 不変。
    - **FS 層（`mediaMv`/`purgeTrash`）**: dry-run ファースト + 上書き/削除は trash 経由（`file_ops`/`trash` に既存）。trash が FS 側の即時巻き戻し経路。
    - prod RW は (b) 操作の内部でのみ一時取得（`--target prod` の readonly backend は使わない・§5.2）。人間 gate なし・監査は P4（TASK-46）。bulk 操作の FS 再現上限は別途。
14. **prod 復旧の保持期間**: pre-stash・backup trail の具体的リテンション（§8）。→ P2。
15. **diff-summarize/LLM prompt の形式**（→ 決定・TASK-53）: 要約は `ProdStgDiffSummary = {counts, totals, distribution}`。分布は media（`byType`/`byArtistTop`/`byFlagExist`）・media_tags（`byTagTop`）・attributes（`byKey`）・hashes（`byFilenameTop`）、上位 N=10（変動件数降順）。prompt（`build_diff_explanation_prompt`）は4セクション（件数表・代表サンプル・分布・説明指示）+ セマンティクス注記（added=stg新規 等）、`maxSamplesPerSection=10`・`includeDistribution` で分布セクション ON/OFF。純粋関数でクライアント側計算（リモート転送しない・Rust/TS で構造的一致）。

---

## 16. 実装フェーズ

| Phase | 内容 | リスク |
|-------|------|--------|
| **P0** | Rust WAL + busy_timeout・prod readonly + no-migrate・`stg_db_path`・`--db`=stg 配線・stg 命名・TS readonly 時 WAL スキップ・**§13 設定層（CLI>env>デフォルト・`--target`/`KIJUKU_TARGET`・デフォルト stg）**・**§7.3 migrate 前 snapshot・migrate の stg/(b) 専用化** | 中〜高 |
| **P1** | sync（backup() 転用）・observe（stg クエリ + **promote gate 機械化**）・diff（prod RO + **diff-summarize + LLM prompt**）・stg 排他・**§7.3 TS migrate transaction 統一・pre-stash 強制** | 中 |
| **P2** | promote（**`copy_db_online` 専用 API**・gate 強制・pre-stash 強制・人間 gate なし）・**(b) 操作の prod 直接 gate（dry-run+trash+pre-stash）**・promote 粒度/metadata・prod 復旧（§8） | 中〜高 |
| **P3** | discard・ライフサイクル運用・破壊操作（migrate 含む）の (b) 扱い | 中 |
| **P4** | 監査ログ（prod 外・append-only・事後追跡）・両 SDK parity・運用ドキュメント | 低 |

各 Phase で Rust → TS → CLI。テスト先行:
- prod readonly で書込拒否・prod 読込で migrate スキップ（readonly でも read 成功）
- sync/promote ラウンドトリップ・**gate 不合格で promote 拒否**
- **(a) は stg 経由・(b) は stg で拒否され prod 直接 gate で保護**
- **設定層: CLI>env>デフォルト・デフォルト stg・prod は `--target prod` でのみ**
- stg 上破壊操作が prod 無傷・promote 後差分一致・クラッシュ時 pre-stash 回復・prod 復旧（pre-stash 戻し）

---

## 17. トレードオフ

### コスト
- prod/stg 2 ファイル運用・sync/promote/discard・操作階層化 (a)/(b)
- prod readonly + no-migrate・WAL + busy_timeout・設定層（CLI/env/デフォルト）
- `copy_db_online` 新設 + pre-stash 強制・promote gate・diff-summarize + LLM prompt・prod 復旧

### メリット
- **prod 不変性**: promote/(b) 以外 prod 不変（構造的 readonly）。
- **環境による保護**: 人間の都度承認でなく、環境/構造で無意識破壊を防止（デフォルト stg・prod は明示）。
- **書込後バグ捕捉**: (a) 操作は stg で機械 gate により完全検証（fast-promote なし・フル gate）。
- **操作の切り分け**: (a) は stg 完全検証、(b) は stg 制限 + prod 直接システム gate。stg 軽量（COW mediaRoot 不要）。
- **migrate 衝突の解消**: prod readonly + 読込経路 no-migrate。
- **可読性**: diff-summarize + LLM explanation prompt で量の多い差分を人間/LLM が判断可能。

### 棄却した代替案（再掲）
- 純 prevention・同一ファイル二重 table・受動的 read replica・confirm プロンプト・**LLM 自己 promote（条件付き許容に見直し）**・**COW 的 stg mediaRoot（棄却）**（§11）。
