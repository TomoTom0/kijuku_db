# Path 設計（決定論 path + root 管理）

> 関連タスク: TASK-40「path 設計（決定論 path）の検討と方針確定」
> 関連ドキュメント: [設計決定事項](./decisions.md)、[update-exist 詳細](./update-exist-detail.md)、[backup](./backup.md)
> レビュー履歴: Codex CLI によるレビューを実施（高6件・中4件の指摘を取り込み改訂）

## 1. 目的と背景

### 1.1 解決すべき不整合

現状、`Media.path` は**ユーザー/インポータが与える自由文字列**として DB に保存される。一方で、サムネイル生成と存在チェックは**特定のディレクトリレイアウトを暗黙に前提**している:

- `rust-sdk/src/thumbnail.rs:36-40` `resolve_thumbnail_path(path, uuid)` は、path 中の**最後の `content` コンポーネントの親**を取り、`{親}/cover/{uuid}.jpg` を期待サムネイル位置として計算する。
- `rust-sdk/src/update_exist.rs:169`（comic: path をディレクトリ扱い）、`:202`（video/music: `{uuid}.*` 代替拡張子探索）。

つまり**コードは「`.../{uuid}/content/...` + 兄弟 `cover/{uuid}.jpg`」という配置を期待しているが、DB は自由 path を許している**。この乖離が、path が期待レイアウトから外れたレコードでサムネイル/存在チェックが黙って失敗する原因になる。

### 1.2 file_ops と DB の不整合

`media_cp/mv/sync`（`rust-sdk/src/file_ops.rs:106-310`）はファイルシステム操作のみを行い、DB の `Media.path` を一切更新しない。`FileOpOptions.update_db` フラグ（`file_ops.rs:16-32`）は受け取るが**コード内で参照すらされない**。結果として cp/mv/sync 後、`Media.path` は旧パスのままで **DB とファイルシステムが不整合**する。

### 1.3 拡張性の欠如（複数 root 非対応）

現状は `DBOptions.media_root: Option<String>`（`types.rs:323-329`）の**単一 root** しか扱えない。複数ドライブ/NAS/ボリュームにまたがる運用では、すべてのメディアを単一のディレクトリツリー（またはシンボリックリンク）の下に収める必要がある。コード・コメント・TODO を含め、複数 root を想定した記述は一切存在しない（新規機能として導入する必要がある）。

### 1.4 設計の目標

1. **決定論 path**: メディアの物理位置を `(root_id, uuid, media_type, extension)` から**決定論的に導出**可能にする。レコードさえあればファイルシステム上の位置が復元できる（自己修復性）。
2. **複数 root 管理（第一級要件）**: `root_id` + `roots` テーブルで複数ボリュームを扱う。拡張性は後付けではなく最初から組む。
3. **FS/DB 整合**: file_ops 操作後に DB の path を uuid から再計算して追従させる。操作はジャーナル駆動でクラッシュリカバリ可能にする。
4. **既存の暗黙前提の正式化**: thumbnail/exist ロジックが前提とする `content`/`cover` レイアウトを、path 体系として正式に採用する。

---

## 2. 設計方針

### 2.1 決定論 path（uuid ベース・root 相対）

メディアの path は自由文字列ではなく、**root からの相対パスとして `(uuid, media_type, extension)` から決定論的に導出**される値とする。

**レイアウト**（root 配下、uuid ごと）:

```
{root}/{uuid}/
    content/
        comic   : ページ画像ディレクトリ（001.jpg, 002.jpg, ...）。path はここを指す
        video   : {uuid}.{ext} の単一ファイル。path はこのファイルを指す
        music   : {uuid}.{ext} の単一ファイル。path はこのファイルを指す
    cover/
        {uuid}.jpg   : サムネイル（music は対象外）
```

**`Media.path`（root 相対）の導出規則**:

| media_type | `Media.path`（root 相対） |
|------------|--------------------------|
| `comic`    | `{uuid}/content`（ディレクトリ） |
| `video`    | `{uuid}/content/{uuid}.{ext}`（ファイル） |
| `music`    | `{uuid}/content/{uuid}.{ext}`（ファイル） |

これは thumbnail.rs / update_exist.rs が既に前提としているレイアウトそのものであり、**新規の発明ではなく既存暗黙前提の正式化**である。

