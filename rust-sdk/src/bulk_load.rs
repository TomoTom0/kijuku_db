//! ローカル→D1 のバルクロード（全データ完全移行）ツール。
//!
//! 既存のローカル SQLite（NAS 等）のデータを別バックエンド（典型的には D1）へ
//! REST 経由で完全に移行する。読み出し元と書き込み先はどちらも `KijukuBackend` で
//! 扱い、`media` / `tags` / `media_tags` / `media_attributes` / `media_hashes` の
//! **全データを欠落させず**運び、件数・内容一致を検証する。
//!
//! D1 側では `id` が再採番されるため、`uuid`（media）と タグ名（tags）をキーに
//! 新旧 id の対応表を作って関連（media_tags / attributes）を張り直す。
//! `media_hashes` は `item_uuid` 基準で uuid が保持されるため、そのまま転送できる。
//!
//! 一次性の移行ユーティリティであり、定常同期ではない。`dest` は空（移行先）
//! を前提とする（media の uuid は UNIQUE 制約があるため重複すると失敗する）。

use crate::backend::KijukuBackend;
use crate::error::{KijukuError, Result};
use crate::types::{
    Media, MediaFilter, MediaHash, MediaHashInput, MediaInput, QueryOptions, SortKey, SortOrder,
};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};

/// バルクロードの転送結果。各カテゴリの転送件数と、検証成否を持つ。
#[derive(Debug, Clone, Serialize)]
pub struct TransferReport {
    /// 転送した media 件数（= source 件数）
    pub media: usize,
    /// 転送した tags 件数（= source 件数）
    pub tags: usize,
    /// 転送した media_tags 関連（リンク）件数
    pub media_tags: usize,
    /// 転送した media_attributes 件数
    pub attributes: usize,
    /// 転送した media_hashes 件数
    pub hashes: usize,
    /// 移行後の件数・内容検証が通ったか
    pub verified: bool,
}

/// バルクロードのオプション。
#[derive(Debug, Clone)]
pub struct TransferOptions {
    /// source の media 読み出しをページングする件数。
    pub chunk_size: usize,
    /// 進捗を stderr に出力するか。
    pub verbose: bool,
}

impl Default for TransferOptions {
    fn default() -> Self {
        Self {
            chunk_size: 500,
            verbose: false,
        }
    }
}

/// `Media` を新規作成用の `MediaInput` へ変換（`uuid` を保持して完全転送）。
///
/// `volume_number` は `volume_text` から自動計算されるためコピーしない
/// （source 側も同じ計算で導出されているはずなので、検証時には一致する）。
fn media_to_input(m: &Media) -> MediaInput {
    MediaInput {
        title: m.title.clone(),
        media_type: m.media_type,
        uuid: Some(m.uuid.clone()),
        title_id: m.title_id.clone(),
        path: m.path.clone(),
        thumbnail_path: m.thumbnail_path.clone(),
        artist: m.artist.clone(),
        artist_id: m.artist_id.clone(),
        description: m.description.clone(),
        file_size: m.file_size,
        duration_sec: m.duration_sec,
        page_count: m.page_count,
        series: m.series.clone(),
        volume_number: None, // volume_text から自動計算されるため無視
        volume_text: m.volume_text.clone(),
        volume_title: m.volume_title.clone(),
        magazine: m.magazine.clone(),
        magazine_id: m.magazine_id.clone(),
        language: m.language.clone(),
        source: m.source.clone(),
        external_id: m.external_id.clone(),
        artist_en: m.artist_en.clone(),
        title_en: m.title_en.clone(),
        chapters: m.chapters.clone(),
        extension: m.extension.clone(),
        flag_exist: Some(m.flag_exist),
        title_pron: m.title_pron.clone(),
        artist_pron: m.artist_pron.clone(),
        series_pron: m.series_pron.clone(),
    }
}

/// `MediaHash` を新規登録用の `MediaHashInput` へ変換。
fn hash_to_input(h: &MediaHash) -> MediaHashInput {
    MediaHashInput {
        item_uuid: h.item_uuid.clone(),
        filename: h.filename.clone(),
        time_range: h.time_range.clone(),
        content_hash: h.content_hash.clone(),
        alternative_of: h.alternative_of.clone(),
    }
}

fn log_verbose(opts: &TransferOptions, msg: &str) {
    if opts.verbose {
        eprintln!("[bulk-load] {}", msg);
    }
}

