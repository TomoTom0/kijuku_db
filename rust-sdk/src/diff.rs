//! バックアップと現在DBの差分（diff）機能
//!
//! 復元判断を支援するため、指定バックアップと現在のDBを比較し
//! 「何が追加され/削除され/変更されたか」を可視化する。
//!
//! added/removed/changed の意味:
//! - added:   バックアップに在り現在に無し（復元で復活する）
//! - removed: 現在に在りバックアップに無し（復元で失われる）
//! - changed: 両方に在り内容が異なる（復元で上書きされる）

use std::collections::{BTreeMap, BTreeSet};

use crate::types::{Media, MediaAttribute, MediaHash, MediaTagAssoc, Tag};

/// 差分の詳細度
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum DiffDetail {
    /// 件数サマリのみ（各リストは空、summary のみ）
    SummaryOnly,
    /// 各カテゴリ上位 N 件まで
    Limited { n: usize },
    /// 全件
    Full,
}

impl Default for DiffDetail {
    fn default() -> Self {
        DiffDetail::Limited { n: 100 }
    }
}

/// 差分取得オプション
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct DiffOptions {
    /// 詳細度（省略時 Limited(100)）
    pub detail: Option<DiffDetail>,
}

/// 差分件数サマリ
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct DiffCounts {
    pub added: usize,
    pub removed: usize,
    pub changed: usize,
}