**絶対パス解決**: ファイルシステムアクセス時は `roots[root_id].path / Media.path` で絶対パスを得る。`Media.path` は root 相対なので、同じレコードが root の実パスの異なる別環境（バックアップ/移植）でも一貫する。

**導出関数（新設・`media_path.rs` に追加）**:

```rust
/// root 相対のメディア content path を決定論的に導出する。
pub fn derive_media_rel_path(uuid: &str, media_type: &str, extension: Option<&str>) -> String {
    match media_type {
        "comic" => format!("{uuid}/content"),
        "video" | "music" => {
            let ext = extension.unwrap_or("bin");
            format!("{uuid}/content/{uuid}.{ext}")
        }
        _ => format!("{uuid}/content"),
    }
}

/// root 相対のサムネイル cover path を導出する（media_type に依存しない）。
pub fn derive_thumbnail_rel_path(uuid: &str) -> String {
    format!("{uuid}/cover/{uuid}.jpg")
}
```

### 2.2 root 管理（root_id + roots テーブル） — 第一級要件

path の拡張性は要件であり、後回しにしない。複数 root を最初からサポートする。

**`roots` テーブル（新設）**:

```sql
CREATE TABLE roots (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    uuid        TEXT NOT NULL UNIQUE,        -- 安定識別子（クロス環境/バックアップ移植用）
    path        TEXT,                        -- ファイルシステム絶対パス（環境依存）。D1等の論理rootは NULL
    label       TEXT,                        -- 人間用ラベル（例: "NAS-main", "external-1"）
    status      TEXT NOT NULL DEFAULT 'active'
                CHECK(status IN ('active','readonly','disabled')),
    created_at  DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at  DATETIME DEFAULT CURRENT_TIMESTAMP
);
```

- `roots.uuid` は**安定識別子**（決定論の対象ではない）。実パスが環境ごとに異なっても、同じ root なら同じ uuid で参照でき、メディアレコードの移植性が保たれる。cross-DB の diff/relocate は `roots.uuid` で突き合わせる（`roots.id` は環境ごとに異なり得る）。
- `roots.path` は環境依存の実パス。NULL 許可（D1 などローカル FS を持たない論理root用）。
- `status` は操作権限を表す。**操作ごとの許可マトリクス**:

| status | 読み取り(file/thumbnail/hash) | 書き込み(cp/mv/sync/relocate) | 備考 |
|--------|----------------------------|----------------------------|------|
| `active` | OK | OK | 通常の稼働 root |
| `readonly` | OK | **拒否** | 読み取り専用アーカイブ（書き込み禁止） |
| `disabled` | **拒否**（metadata のみ参照可） | 拒否 | 退避/取り外し済み。レコードのメタデータは残すがファイルアクセスしない |

### 2.3 `Media.path` の保存モデルと配置状態（layout_status）

`Media.path` は **DB に格納**する（非保存・読み時計算は採用しない: レガシー表現・UNIQUE 制約・全読み側再計算の複雑さから棄却）。ただし「常に派生値と等価」では**レガシー（未移行）レコードを表現できない**ため、配置状態カラムで不変条件を分離する:

```sql
ALTER TABLE media ADD COLUMN root_id        INTEGER NOT NULL REFERENCES roots(id);
ALTER TABLE media ADD COLUMN layout_status  TEXT NOT NULL DEFAULT 'legacy'
                                      CHECK(layout_status IN ('canonical','legacy'));
```

**不変条件**:

- `layout_status='canonical'` ⟹ `Media.path == derive_media_rel_path(uuid, media_type, extension)`（**常に派生値と等価**）。新規作成・移行済みのメディア。
- `layout_status='legacy'` ⟹ `Media.path` は**現時点の実位置**（root 相対だが決定論レイアウトではない可能性あり）。読み側は格納 path をそのまま使う。relocate コマンドで canonical 化する。

この分離により、「派生等価」と「レガシー保持」の矛盾を解消する。`canonical` レコードは常に再導出可能（自己修復）、`legacy` レコードは実 path を保持しつつ段階的に canonical 化できる。

**UNIQUE 制約**: 既存 `path TEXT UNIQUE` を **`(root_id, path)` 複合 UNIQUE** に置き換える（異なる root 間では同じ相対 path が存在し得る）。`path` は nullable を維持（NULL = 物理ファイルなし/削除済み。複数 NULL は UNIQUE に抵触しない）。`root_id` は NOT NULL（全行に root が紐づく）。

---