/// source の全データを dest へ完全移行し、件数・内容一致を検証する。
///
/// `dest` は空（移行先）を前提とする。順序: tags → media（ページごとに、
/// その media の media_tags / attributes / hashes も併せて転送）→ 検証。
pub async fn transfer(
    source: &dyn KijukuBackend,
    dest: &dyn KijukuBackend,
    opts: &TransferOptions,
) -> Result<TransferReport> {
    // dest のスキーマを保証（冪等）。source は既存データを持つため未変更を想定。
    dest.migrate().await?;

    let mut report = TransferReport {
        media: 0,
        tags: 0,
        media_tags: 0,
        attributes: 0,
        hashes: 0,
        verified: false,
    };

    // ---- Pass A: tags ----
    // タグは media_tags より先に作成する。get-or-create で部分的な再実行にも耐える。
    let src_tags = source.get_all_tags().await?;
    let mut tag_name_to_id: HashMap<String, i64> = HashMap::new();
    for t in &src_tags {
        let new_id = match dest.get_tag_by_name(&t.name).await? {
            Some(existing) => existing.id,
            None => dest.create_tag(&t.name).await?.id,
        };
        tag_name_to_id.insert(t.name.clone(), new_id);
    }
    report.tags = src_tags.len();
    log_verbose(opts, &format!("tags: {} 件転送", report.tags));

    // ---- Pass B: media（ページング）+ 関連データ ----
    let chunk = opts.chunk_size.max(1);
    let mut uuid_to_new_id: HashMap<String, i64> = HashMap::new();
    let mut offset = 0i64;
    loop {
        let page_opts = QueryOptions {
            sort_keys: vec![SortKey {
                field: "id".to_string(),
                order: SortOrder::Asc,
            }],
            limit: Some(chunk as i64),
            offset: Some(offset),
        };
        let page = source
            .find_media(&MediaFilter::default(), Some(&page_opts))
            .await?;
        let n = page.len();
        if n == 0 {
            break;
        }

        // media 本体（uuid 保持で一括作成）
        let inputs: Vec<MediaInput> = page.iter().map(media_to_input).collect();
        let created = dest.bulk_create_media(&inputs).await?;
        for c in &created {
            uuid_to_new_id.insert(c.uuid.clone(), c.id);
        }
        report.media += n;

        // 各 media の関連データを転送
        for src in &page {
            let new_media_id = *uuid_to_new_id
                .get(&src.uuid)
                .ok_or_else(|| KijukuError::Other(format!("uuid {} の新 id が不明", src.uuid)))?;

            // media_tags
            let tags = source.get_media_tags(src.id).await?;
            for tg in &tags {
                if let Some(&new_tag_id) = tag_name_to_id.get(&tg.name) {
                    dest.add_tag_to_media(new_media_id, new_tag_id).await?;
                    report.media_tags += 1;
                } else {
                    return Err(KijukuError::Other(format!(
                        "media(id={}) のタグ '{}' に対応する dest タグがない",
                        src.id, tg.name
                    )));
                }
            }

            // attributes
            let attrs = source.get_media_attributes(src.id).await?;
            for a in &attrs {
                dest.set_media_attribute(
                    new_media_id,
                    &a.key,
                    a.value.as_deref(),
                    Some(a.value_type),
                )
                .await?;
                report.attributes += 1;
            }

            // hashes（uuid 基準・そのまま）
            let hashes = source.get_media_hashes(&src.uuid).await?;
            if !hashes.is_empty() {
                let hinputs: Vec<MediaHashInput> = hashes.iter().map(hash_to_input).collect();
                dest.add_media_hashes(&hinputs).await?;
                report.hashes += hashes.len();
            }
        }

        log_verbose(
            opts,
            &format!("media: {} 件まで転送済み", report.media),
        );

        offset += n as i64;
        if n < chunk {
            break; // 最終ページ
        }
    }

    // ---- 検証 ----
    verify(source, dest).await?;
    report.verified = true;
    Ok(report)
}

