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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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
}