## 3. 設定・多 root 接続

### 3.1 DBOptions の拡張

```rust
pub struct DBOptions {
    pub roots: Vec<RootConfig>,   // 既存 media_root: Option<String> は後方互換ショートカット（単一root扱い）
    ...
}
pub struct RootConfig {
    pub uuid: String,             // 既存rootと紐付く場合。新規は自動生成
    pub path: Option<String>,     // 絶対パス（論理rootは None）
    pub label: Option<String>,
    pub read_only: bool,          // status: readonly に対応
}
```

### 3.2 root 同期（DB open 時）

DB open 時に `DBOptions.roots` と `roots` テーブルを **`roots.uuid` 基準で同期**する:

- config にあって DB にない root → INSERT
- 両方にある root → 実パス/label/status を更新（config 側を正とする）
- DB にあって config にない root → そのまま残す（disabled 扱いはしない。ユーザー判断）
- **マイグレーションで path が NULL のまま残ったデフォルト root**（§5.1）は、config の `media_root` があればそこから実パスを埋める

**移行（v7）と root 初期化の責務分離**（レビュー指摘への対応）: migration 層は `Connection`/`SqlExec` のみ持ち `DBOptions` を知らないため、**root の実パス初期化は migration ではなく DB open 時のこの同期処理で行う**。migration は空のデフォルト root（path=NULL）を作るまで（§5.1）。

### 3.3 複数 root の CLI / remote プロトコル

現行は `--media-root` 単一受け渡し（`cli.rs:627`, `remote.rs:79`, `ts-sdk/src/remote.ts:342`）。複数 root に拡張:

- **CLI**: `--media-root <path>`（単一・後方互換）に加え、`--root <uuid|label|path>`（root 選択）、cp/mv の cross-root 用に `--src-root`/`--dst-root` を追加。
- **remote RPC**: `RemoteConfig.roots: Vec<RootConfig>`（単一 `media_root` は廃止または alias）。`build_remote_command` は複数 `--root` を渡す。
- **file_ops 引数**: src/dst は「`<root指定>:<相対path>`」形式（例: `nas-main:abc/content/x.mp4`）または `--src-root`/`--dst-root` + 相対path。単一 root 運用時は root 指定を省略可。

> 引数体系の詳細構文は P2 実装時に確定するが、プロトコル（root 識別子で選択）はここで決定。

---

## 4. スキーマ変更（マイグレーション v7）

現行 `schema_version` 方式（`migration.rs:33` target=6）。v7 を追加する。**v7 は純スキーマ変更のみ**（root 実パスの初期化は含まない — それは DB open 同期の責務 §3.2）。

### 4.1 v7 の内容（純スキーマ・再構築）

`media` テーブルの制約変更は ALTER 不可のため、`migration.rs:114-175` の再構築パターンを踏襲して一発で作り直す:

1. `roots` テーブル作成（§2.2）。
2. **デフォルト root を1行 INSERT**: `uuid = <生成値>`、`path = NULL`、`label = 'default'`、`status = 'active'`。`uuid` 生成は純 SQL で `lower(hex(randomblob(16)))`（32 hex・一意）。実パスは DB open 同期（§3.2）で埋める。
3. `media` 再構築: `root_id INTEGER NOT NULL REFERENCES roots(id)`（**全行をデフォルト root に紐付け**）、`layout_status TEXT NOT NULL DEFAULT 'legacy'`、`path` は既存値を引き継ぎ、`(root_id, path)` 複合 UNIQUE を付与。

**NULL 重複抜け穴の回避**（レビュー指摘）: `root_id` は再構築時に**全行へデフォルト root を埋めてから** UNIQUE 作成するため、`root_id` に NULL は存在しない。これにより段階移行中の重複配置抜けを防ぐ。

### 4.2 影響ファイル（スキーマ系）

- `rust-sdk/schema.sql`（roots テーブル・path 制約変更。他バックエンドはスコープ外）
- `rust-sdk/src/migration.rs`（target=7、`apply_migration`/`apply_migration_async` へ v7 分岐追加）
- `ts-sdk/src/migration.ts`（同上）
- `docs/design/decisions.md` の制約・スキーマバージョン管理節（`:119-128, :163`）更新

---

## 5. 既存データ移行（分類・非破壊・段階的）