/// source と dest の全データの件数・内容一致を検証し、結果レポートを返す。
/// 不一致があれば `Err`。`transfer` の終段でも呼ばれるが、`--verify-only` で
/// 移行なしに再検証する際にも単独で使える。
pub async fn verify(
    source: &dyn KijukuBackend,
    dest: &dyn KijukuBackend,
) -> Result<TransferReport> {
    let mut report = TransferReport {
        media: 0,
        tags: 0,
        media_tags: 0,
        attributes: 0,
        hashes: 0,
        verified: false,
    };

    // --- media: 件数 + 内容 ---
    let src_media = source
        .find_media(&MediaFilter::default(), None)
        .await?;
    let dst_media = dest
        .find_media(&MediaFilter::default(), None)
        .await?;
    report.media = src_media.len();
    if src_media.len() != dst_media.len() {
        return Err(KijukuError::Other(format!(
            "media 件数不一致: source={} dest={}",
            src_media.len(),
            dst_media.len()
        )));
    }
    let dst_by_uuid: HashMap<&str, &Media> =
        dst_media.iter().map(|m| (m.uuid.as_str(), m)).collect();
    for s in &src_media {
        let d = dst_by_uuid.get(s.uuid.as_str()).ok_or_else(|| {
            KijukuError::Other(format!("dest に uuid={} の media がない", s.uuid))
        })?;
        if !media_content_eq(s, d) {
            return Err(KijukuError::Other(format!(
                "media 内容不一致 (uuid={}): title source={:?} dest={:?}",
                s.uuid, s.title, d.title
            )));
        }
    }

    // --- tags: 件数 + 名前集合 ---
    let src_tags = source.get_all_tags().await?;
    let dst_tags = dest.get_all_tags().await?;
    report.tags = src_tags.len();
    let src_tag_names: std::collections::BTreeSet<String> =
        src_tags.iter().map(|t| t.name.clone()).collect();
    let dst_tag_names: std::collections::BTreeSet<String> =
        dst_tags.iter().map(|t| t.name.clone()).collect();
    if src_tag_names != dst_tag_names {
        return Err(KijukuError::Other(format!(
            "tags 不一致: source={:?} dest={:?}",
            src_tag_names, dst_tag_names
        )));
    }

    // --- media_tags / attributes / hashes: 各 media で内容比較 ---
    for s in &src_media {
        // media_tags（タグ名の集合で比較）
        let s_tags = source.get_media_tags(s.id).await?;
        let d = dst_by_uuid.get(s.uuid.as_str()).unwrap();
        let d_tags = dest.get_media_tags(d.id).await?;
        let s_names: std::collections::BTreeSet<String> =
            s_tags.iter().map(|t| t.name.clone()).collect();
        let d_names: std::collections::BTreeSet<String> =
            d_tags.iter().map(|t| t.name.clone()).collect();
        if s_names != d_names {
            return Err(KijukuError::Other(format!(
                "media_tags 不一致 (uuid={}): source={:?} dest={:?}",
                s.uuid, s_names, d_names
            )));
        }
        report.media_tags += s_tags.len();

        // attributes（key, value, value_type の集合で比較）
        let s_attrs = source.get_media_attributes(s.id).await?;
        let d_attrs = dest.get_media_attributes(d.id).await?;
        let s_attr_set: BTreeMap<(String, Option<String>, String), ()> = s_attrs
            .iter()
            .map(|a| ((a.key.clone(), a.value.clone(), a.value_type.as_str().to_string()), ()))
            .collect();
        let d_attr_set: BTreeMap<(String, Option<String>, String), ()> = d_attrs
            .iter()
            .map(|a| ((a.key.clone(), a.value.clone(), a.value_type.as_str().to_string()), ()))
            .collect();
        if s_attr_set != d_attr_set {
            return Err(KijukuError::Other(format!(
                "attributes 不一致 (uuid={})",
                s.uuid
            )));
        }
        report.attributes += s_attrs.len();

        // hashes（filename, time_range, content_hash, alternative_of の集合で比較）
        let s_hashes = source.get_media_hashes(&s.uuid).await?;
        let d_hashes = dest.get_media_hashes(&s.uuid).await?;
        let to_key = |h: &MediaHash| {
            (
                h.filename.clone(),
                h.time_range.clone(),
                h.content_hash.clone(),
                h.alternative_of.clone(),
            )
        };
        let s_hash_set: std::collections::BTreeSet<_> = s_hashes.iter().map(to_key).collect();
        let d_hash_set: std::collections::BTreeSet<_> = d_hashes.iter().map(to_key).collect();
        if s_hash_set != d_hash_set {
            return Err(KijukuError::Other(format!(
                "hashes 不一致 (uuid={})",
                s.uuid
            )));
        }
        report.hashes += s_hashes.len();
    }

    report.verified = true;
    Ok(report)
}