impl DiffCounts {
    /// added + removed + changed の合計（分布ソート用）
    pub fn total(&self) -> usize {
        self.added + self.removed + self.changed
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MediaChange {
    pub current: Media,
    pub backup: Media,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TagChange {
    pub current: Tag,
    pub backup: Tag,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AttributeChange {
    pub current: MediaAttribute,
    pub backup: MediaAttribute,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HashChange {
    pub current: MediaHash,
    pub backup: MediaHash,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct MediaDiff {
    pub added: Vec<Media>,
    pub removed: Vec<Media>,
    pub changed: Vec<MediaChange>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct TagDiff {
    pub added: Vec<Tag>,
    pub removed: Vec<Tag>,
    pub changed: Vec<TagChange>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct MediaTagAssocDiff {
    pub added: Vec<MediaTagAssoc>,
    pub removed: Vec<MediaTagAssoc>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct AttributeDiff {
    pub added: Vec<MediaAttribute>,
    pub removed: Vec<MediaAttribute>,
    pub changed: Vec<AttributeChange>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct HashDiff {
    pub added: Vec<MediaHash>,
    pub removed: Vec<MediaHash>,
    pub changed: Vec<HashChange>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupDiffSummary {
    pub media: DiffCounts,
    pub tags: DiffCounts,
    /// 紐付けは一致/不一致のみ（changed は常に 0）
    pub media_tags: DiffCounts,
    pub attributes: DiffCounts,
    pub hashes: DiffCounts,
}

/// バックアップと現在DBの差分
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BackupDiff {
    pub media: MediaDiff,
    pub tags: TagDiff,
    pub media_tags: MediaTagAssocDiff,
    pub attributes: AttributeDiff,
    pub hashes: HashDiff,
    pub summary: BackupDiffSummary,
}

/// 差分比較用のDBスナップショット（5テーブル全件を主キーで索引付け）
pub type AttrKey = (i64, String);
pub type HashKey = (String, String, String);

#[derive(Debug, Clone, Default)]
pub struct BackupSnapshot {
    pub media: BTreeMap<i64, Media>,
    pub tags: BTreeMap<i64, Tag>,
    pub media_tags: BTreeSet<(i64, i64)>,
    pub attributes: BTreeMap<AttrKey, MediaAttribute>,
    pub hashes: BTreeMap<HashKey, MediaHash>,
}

impl BackupSnapshot {
    pub fn from_media(values: Vec<Media>) -> Self {
        let media = values.into_iter().map(|m| (m.id, m)).collect();
        Self {
            media,
            tags: BTreeMap::new(),
            media_tags: BTreeSet::new(),
            attributes: BTreeMap::new(),
            hashes: BTreeMap::new(),
        }
    }
}

/// 現在DBとバックアップのスナップショットから差分を計算する
pub fn compute_diff(current: &BackupSnapshot, backup: &BackupSnapshot, options: &DiffOptions) -> BackupDiff {
    let (media, media_counts) = diff_media(&current.media, &backup.media);
    let (tags, tag_counts) = diff_tag(&current.tags, &backup.tags);
    let (media_tags, mt_counts) = diff_assoc(&current.media_tags, &backup.media_tags);
    let (attributes, attr_counts) = diff_attribute(&current.attributes, &backup.attributes);
    let (hashes, hash_counts) = diff_hash(&current.hashes, &backup.hashes);

    let mut diff = BackupDiff {
        media,
        tags,
        media_tags,
        attributes,
        hashes,
        summary: BackupDiffSummary {
            media: media_counts,
            tags: tag_counts,
            media_tags: mt_counts,
            attributes: attr_counts,
            hashes: hash_counts,
        },
    };
    let detail = options.detail.clone().unwrap_or_default();
    apply_detail(&mut diff, &detail);
    diff
}

fn diff_media(
    current: &BTreeMap<i64, Media>,
    backup: &BTreeMap<i64, Media>,
) -> (MediaDiff, DiffCounts) {
    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut changed = Vec::new();

    for (id, b) in backup {
        match current.get(id) {
            None => added.push(b.clone()),
            Some(c) if c != b => changed.push(MediaChange { current: c.clone(), backup: b.clone() }),
            _ => {}
        }
    }
    for (id, c) in current {
        if !backup.contains_key(id) {
            removed.push(c.clone());
        }
    }

    let counts = DiffCounts {
        added: added.len(),
        removed: removed.len(),
        changed: changed.len(),
    };
    (MediaDiff { added, removed, changed }, counts)
}

fn diff_tag(current: &BTreeMap<i64, Tag>, backup: &BTreeMap<i64, Tag>) -> (TagDiff, DiffCounts) {
    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut changed = Vec::new();

    for (id, b) in backup {
        match current.get(id) {
            None => added.push(b.clone()),
            Some(c) if c != b => changed.push(TagChange { current: c.clone(), backup: b.clone() }),
            _ => {}
        }
    }
    for (id, c) in current {
        if !backup.contains_key(id) {
            removed.push(c.clone());
        }
    }

    let counts = DiffCounts {
        added: added.len(),
        removed: removed.len(),
        changed: changed.len(),
    };
    (TagDiff { added, removed, changed }, counts)
}

fn diff_assoc(
    current: &BTreeSet<(i64, i64)>,
    backup: &BTreeSet<(i64, i64)>,
) -> (MediaTagAssocDiff, DiffCounts) {
    let added: Vec<MediaTagAssoc> = backup
        .difference(current)
        .map(|(m, t)| MediaTagAssoc { media_id: *m, tag_id: *t })
        .collect();
    let removed: Vec<MediaTagAssoc> = current
        .difference(backup)
        .map(|(m, t)| MediaTagAssoc { media_id: *m, tag_id: *t })
        .collect();

    let counts = DiffCounts {
        added: added.len(),
        removed: removed.len(),
        changed: 0,
    };
    (MediaTagAssocDiff { added, removed }, counts)
}

fn diff_attribute(
    current: &BTreeMap<AttrKey, MediaAttribute>,
    backup: &BTreeMap<AttrKey, MediaAttribute>,
) -> (AttributeDiff, DiffCounts) {
    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut changed = Vec::new();

    for (key, b) in backup {
        match current.get(key) {
            None => added.push(b.clone()),
            Some(c) if c != b => changed.push(AttributeChange { current: c.clone(), backup: b.clone() }),
            _ => {}
        }
    }
    for (key, c) in current {
        if !backup.contains_key(key) {
            removed.push(c.clone());
        }
    }

    let counts = DiffCounts {
        added: added.len(),
        removed: removed.len(),
        changed: changed.len(),
    };
    (AttributeDiff { added, removed, changed }, counts)
}

fn diff_hash(
    current: &BTreeMap<HashKey, MediaHash>,
    backup: &BTreeMap<HashKey, MediaHash>,
) -> (HashDiff, DiffCounts) {
    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut changed = Vec::new();

    for (key, b) in backup {
        match current.get(key) {
            None => added.push(b.clone()),
            Some(c) if c != b => changed.push(HashChange { current: c.clone(), backup: b.clone() }),
            _ => {}
        }
    }
    for (key, c) in current {
        if !backup.contains_key(key) {
            removed.push(c.clone());
        }
    }

    let counts = DiffCounts {
        added: added.len(),
        removed: removed.len(),
        changed: changed.len(),
    };
    (HashDiff { added, removed, changed }, counts)
}

/// 詳細度に応じて各リストを切り詰める（summary の件数は維持）
fn apply_detail(diff: &mut BackupDiff, detail: &DiffDetail) {
    match detail {
        DiffDetail::SummaryOnly => {
            diff.media.added.clear();
            diff.media.removed.clear();
            diff.media.changed.clear();
            diff.tags.added.clear();
            diff.tags.removed.clear();
            diff.tags.changed.clear();
            diff.media_tags.added.clear();
            diff.media_tags.removed.clear();
            diff.attributes.added.clear();
            diff.attributes.removed.clear();
            diff.attributes.changed.clear();
            diff.hashes.added.clear();
            diff.hashes.removed.clear();
            diff.hashes.changed.clear();
        }
        DiffDetail::Limited { n } => {
            fn trunc<T>(v: &mut Vec<T>, n: usize) {
                v.truncate(n);
            }
            trunc(&mut diff.media.added, *n);
            trunc(&mut diff.media.removed, *n);
            trunc(&mut diff.media.changed, *n);
            trunc(&mut diff.tags.added, *n);
            trunc(&mut diff.tags.removed, *n);
            trunc(&mut diff.tags.changed, *n);
            trunc(&mut diff.media_tags.added, *n);
            trunc(&mut diff.media_tags.removed, *n);
            trunc(&mut diff.attributes.added, *n);
            trunc(&mut diff.attributes.removed, *n);
            trunc(&mut diff.attributes.changed, *n);
            trunc(&mut diff.hashes.added, *n);
            trunc(&mut diff.hashes.removed, *n);
            trunc(&mut diff.hashes.changed, *n);
        }
        DiffDetail::Full => {}
    }
}

// ========== prod/stg 差分の要約（TASK-53・設計 §4.4/§15-15）==========

/// 分布の上位何件を残すか
const DISTRIBUTION_TOP_N: usize = 10;

/// prod/stg 比較の差分要約（promote 判断の補助資料・設計 §4.4/§15-15）。
///
/// `BackupDiff` を stg 編集視点（added=stg新規=promoteでprod追加 等）で作った前提で、
/// 件数・合計・分布を圧縮して保持する。機械的 gate（TASK-54）の入力や LLM explanation
/// prompt の素材になる。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProdStgDiffSummary {
    /// 5テーブル別の件数（既存 `BackupDiffSummary` を再利用）
    #[serde(default)]
    pub counts: BackupDiffSummary,
    /// 全テーブル横断の合計（gate しきい値比較用）
    #[serde(default)]
    pub totals: DiffTotals,
    /// テーブル別の偏り（異常な一括変更の検出）
    #[serde(default)]
    pub distribution: DiffDistribution,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct DiffTotals {
    pub added: usize,
    pub removed: usize,
    pub changed: usize,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffDistribution {
    #[serde(default)]
    pub media: MediaDistribution,
    #[serde(default)]
    pub media_tags: MediaTagAssocDistribution,
    #[serde(default)]
    pub attributes: AttributeDistribution,
    #[serde(default)]
    pub hashes: HashDistribution,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaDistribution {
    /// media_type 別の件数（"comic"/"video"/"music" ...）
    #[serde(default)]
    pub by_type: BTreeMap<String, DiffCounts>,
    /// artist 別上位 N（None は "unknown"）。変動件数（total）降順
    #[serde(default)]
    pub by_artist_top: Vec<(String, DiffCounts)>,
    /// flag_exist 別の件数（"true"/"false"）。一括存在フラグ変更の検出
    #[serde(default)]
    pub by_flag_exist: BTreeMap<String, DiffCounts>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaTagAssocDistribution {
    /// tag_id 別上位 N（どのタグ紐付けが大量増減したか）。変動件数降順
    #[serde(default)]
    pub by_tag_top: Vec<(i64, DiffCounts)>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttributeDistribution {
    /// key 別の件数（EAV: 特定属性キーの一括変更検出）
    #[serde(default)]
    pub by_key: BTreeMap<String, DiffCounts>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HashDistribution {
    /// filename 別上位 N（どのファイル群のハッシュ再計算が起きたか）。変動件数降順
    #[serde(default)]
    pub by_filename_top: Vec<(String, DiffCounts)>,
}

/// `BackupDiff`（stg 編集視点）から要約を生成する純粋関数（設計 §4.4/§15-15）。
///
/// 件数（`counts`）・全テーブル合計（`totals`）・テーブル別分布（`distribution`）を計算する。
/// 分布は `diff` の各リスト（added/removed/changed）を走査して構築するため、`apply_detail`
/// で truncate された場合は分布の件数が `counts` と一致しない（`Full` 指定時のみ完全一致）。
pub fn summarize_diff(diff: &BackupDiff) -> ProdStgDiffSummary {
    let counts = diff.summary.clone();
    let totals = DiffTotals {
        added: counts.media.added
            + counts.tags.added
            + counts.media_tags.added
            + counts.attributes.added
            + counts.hashes.added,
        removed: counts.media.removed
            + counts.tags.removed
            + counts.media_tags.removed
            + counts.attributes.removed
            + counts.hashes.removed,
        changed: counts.media.changed
            + counts.tags.changed
            + counts.media_tags.changed
            + counts.attributes.changed
            + counts.hashes.changed,
    };
    let distribution = DiffDistribution {
        media: summarize_media(&diff.media),
        media_tags: summarize_media_tags(&diff.media_tags),
        attributes: summarize_attributes(&diff.attributes),
        hashes: summarize_hashes(&diff.hashes),
    };
    ProdStgDiffSummary {
        counts,
        totals,
        distribution,
    }
}

// ========== 機械的 promote gate（observe・TASK-54・設計 §3.4/§15-10） ==========

/// golden assertion（運用者定義のドメイン不変条件・設計 §3.4）。
/// `sql` の最初のカラム・最初の行を件数として実行し、`expected_min`/`expected_max` の
/// 範囲内なら合格。DB制約（UNIQUE/CHECK/FK）が担保する領域と重複しない運用者定義領域用。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoldenAssertion {
    pub name: String,
    pub sql: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_min: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_max: Option<i64>,
}

/// golden assertion の SQL 実行結果（`observe` 側で実行し `evaluate_gate` に渡す・純粋性担保）。
/// SQL エラー時は `Error` とし gate を FAIL 扱いにする（安全側）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum GoldenResult {
    Count(i64),
    Error(String),
}

/// gate 設定（設計 §3.4/§15-10）。閾値は `ProdStgDiffSummary.totals` と比較する。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GateConfig {
    /// promote で prod に追加される行数の上限（`totals.added`）
    #[serde(default = "default_max_added")]
    pub max_added: usize,
    /// promote で prod から削除される行数の上限（`totals.removed`・削除は最も危険なので厳しめ）
    #[serde(default = "default_max_removed")]
    pub max_removed: usize,
    /// promote で prod が上書きされる行数の上限（`totals.changed`）
    #[serde(default = "default_max_changed")]
    pub max_changed: usize,
    /// 運用者定義の golden assertion（デフォルト空・設計 §3.4）
    #[serde(default)]
    pub golden_assertions: Vec<GoldenAssertion>,
}

impl Default for GateConfig {
    fn default() -> Self {
        GateConfig {
            max_added: default_max_added(),
            max_removed: default_max_removed(),
            max_changed: default_max_changed(),
            golden_assertions: Vec::new(),
        }
    }
}

fn default_max_added() -> usize {
    5000
}
fn default_max_removed() -> usize {
    1000
}
fn default_max_changed() -> usize {
    5000
}

/// 個別 gate 検査の結果
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GateCheck {
    pub name: String,
    pub passed: bool,
    pub detail: String,
}

/// observe の結果（設計 §3.4/§4.4）。`passed` は全 gate check 合格か（promote 可否の客観判定）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObserveResult {
    pub passed: bool,
    pub prod_schema_version: i64,
    pub stg_schema_version: i64,
    /// 差分要約（gate の素材・`summarize_diff` の出力）
    pub summary: ProdStgDiffSummary,
    /// 各 gate 検査の結果
    pub checks: Vec<GateCheck>,
}

impl ObserveResult {
    /// 全 check 合格か
    pub fn all_passed(checks: &[GateCheck]) -> bool {
        checks.iter().all(|c| c.passed)
    }
}

/// observe のオプション（設計 §4.4）。差分取得と gate 評価の設定を束ねる。
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObserveOptions {
    /// 差分取得オプション（省略時 `DiffOptions::default` = Limited(100)）
    #[serde(default)]
    pub diff_options: DiffOptions,
    /// gate 設定（省略時 `GateConfig::default`）
    #[serde(default)]
    pub gate_config: GateConfig,
}

/// gate を評価する純粋関数（設計 §3.4）。SQL 実行（integrity/fk/golden）は呼出側（`observe`）
/// で行い結果を引数で受け取ることで、DB に依存しない単体テストを可能にする。
///
/// - `integrity`: `migration::integrity_check` の結果（"ok" で合格）
/// - `fk_violations`: `migration::foreign_key_check` の結果（空で合格）
/// - `fk_enabled`: `migration::is_foreign_keys_enabled` の結果（true で合格寄与）
/// - `golden_results`: `config.golden_assertions` と同順序の SQL 実行結果
pub fn evaluate_gate(
    summary: &ProdStgDiffSummary,
    integrity: &str,
    fk_violations: &[crate::migration::FkViolation],
    fk_enabled: bool,
    prod_schema_version: i64,
    stg_schema_version: i64,
    golden_results: &[GoldenResult],
    config: &GateConfig,
) -> Vec<GateCheck> {
    let mut checks = Vec::new();

    // 1. integrity_check
    checks.push(GateCheck {
        name: "integrity_check".to_string(),
        passed: integrity == "ok",
        detail: integrity.to_string(),
    });

    // 2. 外部キー整合性（FK 有効 + 違反なし）
    let fk_ok = fk_enabled && fk_violations.is_empty();
    let fk_detail = if !fk_enabled {
        "foreign_keys disabled".to_string()
    } else if fk_violations.is_empty() {
        "ok (no violations)".to_string()
    } else {
        let sample: Vec<String> = fk_violations
            .iter()
            .take(5)
            .map(|v| format!("{} rowid={}", v.table, v.rowid))
            .collect();
        format!("{} violation(s): {}", fk_violations.len(), sample.join(", "))
    };
    checks.push(GateCheck {
        name: "foreign_key_check".to_string(),
        passed: fk_ok,
        detail: fk_detail,
    });

    // 3. schema_version 一致（prod と stg）
    checks.push(GateCheck {
        name: "schema_version_match".to_string(),
        passed: prod_schema_version == stg_schema_version,
        detail: format!("prod={} stg={}", prod_schema_version, stg_schema_version),
    });

    // 4. 件数・差分上限（`totals` vs 各 max）
    let t = &summary.totals;
    checks.push(GateCheck {
        name: "max_added".to_string(),
        passed: t.added <= config.max_added,
        detail: format!("added={} (max {})", t.added, config.max_added),
    });
    checks.push(GateCheck {
        name: "max_removed".to_string(),
        passed: t.removed <= config.max_removed,
        detail: format!("removed={} (max {})", t.removed, config.max_removed),
    });
    checks.push(GateCheck {
        name: "max_changed".to_string(),
        passed: t.changed <= config.max_changed,
        detail: format!("changed={} (max {})", t.changed, config.max_changed),
    });

    // 5. golden assertions（運用者定義）
    for (i, ga) in config.golden_assertions.iter().enumerate() {
        let (passed, detail) = match golden_results.get(i) {
            Some(GoldenResult::Error(e)) => (false, format!("query error: {}", e)),
            Some(GoldenResult::Count(count)) => {
                let min_ok = ga.expected_min.map_or(true, |m| *count >= m);
                let max_ok = ga.expected_max.map_or(true, |m| *count <= m);
                let bound = match (ga.expected_min, ga.expected_max) {
                    (Some(lo), Some(hi)) => format!("[{}, {}]", lo, hi),
                    (Some(lo), None) => format!(">= {}", lo),
                    (None, Some(hi)) => format!("<= {}", hi),
                    (None, None) => "no bound".to_string(),
                };
                (min_ok && max_ok, format!("count={} (expected {})", count, bound))
            }
            None => (false, "no result (assertion/result mismatch)".to_string()),
        };
        checks.push(GateCheck {
            name: format!("golden:{}", ga.name),
            passed,
            detail,
        });
    }

    checks
}

fn flag_exist_key(b: bool) -> &'static str {
    if b {
        "true"
    } else {
        "false"
    }
}

fn bump(map: &mut BTreeMap<String, DiffCounts>, key: &str, field: &str) {
    let entry = map.entry(key.to_string()).or_default();
    match field {
        "added" => entry.added += 1,
        "removed" => entry.removed += 1,
        _ => entry.changed += 1,
    }
}

fn top_n_string(map: &BTreeMap<String, DiffCounts>, n: usize) -> Vec<(String, DiffCounts)> {
    let mut entries: Vec<_> = map.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    entries.sort_by(|a, b| b.1.total().cmp(&a.1.total()));
    entries.truncate(n);
    entries
}

fn top_n_i64(map: &BTreeMap<i64, DiffCounts>, n: usize) -> Vec<(i64, DiffCounts)> {
    let mut entries: Vec<_> = map.iter().map(|(k, v)| (*k, v.clone())).collect();
    entries.sort_by(|a, b| b.1.total().cmp(&a.1.total()));
    entries.truncate(n);
    entries
}

fn summarize_media(media: &MediaDiff) -> MediaDistribution {
    let mut by_type: BTreeMap<String, DiffCounts> = BTreeMap::new();
    let mut by_artist: BTreeMap<String, DiffCounts> = BTreeMap::new();
    let mut by_flag_exist: BTreeMap<String, DiffCounts> = BTreeMap::new();

    for m in &media.added {
        bump(&mut by_type, m.media_type.as_str(), "added");
        bump(&mut by_artist, m.artist.as_deref().unwrap_or("unknown"), "added");
        bump(&mut by_flag_exist, flag_exist_key(m.flag_exist), "added");
    }
    for m in &media.removed {
        bump(&mut by_type, m.media_type.as_str(), "removed");
        bump(&mut by_artist, m.artist.as_deref().unwrap_or("unknown"), "removed");
        bump(&mut by_flag_exist, flag_exist_key(m.flag_exist), "removed");
    }
    // changed は prod 側（current）で分類（promote で上書きされる prod レコードの観点）
    for c in &media.changed {
        let m = &c.current;
        bump(&mut by_type, m.media_type.as_str(), "changed");
        bump(&mut by_artist, m.artist.as_deref().unwrap_or("unknown"), "changed");
        bump(&mut by_flag_exist, flag_exist_key(m.flag_exist), "changed");
    }

    MediaDistribution {
        by_type,
        by_artist_top: top_n_string(&by_artist, DISTRIBUTION_TOP_N),
        by_flag_exist,
    }
}

fn summarize_media_tags(media_tags: &MediaTagAssocDiff) -> MediaTagAssocDistribution {
    let mut by_tag: BTreeMap<i64, DiffCounts> = BTreeMap::new();
    for a in &media_tags.added {
        let entry = by_tag.entry(a.tag_id).or_default();
        entry.added += 1;
    }
    for a in &media_tags.removed {
        let entry = by_tag.entry(a.tag_id).or_default();
        entry.removed += 1;
    }
    MediaTagAssocDistribution {
        by_tag_top: top_n_i64(&by_tag, DISTRIBUTION_TOP_N),
    }
}

fn summarize_attributes(attributes: &AttributeDiff) -> AttributeDistribution {
    let mut by_key: BTreeMap<String, DiffCounts> = BTreeMap::new();
    for a in &attributes.added {
        bump(&mut by_key, &a.key, "added");
    }
    for a in &attributes.removed {
        bump(&mut by_key, &a.key, "removed");
    }
    for c in &attributes.changed {
        bump(&mut by_key, &c.current.key, "changed");
    }
    AttributeDistribution { by_key }
}

fn summarize_hashes(hashes: &HashDiff) -> HashDistribution {
    let mut by_filename: BTreeMap<String, DiffCounts> = BTreeMap::new();
    for h in &hashes.added {
        bump(&mut by_filename, &h.filename, "added");
    }
    for h in &hashes.removed {
        bump(&mut by_filename, &h.filename, "removed");
    }
    for c in &hashes.changed {
        bump(&mut by_filename, &c.current.filename, "changed");
    }
    HashDistribution {
        by_filename_top: top_n_string(&by_filename, DISTRIBUTION_TOP_N),
    }
}

// ---------- LLM explanation prompt 生成（設計 §4.4/§15-15）----------

/// `build_diff_explanation_prompt` のオプション
#[derive(Debug, Clone)]
pub struct DiffExplanationPromptOptions {
    /// 各セクションの代表サンプル最大件数（token 節約）
    pub max_samples_per_section: usize,
    /// 分布セクションを含めるか
    pub include_distribution: bool,
    /// 呼出側の任意メタ（session ID 等）。プロンプト先頭に記載
    pub extra_context: Option<String>,
}

impl Default for DiffExplanationPromptOptions {
    fn default() -> Self {
        Self {
            max_samples_per_section: 10,
            include_distribution: true,
            extra_context: None,
        }
    }
}

/// prod/stg 差分から LLM review 用の explanation prompt を生成する（設計 §4.4/§15-15）。
///
/// 変更内容・影響・リスク・promote 可否の根拠を LLM に説明させる日本語プロンプトを返す。
/// 必ずセマンティクス注記（added=promoteでprod追加 等）を含み、LLM の方向誤認を防ぐ。
pub fn build_diff_explanation_prompt(
    diff: &BackupDiff,
    summary: &ProdStgDiffSummary,
    options: &DiffExplanationPromptOptions,
) -> String {
    use std::fmt::Write;

    let max = options.max_samples_per_section;
    let mut out = String::new();

    writeln!(out, "# メディアDB変更レビュー（stg → prod promote 判断）").unwrap();
    writeln!(out).unwrap();
    if let Some(ctx) = &options.extra_context {
        writeln!(out, "コンテキスト: {}", ctx).unwrap();
        writeln!(out).unwrap();
    }
    writeln!(out, "あなたはメディアDBの変更レビュアーです。stg（作業用コピー）で行われた編集を").unwrap();
    writeln!(out, "prod（本番）へ promote してよいか判断しています。以下の差分データを元に、").unwrap();
    writeln!(out, "変更内容・影響・リスク・promote 可否の根拠を簡潔に説明してください。").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "> セマンティクス注記: added=stg新規（promote で prod に追加）・").unwrap();
    writeln!(out, "> removed=prod のみ（promote で prod から削除）・").unwrap();
    writeln!(out, "> changed=両方で異なる（promote で prod が上書き）").unwrap();
    writeln!(out).unwrap();

    // 1. 変更サマリ（件数表）
    let c = &summary.counts;
    writeln!(out, "## 1. 変更サマリ（件数）").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "| テーブル | added | removed | changed |").unwrap();
    writeln!(out, "|----------|-------|---------|---------|").unwrap();
    writeln!(out, "| media    | {} | {} | {} |", c.media.added, c.media.removed, c.media.changed).unwrap();
    writeln!(out, "| tags     | {} | {} | {} |", c.tags.added, c.tags.removed, c.tags.changed).unwrap();
    writeln!(out, "| media_tags | {} | {} | {} |", c.media_tags.added, c.media_tags.removed, c.media_tags.changed).unwrap();
    writeln!(out, "| attributes | {} | {} | {} |", c.attributes.added, c.attributes.removed, c.attributes.changed).unwrap();
    writeln!(out, "| hashes   | {} | {} | {} |", c.hashes.added, c.hashes.removed, c.hashes.changed).unwrap();
    writeln!(out).unwrap();
    writeln!(out, "合計: +{} -{} ~{}", summary.totals.added, summary.totals.removed, summary.totals.changed).unwrap();
    writeln!(out).unwrap();

    // 2. 代表サンプル
    writeln!(out, "## 2. 変更内容の代表サンプル（各最大{}件）", max).unwrap();
    writeln!(out).unwrap();
    write_media_samples(&mut out, &diff.media, max);
    write_tag_samples(&mut out, &diff.tags, max);
    write_assoc_samples(&mut out, &diff.media_tags, max);
    write_attribute_samples(&mut out, &diff.attributes, max);
    write_hash_samples(&mut out, &diff.hashes, max);

    // 3. 分布
    let mut section_no = 3;
    if options.include_distribution {
        writeln!(out, "## 3. 分布（テーブル別の偏り・異常変動の検出）").unwrap();
        writeln!(out).unwrap();
        write_distribution(&mut out, &summary.distribution);
        section_no = 4;
    }

    // 4. 説明指示
    writeln!(out, "## {}. 説明指示", section_no).unwrap();
    writeln!(out).unwrap();
    writeln!(out, "以下の4項目を Markdown 見出し（###）で説明してください:").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "### 変更内容の要約").unwrap();
    writeln!(out, "（何が起きるかを1-2段落で）").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "### 影響範囲").unwrap();
    writeln!(out, "（どのデータ範囲・機能に影響するか）").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "### リスク").unwrap();
    writeln!(out, "（データ整合性・FK・意図しない削除・異常な大量変動等。特に分布の偏りに注意）").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "### promote 可否の根拠").unwrap();
    writeln!(out, "（実施すべきか・保留すべきか・その理由。promote 不可なら必須条件を明示）").unwrap();

    out
}

fn write_media_samples(out: &mut String, media: &MediaDiff, max: usize) {
    use std::fmt::Write;
    writeln!(out, "### media (added: 最大{}件)", max).unwrap();
    for m in media.added.iter().take(max) {
        writeln!(
            out,
            "- id={} title=\"{}\" type={} artist={}",
            m.id,
            m.title,
            m.media_type.as_str(),
            m.artist.as_deref().unwrap_or("unknown")
        )
        .unwrap();
    }
    writeln!(out).unwrap();
    writeln!(out, "### media (removed: 最大{}件)", max).unwrap();
    for m in media.removed.iter().take(max) {
        writeln!(out, "- id={} title=\"{}\" type={}", m.id, m.title, m.media_type.as_str()).unwrap();
    }
    writeln!(out).unwrap();
    writeln!(out, "### media (changed: 最大{}件)", max).unwrap();
    for c in media.changed.iter().take(max) {
        writeln!(out, "- id={} title: \"{}\" -> \"{}\"", c.current.id, c.current.title, c.backup.title).unwrap();
    }
    writeln!(out).unwrap();
}

fn write_tag_samples(out: &mut String, tags: &TagDiff, max: usize) {
    use std::fmt::Write;
    writeln!(out, "### tags (added: 最大{}件)", max).unwrap();
    for t in tags.added.iter().take(max) {
        writeln!(out, "- id={} name=\"{}\"", t.id, t.name).unwrap();
    }
    writeln!(out).unwrap();
    writeln!(out, "### tags (removed: 最大{}件)", max).unwrap();
    for t in tags.removed.iter().take(max) {
        writeln!(out, "- id={} name=\"{}\"", t.id, t.name).unwrap();
    }
    writeln!(out).unwrap();
    writeln!(out, "### tags (changed: 最大{}件)", max).unwrap();
    for c in tags.changed.iter().take(max) {
        writeln!(out, "- id={} name: \"{}\" -> \"{}\"", c.current.id, c.current.name, c.backup.name).unwrap();
    }
    writeln!(out).unwrap();
}

fn write_assoc_samples(out: &mut String, media_tags: &MediaTagAssocDiff, max: usize) {
    use std::fmt::Write;
    writeln!(out, "### media_tags (added: 最大{}件)", max).unwrap();
    for a in media_tags.added.iter().take(max) {
        writeln!(out, "- media_id={} tag_id={}", a.media_id, a.tag_id).unwrap();
    }
    writeln!(out).unwrap();
    writeln!(out, "### media_tags (removed: 最大{}件)", max).unwrap();
    for a in media_tags.removed.iter().take(max) {
        writeln!(out, "- media_id={} tag_id={}", a.media_id, a.tag_id).unwrap();
    }
    writeln!(out).unwrap();
}

fn write_attribute_samples(out: &mut String, attributes: &AttributeDiff, max: usize) {
    use std::fmt::Write;
    writeln!(out, "### attributes (added: 最大{}件)", max).unwrap();
    for a in attributes.added.iter().take(max) {
        writeln!(out, "- media_id={} key=\"{}\" value={:?}", a.media_id, a.key, a.value).unwrap();
    }
    writeln!(out).unwrap();
    writeln!(out, "### attributes (removed: 最大{}件)", max).unwrap();
    for a in attributes.removed.iter().take(max) {
        writeln!(out, "- media_id={} key=\"{}\"", a.media_id, a.key).unwrap();
    }
    writeln!(out).unwrap();
    writeln!(out, "### attributes (changed: 最大{}件)", max).unwrap();
    for c in attributes.changed.iter().take(max) {
        writeln!(
            out,
            "- media_id={} key=\"{}\": {:?} -> {:?}",
            c.current.media_id, c.current.key, c.current.value, c.backup.value
        )
        .unwrap();
    }
    writeln!(out).unwrap();
}

fn write_hash_samples(out: &mut String, hashes: &HashDiff, max: usize) {
    use std::fmt::Write;
    writeln!(out, "### hashes (added: 最大{}件)", max).unwrap();
    for h in hashes.added.iter().take(max) {
        writeln!(out, "- item_uuid={} filename=\"{}\"", h.item_uuid, h.filename).unwrap();
    }
    writeln!(out).unwrap();
    writeln!(out, "### hashes (removed: 最大{}件)", max).unwrap();
    for h in hashes.removed.iter().take(max) {
        writeln!(out, "- item_uuid={} filename=\"{}\"", h.item_uuid, h.filename).unwrap();
    }
    writeln!(out).unwrap();
    writeln!(out, "### hashes (changed: 最大{}件)", max).unwrap();
    for c in hashes.changed.iter().take(max) {
        writeln!(out, "- item_uuid={} filename=\"{}\"", c.current.item_uuid, c.current.filename).unwrap();
    }
    writeln!(out).unwrap();
}

fn write_counts_line(out: &mut String, label: &str, counts: &DiffCounts) {
    use std::fmt::Write;
    let _ = writeln!(out, "- {}: +{} -{} ~{}", label, counts.added, counts.removed, counts.changed);
}

fn write_distribution(out: &mut String, dist: &DiffDistribution) {
    use std::fmt::Write;

    writeln!(out, "### media").unwrap();
    for (k, v) in &dist.media.by_type {
        write_counts_line(out, &format!("type={}", k), v);
    }
    for (k, v) in &dist.media.by_artist_top {
        write_counts_line(out, &format!("artist={}", k), v);
    }
    for (k, v) in &dist.media.by_flag_exist {
        write_counts_line(out, &format!("flag_exist={}", k), v);
    }
    writeln!(out).unwrap();

    writeln!(out, "### media_tags").unwrap();
    for (k, v) in &dist.media_tags.by_tag_top {
        write_counts_line(out, &format!("tag_id={}", k), v);
    }
    writeln!(out).unwrap();

    writeln!(out, "### attributes").unwrap();
    for (k, v) in &dist.attributes.by_key {
        write_counts_line(out, &format!("key={}", k), v);
    }
    writeln!(out).unwrap();

    writeln!(out, "### hashes").unwrap();
    for (k, v) in &dist.hashes.by_filename_top {
        write_counts_line(out, &format!("filename={}", k), v);
    }
    writeln!(out).unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(id: i64, name: &str) -> Tag {
        Tag { id, name: name.to_string() }
    }

    #[test]
    fn test_compute_diff_no_changes() {
        let mut snap = BackupSnapshot::default();
        snap.tags.insert(1, tag(1, "A"));
        let opts = DiffOptions::default();
        let diff = compute_diff(&snap, &snap, &opts);
        assert_eq!(diff.summary.tags.added, 0);
        assert_eq!(diff.summary.tags.removed, 0);
        assert_eq!(diff.summary.tags.changed, 0);
    }

    #[test]
    fn test_compute_diff_tag_added_removed_changed() {
        // diff_media/diff_tag/diff_attribute/diff_hash は同構造なので Tag で代表検証
        let mut current = BackupSnapshot::default();
        current.tags.insert(1, tag(1, "A-current")); // changed
        current.tags.insert(2, tag(2, "B")); // removed (現在にのみ)
        let mut backup = BackupSnapshot::default();
        backup.tags.insert(1, tag(1, "A-backup")); // changed
        backup.tags.insert(3, tag(3, "C")); // added (バックアップにのみ)

        let opts = DiffOptions { detail: Some(DiffDetail::Full) };
        let diff = compute_diff(&current, &backup, &opts);
        assert_eq!(diff.summary.tags.added, 1);
        assert_eq!(diff.summary.tags.removed, 1);
        assert_eq!(diff.summary.tags.changed, 1);
        assert_eq!(diff.tags.added[0].id, 3);
        assert_eq!(diff.tags.removed[0].id, 2);
        assert_eq!(diff.tags.changed[0].current.name, "A-current");
        assert_eq!(diff.tags.changed[0].backup.name, "A-backup");
    }

    #[test]
    fn test_compute_diff_assoc() {
        let mut current = BackupSnapshot::default();
        current.media_tags.insert((1, 10));
        current.media_tags.insert((2, 20));
        let mut backup = BackupSnapshot::default();
        backup.media_tags.insert((2, 20));
        backup.media_tags.insert((3, 30));

        let opts = DiffOptions { detail: Some(DiffDetail::Full) };
        let diff = compute_diff(&current, &backup, &opts);
        assert_eq!(diff.summary.media_tags.added, 1); // (3,30)
        assert_eq!(diff.summary.media_tags.removed, 1); // (1,10)
        assert_eq!(diff.summary.media_tags.changed, 0);
    }

    #[test]
    fn test_apply_detail_summary_only_empties_lists() {
        let mut current = BackupSnapshot::default();
        current.tags.insert(1, tag(1, "A"));
        let backup = BackupSnapshot::default();
        let opts = DiffOptions { detail: Some(DiffDetail::SummaryOnly) };
        let diff = compute_diff(&current, &backup, &opts);
        // summary は件数を保持するが、リストは空
        assert_eq!(diff.summary.tags.removed, 1);
        assert!(diff.tags.removed.is_empty());
    }

    #[test]
    fn test_apply_detail_limited_truncates() {
        let mut current = BackupSnapshot::default();
        for i in 0..10 {
            current.tags.insert(i, tag(i, "x"));
        }
        let backup = BackupSnapshot::default();
        let opts = DiffOptions { detail: Some(DiffDetail::Limited { n: 3 }) };
        let diff = compute_diff(&current, &backup, &opts);
        assert_eq!(diff.summary.tags.removed, 10);
        assert_eq!(diff.tags.removed.len(), 3); // 上位3件
    }

    // ========== summarize_diff / build_diff_explanation_prompt（TASK-53）==========

    fn fixed_dt() -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::from_timestamp(0, 0).unwrap()
    }

    fn media(
        id: i64,
        title: &str,
        media_type: crate::types::MediaType,
        artist: Option<&str>,
        flag_exist: bool,
    ) -> Media {
        Media {
            id,
            uuid: format!("uuid-{}", id),
            title: title.to_string(),
            title_id: None,
            path: None,
            media_type,
            thumbnail_path: None,
            artist: artist.map(|s| s.to_string()),
            artist_id: None,
            description: None,
            file_size: None,
            duration_sec: None,
            page_count: None,
            series: None,
            volume_number: None,
            volume_text: None,
            volume_title: None,
            magazine: None,
            magazine_id: None,
            language: None,
            source: None,
            external_id: None,
            artist_en: None,
            title_en: None,
            chapters: None,
            extension: None,
            flag_exist,
            created_at: fixed_dt(),
            updated_at: fixed_dt(),
            title_pron: None,
            artist_pron: None,
            series_pron: None,
        }
    }

    #[test]
    fn test_summarize_diff_totals_and_media_distribution() {
        use crate::types::MediaType;
        // prod 空、stg に media 3件（Comic x2 / Video x1）→ compute_diff(current=prod, backup=stg) で added
        let prod = BackupSnapshot::default();
        let mut stg = BackupSnapshot::default();
        stg.media.insert(1, media(1, "c1", MediaType::Comic, Some("A"), true));
        stg.media.insert(2, media(2, "c2", MediaType::Comic, Some("A"), false));
        stg.media.insert(3, media(3, "v1", MediaType::Video, None, true));

        let diff = compute_diff(&prod, &stg, &DiffOptions { detail: Some(DiffDetail::Full) });
        let summary = summarize_diff(&diff);

        // totals: media added=3 のみ（他テーブル空）
        assert_eq!(summary.totals.added, 3);
        assert_eq!(summary.totals.removed, 0);
        assert_eq!(summary.totals.changed, 0);

        // by_type: comic=2, video=1
        assert_eq!(summary.distribution.media.by_type.get("comic").unwrap().added, 2);
        assert_eq!(summary.distribution.media.by_type.get("video").unwrap().added, 1);

        // by_artist: A=2（上位 N に含まれる）
        let artist_a = summary
            .distribution
            .media
            .by_artist_top
            .iter()
            .find(|(k, _)| k == "A")
            .expect("artist A in top");
        assert_eq!(artist_a.1.added, 2);

        // by_flag_exist: true=2, false=1
        assert_eq!(summary.distribution.media.by_flag_exist.get("true").unwrap().added, 2);
        assert_eq!(summary.distribution.media.by_flag_exist.get("false").unwrap().added, 1);
    }

    #[test]
    fn test_summarize_diff_attribute_distribution_by_key() {
        use crate::types::{AttributeValueType, MediaAttribute};
        let prod = BackupSnapshot::default();
        let mut stg = BackupSnapshot::default();
        for (mid, key) in [(1i64, "rating"), (2, "rating"), (3, "source")] {
            stg.attributes.insert(
                (mid, key.to_string()),
                MediaAttribute {
                    media_id: mid,
                    key: key.to_string(),
                    value: None,
                    value_type: AttributeValueType::String,
                },
            );
        }
        let diff = compute_diff(&prod, &stg, &DiffOptions { detail: Some(DiffDetail::Full) });
        let summary = summarize_diff(&diff);

        assert_eq!(summary.distribution.attributes.by_key.get("rating").unwrap().added, 2);
        assert_eq!(summary.distribution.attributes.by_key.get("source").unwrap().added, 1);
    }

    #[test]
    fn test_build_diff_explanation_prompt_has_sections() {
        use crate::types::MediaType;
        let prod = BackupSnapshot::default();
        let mut stg = BackupSnapshot::default();
        stg.media.insert(1, media(1, "new", MediaType::Comic, None, true));

        let diff = compute_diff(&prod, &stg, &DiffOptions::default());
        let summary = summarize_diff(&diff);
        let prompt = build_diff_explanation_prompt(&diff, &summary, &DiffExplanationPromptOptions::default());

        assert!(prompt.contains("# メディアDB変更レビュー"));
        assert!(prompt.contains("## 1. 変更サマリ"));
        assert!(prompt.contains("## 2. 変更内容の代表サンプル"));
        assert!(prompt.contains("## 3. 分布"));
        assert!(prompt.contains("## 4. 説明指示"));
        assert!(prompt.contains("### 変更内容の要約"));
        assert!(prompt.contains("### 影響範囲"));
        assert!(prompt.contains("### リスク"));
        assert!(prompt.contains("### promote 可否の根拠"));
    }

    #[test]
    fn test_build_diff_explanation_prompt_empty() {
        let snap = BackupSnapshot::default();
        let diff = compute_diff(&snap, &snap, &DiffOptions::default());
        let summary = summarize_diff(&diff);
        let prompt = build_diff_explanation_prompt(&diff, &summary, &DiffExplanationPromptOptions::default());

        assert!(prompt.contains("合計: +0 -0 ~0"));
    }

    #[test]
    fn test_build_diff_explanation_prompt_semantics_note() {
        let snap = BackupSnapshot::default();
        let diff = compute_diff(&snap, &snap, &DiffOptions::default());
        let summary = summarize_diff(&diff);
        let prompt = build_diff_explanation_prompt(&diff, &summary, &DiffExplanationPromptOptions::default());

        assert!(prompt.contains("added=stg新規"));
        assert!(prompt.contains("removed=prod のみ"));
        assert!(prompt.contains("changed=両方で異なる"));
    }

    #[test]
    fn test_build_diff_explanation_prompt_max_samples() {
        use crate::types::MediaType;
        let prod = BackupSnapshot::default();
        let mut stg = BackupSnapshot::default();
        for i in 1..=15i64 {
            stg.media.insert(i, media(i, "m", MediaType::Comic, None, true));
        }
        let diff = compute_diff(&prod, &stg, &DiffOptions { detail: Some(DiffDetail::Full) });
        let summary = summarize_diff(&diff);
        let prompt = build_diff_explanation_prompt(
            &diff,
            &summary,
            &DiffExplanationPromptOptions {
                max_samples_per_section: 5,
                ..Default::default()
            },
        );

        // media added サンプル（`- id=N title=`）は最大5件
        let added_count = prompt
            .lines()
            .filter(|l| l.starts_with("- id=") && l.contains("title="))
            .count();
        assert_eq!(added_count, 5);
    }

    #[test]
    fn test_build_diff_explanation_prompt_skips_distribution() {
        use crate::types::MediaType;
        let prod = BackupSnapshot::default();
        let mut stg = BackupSnapshot::default();
        stg.media.insert(1, media(1, "new", MediaType::Comic, None, true));

        let diff = compute_diff(&prod, &stg, &DiffOptions::default());
        let summary = summarize_diff(&diff);
        let prompt = build_diff_explanation_prompt(
            &diff,
            &summary,
            &DiffExplanationPromptOptions {
                include_distribution: false,
                ..Default::default()
            },
        );

        assert!(!prompt.contains("## 3. 分布"));
        assert!(prompt.contains("## 3. 説明指示"));
    }
}