**基本方針: マイグレーション（v7）ではファイルの物理移動を行わない。** 既存メディアのファイルはその場所に置いたまま、レコードだけを新しいスキーマに適合させる。物理的な決定論レイアウトへの移行は別コマンド（§5.3）で利用者のタイミングで行う。

### 5.1 Phase 0 — スキーマ移行（v7・自動）

§4.1 の純スキーマ適用。この時点で: すべてのメディアはデフォルト root に属し、`layout_status='legacy'`。デフォルト root の実パスは DB open 同期で埋まる。**ファイルは1つも動いていない**。

### 5.2 Phase 0.5 — 既存 path の分類と root 相対化（dry-run 検査コマンド）

**レビュー指摘（絶対path相対化のデータ破壊リスク）への対応**。既存 `Media.path` には絶対パス・相対パス混在があり得る。これを**破壊せず分類**する検査コマンドを新設:

```
kijuku-cli --db ... path classify --root <default-root-path>   # dry-run（変更せず分類結果を出力）
```

分類カテゴリ:

| カテゴリ | 判定 | 処理 |
|---------|------|------|
| **in-root** | デフォルト root 配下の絶対/相対 path | root 相対に正規化して格納。`layout_status='legacy'` |
| **out-of-root** | root 外の絶対 path | **エラーとして報告**（移動するか別 root 割当か削除か、ユーザー判断）。自動相対化しない |
| **unresolvable** | シンボリックリンク経由・大小文字差・末尾スラッシュ衝突・UNC など | エラーとして報告 |

- **重要**: out-of-root / unresolvable を**黙って処理（破壊）しない**。dry-run で全件分類し、ユーザーが `--apply` で in-root のみ相対化、それ以外は個別対応。
- 大文字小文字・末尾スラッシュは正規化（`normalize_lexical`）で吸収。シンボリックリンクは `resolve_existing_within_root` の canonicalize で実体判定。
- この結果、移行後は**全 path がいずれかの root 配下の相対path**（不変条件）。`resolve_within_root` が常に成立する。

### 5.3 Phase 1 — 新規メディアの決定論化

- `create_media`（`crud.rs:108-168, 397-456`）で `derive_media_rel_path` で**決定論 path を強制設定**、`layout_status='canonical'`。
- インポータは決定論位置へファイルを置く責任（または SDK が配置）。

### 5.4 Phase 2 — レガシーメディアの漸次移行（別コマンド・利用者実行）

レガシー（`layout_status='legacy'`）を決定論レイアウトへ物理移行するコマンド（§6 のジャーナル/リカバリ機構を使用）:

```
kijuku-cli --db ... path relocate [--filter ...] [--apply]   # dry-run ファースト
```

- 各メディアの派生 path を計算し、現 path と異なればジャーナル駆動で安全に物理移動（§6）。成功後 `layout_status='canonical'` に更新。
- `--filter`（media_type や UUID 範囲）と dry-run で大規模ライブラリでも安全に進行。

### 5.5 移行戦略の選択理由

「一括物理移行（破壊的）」ではなく「分類＋段階的（非破壊）」を選ぶ理由: 既存データ破壊リスクの最小化、大規模 I/O の回避、利用者による移行タイミング制御、dry-run による事前検証。

---

## 6. file_ops と DB の整合と信頼性設計

### 6.1 update_db の対象特定ルール（レビュー指摘への対応）

path ベースの cp/mv/sync から対象 uuid を一意に推定できないケースがあるため、`update_db` の有効範囲を**明確に制限**する:

- **(A) UUID 指定のメディアレベル操作**: 対象 uuid が自明。`update_db` で派生 path を再計算して DB 更新。
- **(B) path 指定の操作**: src/dst が**既存の `Media.path` と完全一致**する場合のみ、該当レコードの path を再計算して更新。一致しない（raw ファイル操作）場合は `update_db` を無視（DB は触らない）。

曖昧な uuid 推測は行わない。これにより FS/DB 不整合（§1.2）を安全に解消する。

### 6.2 操作モデルの2階層

1. **ファイルレベル操作**（現行 cp/mv/sync）: root 配下の任意パス間の raw 操作。新規取り込み/外部ディレクトリ同期が用途。`update_db=true` かつ (A)/(B) 条件を満たす場合のみ DB 更新。
2. **メディアレベル操作**（新設）: UUID 指定で「正規の決定論位置へ配置 / 別 root へ移動」。`path relocate`（§5.4）と `update_db` の主戦場。