/// `Media` の内容が、`id` / `created_at` / `updated_at`（dest 側で再採番・再設定される）を
/// 除いて等しいか。
fn media_content_eq(a: &Media, b: &Media) -> bool {
    a.uuid == b.uuid
        && a.title == b.title
        && a.title_id == b.title_id
        && a.path == b.path
        && a.media_type == b.media_type
        && a.thumbnail_path == b.thumbnail_path
        && a.artist == b.artist
        && a.artist_id == b.artist_id
        && a.description == b.description
        && a.file_size == b.file_size
        && a.duration_sec == b.duration_sec
        && a.page_count == b.page_count
        && a.series == b.series
        && a.volume_number == b.volume_number
        && a.volume_text == b.volume_text
        && a.volume_title == b.volume_title
        && a.magazine == b.magazine
        && a.magazine_id == b.magazine_id
        && a.language == b.language
        && a.source == b.source
        && a.external_id == b.external_id
        && a.artist_en == b.artist_en
        && a.title_en == b.title_en
        && a.chapters == b.chapters
        && a.extension == b.extension
        && a.flag_exist == b.flag_exist
        && a.title_pron == b.title_pron
        && a.artist_pron == b.artist_pron
        && a.series_pron == b.series_pron
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{AttributeValueType, MediaHashInput, MediaInput, MediaType};
    use crate::KijukuDB;

    /// source にサンプルデータ（media×2, tags×2, media_tags×3, attributes×2, hashes×1）を詰める。
    /// 呼び出しは全て `&dyn KijukuBackend` 経由で async trait メソッドを行使する
    /// （KijukuDB の inherent 同期メソッドとの名前衝突を避けるため）。
    async fn seed(db: &dyn KijukuBackend) {
        let m1 = db
            .create_media(&MediaInput {
                title: "作品1".to_string(),
                media_type: MediaType::Comic,
                artist: Some("作家A".to_string()),
                volume_text: Some("3".to_string()),
                ..Default::default()
            })
            .await
            .unwrap();
        let m2 = db
            .create_media(&MediaInput {
                title: "作品2".to_string(),
                media_type: MediaType::Video,
                ..Default::default()
            })
            .await
            .unwrap();

        let t1 = db.create_tag("タグ1").await.unwrap();
        let t2 = db.create_tag("タグ2").await.unwrap();
        db.add_tag_to_media(m1.id, t1.id).await.unwrap();
        db.add_tag_to_media(m2.id, t1.id).await.unwrap();
        db.add_tag_to_media(m1.id, t2.id).await.unwrap(); // media_tags 計3

        db.set_media_attribute(m1.id, "rating", Some("5"), Some(AttributeValueType::Integer))
            .await
            .unwrap();
        db.set_media_attribute(m1.id, "note", Some("メモ"), Some(AttributeValueType::String))
            .await
            .unwrap();

        // content_hash は SHA256(32byte) 想定
        db.add_media_hash(&MediaHashInput {
            item_uuid: m1.uuid.clone(),
            filename: "001.jpg".to_string(),
            time_range: String::new(),
            content_hash: vec![0x11u8; 32],
            alternative_of: None,
        })
        .await
        .unwrap();
    }

    fn source_pair() -> (KijukuDB, KijukuDB) {
        let source = KijukuDB::open_in_memory().unwrap();
        let dest = KijukuDB::open_in_memory().unwrap();
        (source, dest)
    }

    #[tokio::test]
    async fn test_transfer_full_local_to_local() {
        let (source, dest) = source_pair();
        let src: &dyn KijukuBackend = &source;
        src.migrate().await.unwrap();
        seed(src).await;

        let report = transfer(src, &dest, &TransferOptions::default()).await.unwrap();

        assert_eq!(report.media, 2);
        assert_eq!(report.tags, 2);
        assert_eq!(report.media_tags, 3);
        assert_eq!(report.attributes, 2);
        assert_eq!(report.hashes, 1);
        assert!(report.verified);
    }

    /// chunk_size=1 でページング経路を検証（順序・重複抜けなし）。
    #[tokio::test]
    async fn test_transfer_with_small_chunk() {
        let (source, dest) = source_pair();
        let src: &dyn KijukuBackend = &source;
        src.migrate().await.unwrap();
        seed(src).await;

        let report = transfer(
            src,
            &dest,
            &TransferOptions {
                chunk_size: 1,
                verbose: false,
            },
        )
        .await
        .unwrap();

        assert_eq!(report.media, 2);
        assert!(report.verified);
    }

    /// verify 単体: 移行後に verify() が通り、件数が取れること。
    #[tokio::test]
    async fn test_verify_standalone_after_transfer() {
        let (source, dest) = source_pair();
        let src: &dyn KijukuBackend = &source;
        src.migrate().await.unwrap();
        seed(src).await;
        transfer(src, &dest, &TransferOptions::default()).await.unwrap();

        let v = verify(src, &dest).await.unwrap();
        assert!(v.verified);
        assert_eq!(v.media, 2);
        assert_eq!(v.hashes, 1);
    }

    /// 移行前（dest 空）では verify が失敗すること。
    #[tokio::test]
    async fn test_verify_fails_before_transfer() {
        let (source, dest) = source_pair();
        let src: &dyn KijukuBackend = &source;
        src.migrate().await.unwrap();
        seed(src).await;

        let res = verify(src, &dest).await;
        assert!(res.is_err());
    }
}