### 6.3 FS/DB 整合のジャーナルとリカバリ（レビュー指摘への対応）

`media_mv` は現状 FS rename 後に即終了し DB トランザクションを持たない（`file_ops.rs:174`）。特に**root 跨ぎ move は非 atomic**（copy + source 削除になる）。破壊的操作（mv/relocate/sync の trash）には**操作ジャーナル**を導入する:

**ジャーナルテーブル**（`op_journal`）またはジャーナルファイルに操作 intent を記録し、段階適用する:

```
1. intent 記述（src, dst, 対象 uuid, 操作種別）を journal に INSERT
2. FS: dst へ copy
3. verify: サイズ/ハッシュで dst が src と一致（root 跨ぎ copy の破損検出）
4. DB 更新: Media.path/root_id/layout_status を更新（1トランザクション）
5. FS: src を trash へ移動
6. journal のエントリを完了マーク
```

**クラッシュリカバリ**: DB open 時に未完了 journal をスキャンし、段階に応じて resume/rollback:

- step2 途中（copy 不完全）→ copy やり直し（dst は冪等に上書き）
- step3 失敗/未実行 → verify 再実行
- step4 未実行（copy 済み・DB 古い）→ DB 更新を完遂
- step5 未実行（DB 更新済み・src 残存）→ src を trash
- step6 未実行 → 完了マーク

**root 跨ぎ move は非 atomic を前提**とし、copy+verify+trash の多段階で実装する。既存の trash.rs（dry-run ファースト・trash 経由）と組み合わせる。

---

## 7. スコープ

本設計は**ローカル SQLite バックエンド専用**。決定論 path・複数 root・file_ops・thumbnail・hash・relocate のすべてをローカル backend でサポートする。他バックエンドへの適用は当面スコープ外とする。

---

## 8. 影響を受ける箇所（実装計画）

### 8.1 スキーマ/マイグレーション（必須）
- `rust-sdk/schema.sql`（roots テーブル・path 制約・layout_status。他バックエンドはスコープ外）
- `rust-sdk/src/migration.rs:33`（target=7）、`:114-175/:423-491`（再構築パターン踏襲）
- `ts-sdk/src/migration.ts:26, 47-230`
- `docs/design/decisions.md:119-128, 163`

### 8.2 path 生成・解決
- `rust-sdk/src/media_path.rs`: `derive_media_rel_path` / `derive_thumbnail_rel_path` 新設。複数 root 解決。
- `rust-sdk/src/crud.rs:108-168, 397-456`（create で決定論 path 強制・layout_status='canonical'）
- **両 SDK 非対称の解消**: TS だけ `path` 指定時に `flag_exist` を自動1化（`ts-sdk/src/crud.ts:57, 176-178`）。Rust（`crud.rs:108`）にはない。この機会に揃える。

### 8.3 thumbnail/exist（前提の正式化・簡素化）
- `rust-sdk/src/thumbnail.rs:13-58, 259-451`・`ts-sdk/src/thumbnail.ts`: 決定論 path で期待位置が一意化。`resolve_media_file_path` の代替拡張子探索（`:64-91`）が簡素化可能。
- `rust-sdk/src/update_exist.rs:169, 202, 84-100`: path 解釈の決定論レイアウト固定化。

### 8.4 ハッシュ計算
- `rust-sdk/src/hash.rs:462-491, 713-744`・`cli.rs:2140-2178, 2208-2212`・`ts-sdk/src/remote.ts:1045-1055`: `media_path` を root 絶対解決してから使用。
- **`media_hashes` レコード自体は影響なし**（FK が `media(uuid)` 基準・path 非依存）。

### 8.5 file_ops と DB 整合・ジャーナル
- `rust-sdk/src/file_ops.rs:16-32, 106-310`・`lib.rs:198-228`・`ts-sdk/src/file_ops.ts`・`ts-sdk/src/index.ts:107-140`: `update_db` 実装（§6.1 ルール）・`op_journal` 導入（§6.3）。
- `rust-sdk/src/bin/cli.rs:627, 795-820, 2290-2316, 1487-1510`・`remote.rs:28, 79-85, 281-320, 402-412`・`ts-sdk/src/remote.ts:50, 342, 423-450`: 複数 root / root_id 解決の配線（§3.3 プロトコル）。

### 8.6 backup / diff（レビュー指摘への対応 — 「影響なし」は不正確）
- DB ファイルバックアップ自体の path は別物だが、**diff は `Media` 全体を比較**（`diff.rs:49, 180`）、**snapshot は `find_media` の `Media` を保持**（`lib.rs:1056`）。
- `Media.root_id` と `roots` テーブルは snapshot/diff 対象に含める。cross-DB diff では `root_id` は `roots.uuid` で突き合わせる（`roots.id` は環境ごとに異なり得るため）。`roots` テーブルの差分も報告対象。

### 8.7 影響を受けない（安全）箇所
- `MediaFilter` / `search.rs`（path でフィルタしない）
- `media_tags` / `media_attributes`（`media.id` 基準）
- `media_hashes`（`media.uuid` 基準）

---

## 9. 実装フェーズ

| Phase | 内容 | リスク |
|-------|------|--------|
| **P0** | スキーマ v7（roots + media.root_id NOT NULL + layout_status + 複合 UNIQUE）+ path classify dry-run/apply（§5.2） | 中（テーブル再構築・path 分類） |
| **P1** | `derive_*_rel_path` 新設 + `create_media` 決定論 path 強制 + thumbnail/exist/hash の root 絶対解決 | 中 |
| **P2** | `DBOptions.roots` 多 root 接続 + open 時同期（§3.2）+ file_ops 複数 root + `update_db`（§6.1）+ CLI/remote `--root` プロトコル | 中〜高 |
| **P3** | `op_journal` + リカバリ（§6.3）+ レガシー移行 `path relocate`（§5.4） | 高（実データ移動・クラッシュリカバリ） |
| **P4** | thumbnail/exist 前提コード簡素化・両 SDK 非対称解消・backup/diff の roots スコープ対応 | 低 |

各 Phase で Rust SDK → TS SDK → CLI の順に実装。テスト先行（決定論導出ユニットテスト・path 分類テスト・ジャーナルリカバリ統合テスト）。

---

## 10. 未決定の詳細（実装時に詰める）

1. **シャーディング接頭辞**: 大規模（10万件超）で `{uuid}/` フラット階層が問題になる場合、`{uuid[:2]}/{uuid}/...` を入れるか。現状は**入れない**（thumbnail.rs は最終 `content` 親のみ見るので互換）。
2. **file_ops 引数の詳細構文**: `<root>:<相対path>` 形式か `--src-root`/`--dst-root` + 相対path か。→ P2 で確定。
3. **`media_root: Option<String>` の扱い**: 後方互換ショートカットとして残すか廃止するか。→ P0/P2 で決定。
4. **path classify の out-of-root 既定動作**: エラー停止か、警告しつつ継続か。→ P0 で決定（デフォルトはエラー停止・`--allow-out-of-root` で継続、を想定）。
5. **`op_journal` の実体**: 専用テーブルかジャーナルファイルか。→ P3 で決定（DB トランザクション一貫性を優先するならテーブル）。

---

## 11. トレードオフ

### コスト
- スキーマ v7（テーブル再構築含む）・path 分類・root 相対化
- 多 root 配線（設定/接続同期/file_ops/CLI/RPC の全層）
- `op_journal` とクラッシュリカバリの実装
- thumbnail/exist/hash の root 絶対解決への読み替え
- レガシー移行コマンドの実装

### メリット
- **自己修復性**: canonical レコードは uuid から物理位置が常に復元可能。
- **FS/DB 整合**: file_ops 後の path 不整合が解消。ジャーナルでクラッシュリカバリ。
- **複数ボリューム対応**: 複数ドライブ/NAS 運用が第一級サポート（拡張性）。
- **暗黙前提の解消**: thumbnail/exist が前提するレイアウトを正式採用。
- **コード簡素化**: 決定論化で代替拡張子探索・path 推論が削れる。

### 棄却した代替案
- **非保存・読み時計算 path**: レガシー表現・UNIQUE・全読み側再計算の複雑さから棄却（§2.3）。代わりに `layout_status` で canonical/legacy を分離。
- **単一 root の維持**: 拡張性要件に反するため棄却。複数 root は最初から組む（後から導入すると再移行が必要で高コスト）。
- **一括物理移行**: 実データ破壊リスク・大規模 I/O から棄却。分類＋段階的移行（§5）を採用。
- **path 相対化の自動黙示処理**: out-of-root/unresolvable を破壊的に処理しない。dry-run 分類＋ユーザー判断（§5.2）。
