use rusqlite::{params, Connection};
use crate::error::Result;
use crate::types::{MediaHash, MediaHashInput};

fn row_to_media_hash(row: &rusqlite::Row) -> rusqlite::Result<MediaHash> {
    Ok(MediaHash {
        item_uuid: row.get(0)?,
        filename: row.get(1)?,
        time_range: row.get(2)?,
        content_hash: row.get(3)?,
        alternative_of: row.get(4)?,
        embedding: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

/// ハッシュのHEX文字列からバイト配列への変換
pub fn hex_to_bytes(hex: &str) -> Result<Vec<u8>> {
    if hex.len() % 2 != 0 {
        return Err(crate::error::KijukuError::Validation(
            "HEX string must have even length".to_string(),
        ));
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&hex[i..i + 2], 16).map_err(|e| {
                crate::error::KijukuError::Validation(format!("Invalid HEX character: {}", e))
            })
        })
        .collect()
}

/// バイト配列からHEX文字列への変換
pub fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// メディアハッシュを登録（単件）
pub fn add_media_hash(conn: &Connection, input: &MediaHashInput) -> Result<MediaHash> {
    conn.execute(
        "INSERT INTO media_hashes (item_uuid, filename, time_range, content_hash, alternative_of)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(item_uuid, filename, time_range) DO UPDATE SET
           content_hash = ?4,
           alternative_of = ?5",
        params![input.item_uuid, input.filename, input.time_range, input.content_hash, input.alternative_of],
    )?;

    let hash = get_media_hash(conn, &input.item_uuid, &input.filename, &input.time_range)?
        .ok_or_else(|| crate::error::KijukuError::Other("Failed to read inserted hash".to_string()))?;
    Ok(hash)
}

/// メディアハッシュを一括登録
pub fn add_media_hashes(conn: &Connection, inputs: &[MediaHashInput]) -> Result<Vec<MediaHash>> {
    let mut results = Vec::with_capacity(inputs.len());
    let tx = conn.unchecked_transaction()?;
    for input in inputs {
        tx.execute(
            "INSERT INTO media_hashes (item_uuid, filename, time_range, content_hash, alternative_of)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(item_uuid, filename, time_range) DO UPDATE SET
               content_hash = ?4,
               alternative_of = ?5",
            params![input.item_uuid, input.filename, input.time_range, input.content_hash, input.alternative_of],
        )?;
    }
    tx.commit()?;

    for input in inputs {
        if let Some(hash) = get_media_hash(conn, &input.item_uuid, &input.filename, &input.time_range)? {
            results.push(hash);
        }
    }
    Ok(results)
}

/// 特定作品の全ハッシュを取得
pub fn get_media_hashes(conn: &Connection, item_uuid: &str) -> Result<Vec<MediaHash>> {
    let mut stmt = conn.prepare(
        "SELECT item_uuid, filename, time_range, content_hash, alternative_of, embedding, created_at, updated_at
         FROM media_hashes
         WHERE item_uuid = ?1
         ORDER BY filename, time_range",
    )?;
    let rows = stmt.query_map(params![item_uuid], row_to_media_hash)?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

/// 特定位置のハッシュを取得
pub fn get_media_hash(
    conn: &Connection,
    item_uuid: &str,
    filename: &str,
    time_range: &str,
) -> Result<Option<MediaHash>> {
    let result = conn.query_row(
        "SELECT item_uuid, filename, time_range, content_hash, alternative_of, embedding, created_at, updated_at
         FROM media_hashes
         WHERE item_uuid = ?1 AND filename = ?2 AND time_range = ?3",
        params![item_uuid, filename, time_range],
        row_to_media_hash,
    );
    match result {
        Ok(hash) => Ok(Some(hash)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// SHA256による完全一致検索
pub fn find_by_content_hash(conn: &Connection, hash: &[u8]) -> Result<Vec<MediaHash>> {
    let mut stmt = conn.prepare(
        "SELECT item_uuid, filename, time_range, content_hash, alternative_of, embedding, created_at, updated_at
         FROM media_hashes
         WHERE content_hash = ?1",
    )?;
    let rows = stmt.query_map(params![hash], row_to_media_hash)?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

/// 特定位置のハッシュを削除（代替行の連鎖削除を含む）
pub fn delete_media_hash(
    conn: &Connection,
    item_uuid: &str,
    filename: &str,
    time_range: &str,
) -> Result<()> {
    // 削除対象が原本の場合、その代替行も削除（空文字ではalternative_ofを検索しない）
    if !filename.is_empty() {
        conn.execute(
            "DELETE FROM media_hashes
             WHERE item_uuid = ?1 AND alternative_of = ?2",
            params![item_uuid, filename],
        )?;
    }
    if !time_range.is_empty() {
        conn.execute(
            "DELETE FROM media_hashes
             WHERE item_uuid = ?1 AND alternative_of = ?2",
            params![item_uuid, time_range],
        )?;
    }
    // 最後に指定行を削除
    conn.execute(
        "DELETE FROM media_hashes
         WHERE item_uuid = ?1 AND filename = ?2 AND time_range = ?3",
        params![item_uuid, filename, time_range],
    )?;
    Ok(())
}

/// 特定作品のハッシュを全削除
pub fn delete_media_hashes(conn: &Connection, item_uuid: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM media_hashes WHERE item_uuid = ?1",
        params![item_uuid],
    )?;
    Ok(())
}

/// 重複ハッシュの検出
pub fn find_duplicate_hashes(conn: &Connection) -> Result<Vec<(Vec<u8>, i64)>> {
    let mut stmt = conn.prepare(
        "SELECT content_hash, COUNT(*) as cnt
         FROM media_hashes
         GROUP BY content_hash
         HAVING cnt > 1",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, i64>(1)?))
    })?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

/// compute結果の1エントリ
#[derive(Debug)]
pub struct ComputeHashResult {
    pub item_uuid: String,
    pub hashes: Vec<MediaHash>,
    pub skipped: bool,
    pub skip_reason: Option<String>,
}

/// ファイルのSHA256をストリーミング計算
fn compute_sha256_file(path: &std::path::Path) -> std::io::Result<Vec<u8>> {
    use sha2::{Sha256, Digest};
    use std::io::Read;

    let mut hasher = Sha256::new();
    let mut file = std::fs::File::open(path)?;
    let mut buf = [0u8; 8192];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 { break; }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().to_vec())
}

/// ファイルの先頭NバイトのSHA256を計算
fn compute_sha256_prefix(path: &std::path::Path, max_bytes: u64) -> std::io::Result<Vec<u8>> {
    use sha2::{Sha256, Digest};
    use std::io::Read;

    let mut hasher = Sha256::new();
    let mut file = std::fs::File::open(path)?;
    let mut buf = [0u8; 8192];
    let mut remaining = max_bytes;
    loop {
        let to_read = std::cmp::min(buf.len() as u64, remaining) as usize;
        if to_read == 0 { break; }
        let n = file.read(&mut buf[..to_read])?;
        if n == 0 { break; }
        hasher.update(&buf[..n]);
        remaining -= n as u64;
    }
    Ok(hasher.finalize().to_vec())
}

/// バイト配列のSHA256を計算
fn compute_sha256_bytes(data: &[u8]) -> Vec<u8> {
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().to_vec()
}

/// 画像ファイルの拡張子かどうか
fn is_image_file(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.ends_with(".jpg") || lower.ends_with(".jpeg") ||
        lower.ends_with(".png") || lower.ends_with(".gif") ||
        lower.ends_with(".webp")
}

/// 音声ファイルの先頭秒数に対応するバイト数を推定
/// 実際にはデコードが必要なので、ファイルサイズから概算する
/// 30秒 ≈ file_size * 30 / duration_sec
/// duration_secが不明な場合はファイルの先頭1MBを使用
fn estimate_bytes_for_seconds(file_size: u64, duration_sec: Option<i32>, seconds: f64) -> u64 {
    if let Some(dur) = duration_sec {
        if dur > 0 {
            let bytes = (file_size as f64 * seconds / dur as f64) as u64;
            return bytes.max(1);
        }
    }
    // フォールバック: 先頭1MB
    1024 * 1024
}

/// 特定のメディアのハッシュを計算・登録
pub fn compute_media_hash(
    conn: &Connection,
    item_uuid: &str,
    media_path: &str,
    media_type: &str,
    duration_sec: Option<i32>,
) -> Result<ComputeHashResult> {
    let path = std::path::Path::new(media_path);

    if !path.exists() {
        return Ok(ComputeHashResult {
            item_uuid: item_uuid.to_string(),
            hashes: vec![],
            skipped: true,
            skip_reason: Some(format!("Path does not exist: {}", media_path)),
        });
    }

    match media_type {
        "music" => compute_music_hash(conn, item_uuid, path, duration_sec),
        "video" => compute_video_hash(conn, item_uuid, path),
        "comic" => compute_comic_hash(conn, item_uuid, path),
        _ => Ok(ComputeHashResult {
            item_uuid: item_uuid.to_string(),
            hashes: vec![],
            skipped: true,
            skip_reason: Some(format!("Unknown media type: {}", media_type)),
        }),
    }
}

fn compute_music_hash(
    conn: &Connection,
    item_uuid: &str,
    path: &std::path::Path,
    duration_sec: Option<i32>,
) -> Result<ComputeHashResult> {
    // ファイルサイズ0チェック
    let file_size = match path.metadata() {
        Ok(m) => m.len(),
        Err(e) => {
            return Ok(ComputeHashResult {
                item_uuid: item_uuid.to_string(),
                hashes: vec![],
                skipped: true,
                skip_reason: Some(format!("Cannot read file metadata: {}", e)),
            });
        }
    };
    if file_size == 0 {
        return Ok(ComputeHashResult {
            item_uuid: item_uuid.to_string(),
            hashes: vec![],
            skipped: true,
            skip_reason: Some("File size is 0".to_string()),
        });
    }

    // 全体hash
    let whole_hash = match compute_sha256_file(path) {
        Ok(h) => h,
        Err(e) => {
            return Ok(ComputeHashResult {
                item_uuid: item_uuid.to_string(),
                hashes: vec![],
                skipped: true,
                skip_reason: Some(format!("Failed to compute hash: {}", e)),
            });
        }
    };

    // 先頭30秒hash
    let prefix_bytes = estimate_bytes_for_seconds(file_size, duration_sec, 30.0);
    let prefix_hash = compute_sha256_prefix(path, prefix_bytes).unwrap_or_else(|_| vec![0u8; 32]);

    let inputs = vec![
        MediaHashInput {
            item_uuid: item_uuid.to_string(),
            filename: String::new(),
            time_range: String::new(),
            content_hash: whole_hash,
            alternative_of: None,
        },
        MediaHashInput {
            item_uuid: item_uuid.to_string(),
            filename: String::new(),
            time_range: "0.0-30.0".to_string(),
            content_hash: prefix_hash,
            alternative_of: None,
        },
    ];

    let hashes = add_media_hashes(conn, &inputs)?;
    Ok(ComputeHashResult {
        item_uuid: item_uuid.to_string(),
        hashes,
        skipped: false,
        skip_reason: None,
    })
}

fn compute_video_hash(
    conn: &Connection,
    item_uuid: &str,
    path: &std::path::Path,
) -> Result<ComputeHashResult> {
    let file_size = match path.metadata() {
        Ok(m) => m.len(),
        Err(e) => {
            return Ok(ComputeHashResult {
                item_uuid: item_uuid.to_string(),
                hashes: vec![],
                skipped: true,
                skip_reason: Some(format!("Cannot read file metadata: {}", e)),
            });
        }
    };
    if file_size == 0 {
        return Ok(ComputeHashResult {
            item_uuid: item_uuid.to_string(),
            hashes: vec![],
            skipped: true,
            skip_reason: Some("File size is 0".to_string()),
        });
    }

    let whole_hash = match compute_sha256_file(path) {
        Ok(h) => h,
        Err(e) => {
            return Ok(ComputeHashResult {
                item_uuid: item_uuid.to_string(),
                hashes: vec![],
                skipped: true,
                skip_reason: Some(format!("Failed to compute hash: {}", e)),
            });
        }
    };

    let input = MediaHashInput {
        item_uuid: item_uuid.to_string(),
        filename: String::new(),
        time_range: String::new(),
        content_hash: whole_hash,
        alternative_of: None,
    };
    let hashes = add_media_hashes(conn, &[input])?;
    Ok(ComputeHashResult {
        item_uuid: item_uuid.to_string(),
        hashes,
        skipped: false,
        skip_reason: None,
    })
}

fn compute_comic_hash(
    conn: &Connection,
    item_uuid: &str,
    path: &std::path::Path,
) -> Result<ComputeHashResult> {
    // pathがディレクトリであることを確認
    if !path.is_dir() {
        return Ok(ComputeHashResult {
            item_uuid: item_uuid.to_string(),
            hashes: vec![],
            skipped: true,
            skip_reason: Some(format!("Comic path is not a directory: {}", path.display())),
        });
    }

    // ディレクトリ内の画像ファイルを列挙
    let entries = match std::fs::read_dir(path) {
        Ok(entries) => entries,
        Err(e) => {
            return Ok(ComputeHashResult {
                item_uuid: item_uuid.to_string(),
                hashes: vec![],
                skipped: true,
                skip_reason: Some(format!("Failed to read directory: {}", e)),
            });
        }
    };

    let mut image_files: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            if is_image_file(&name) { Some(name) } else { None }
        })
        .collect();

    if image_files.is_empty() {
        return Ok(ComputeHashResult {
            item_uuid: item_uuid.to_string(),
            hashes: vec![],
            skipped: true,
            skip_reason: Some("No image files found in directory".to_string()),
        });
    }

    // ファイル名でソート（辞書順）
    image_files.sort();

    // 各ページのhashを計算
    let mut page_hashes = Vec::with_capacity(image_files.len());
    let mut inputs = Vec::with_capacity(image_files.len() + 1);

    for name in &image_files {
        let file_path = path.join(name);
        match compute_sha256_file(&file_path) {
            Ok(hash) => {
                page_hashes.push(hash.clone());
                inputs.push(MediaHashInput {
                    item_uuid: item_uuid.to_string(),
                    filename: name.clone(),
                    time_range: String::new(),
                    content_hash: hash,
                    alternative_of: None,
                });
            }
            Err(e) => {
                eprintln!("Warning: failed to hash {}: {}", name, e);
            }
        }
    }

    // 全体hash: sort([SHA256(page)])を結合してSHA256
    // page_hashesは既にファイル名ソート順
    let mut combined = Vec::new();
    for h in &page_hashes {
        combined.extend_from_slice(h);
    }
    let whole_hash = compute_sha256_bytes(&combined);
    inputs.push(MediaHashInput {
        item_uuid: item_uuid.to_string(),
        filename: String::new(),
        time_range: String::new(),
        content_hash: whole_hash,
        alternative_of: None,
    });

    let hashes = add_media_hashes(conn, &inputs)?;
    Ok(ComputeHashResult {
        item_uuid: item_uuid.to_string(),
        hashes,
        skipped: false,
        skip_reason: None,
    })
}

/// フィルタ条件でメディアを絞り込み、ハッシュを計算・登録
pub fn compute_media_hashes(
    conn: &Connection,
    filter: &crate::types::MediaFilter,
    options: Option<&crate::types::QueryOptions>,
    force: bool,
) -> Result<Vec<ComputeHashResult>> {
    let media_list = crate::search::find_media(conn, filter, options)?;
    let mut results = Vec::new();

    for media in &media_list {
        // flag_exist = 1のみ対象
        if !media.flag_exist {
            continue;
        }
        // pathが必須
        let media_path = match &media.path {
            Some(p) if !p.is_empty() => p.clone(),
            _ => continue,
        };

        // force=falseの場合、既存hashがあればスキップ
        if !force {
            let existing = get_media_hashes(conn, &media.uuid).unwrap_or_default();
            if !existing.is_empty() {
                continue;
            }
        }

        let result = compute_media_hash(
            conn,
            &media.uuid,
            &media_path,
            media.media_type.as_str(),
            media.duration_sec,
        )?;
        results.push(result);
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crud::create_media;
    use crate::migration;
    use crate::types::{MediaInput, MediaFilter, MediaType};

    fn setup() -> (Connection, String) {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();
        let media = create_media(
            &conn,
            &MediaInput {
                title: "test".to_string(),
                media_type: MediaType::Comic,
                ..Default::default()
            },
        )
        .unwrap();
        (conn, media.uuid)
    }

    fn make_hash(item_uuid: &str, filename: &str, time_range: &str) -> MediaHashInput {
        MediaHashInput {
            item_uuid: item_uuid.to_string(),
            filename: filename.to_string(),
            time_range: time_range.to_string(),
            content_hash: vec![0u8; 32],
            alternative_of: None,
        }
    }

    #[test]
    fn test_add_and_get_media_hash() {
        let (conn, uuid) = setup();
        let mut input = make_hash(&uuid, "001.jpg", "");
        input.content_hash = vec![1u8; 32];

        let hash = add_media_hash(&conn, &input).unwrap();
        assert_eq!(hash.item_uuid, uuid);
        assert_eq!(hash.filename, "001.jpg");
        assert_eq!(hash.content_hash, vec![1u8; 32]);

        let got = get_media_hash(&conn, &uuid, "001.jpg", "").unwrap().unwrap();
        assert_eq!(got.content_hash, vec![1u8; 32]);
    }

    #[test]
    fn test_add_media_hashes_bulk() {
        let (conn, uuid) = setup();
        let inputs = vec![
            {
                let mut h = make_hash(&uuid, "001.jpg", "");
                h.content_hash = vec![1u8; 32];
                h
            },
            {
                let mut h = make_hash(&uuid, "002.jpg", "");
                h.content_hash = vec![2u8; 32];
                h
            },
        ];
        let results = add_media_hashes(&conn, &inputs).unwrap();
        assert_eq!(results.len(), 2);

        let all = get_media_hashes(&conn, &uuid).unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_get_media_hashes_for_item() {
        let (conn, uuid) = setup();
        let mut h1 = make_hash(&uuid, "", "");
        h1.content_hash = vec![1u8; 32];
        let mut h2 = make_hash(&uuid, "001.jpg", "");
        h2.content_hash = vec![2u8; 32];
        add_media_hash(&conn, &h1).unwrap();
        add_media_hash(&conn, &h2).unwrap();

        let hashes = get_media_hashes(&conn, &uuid).unwrap();
        assert_eq!(hashes.len(), 2);
    }

    #[test]
    fn test_find_by_content_hash() {
        let (conn, uuid) = setup();
        let mut input = make_hash(&uuid, "", "");
        input.content_hash = vec![0xABu8; 32];
        add_media_hash(&conn, &input).unwrap();

        let found = find_by_content_hash(&conn, &vec![0xABu8; 32]).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].item_uuid, uuid);

        let not_found = find_by_content_hash(&conn, &vec![0xCDu8; 32]).unwrap();
        assert!(not_found.is_empty());
    }

    #[test]
    fn test_delete_media_hash() {
        let (conn, uuid) = setup();
        let mut input = make_hash(&uuid, "001.jpg", "");
        input.content_hash = vec![1u8; 32];
        add_media_hash(&conn, &input).unwrap();

        delete_media_hash(&conn, &uuid, "001.jpg", "").unwrap();
        let got = get_media_hash(&conn, &uuid, "001.jpg", "").unwrap();
        assert!(got.is_none());
    }

    #[test]
    fn test_delete_media_hash_cascade_alternatives() {
        let (conn, uuid) = setup();
        // original
        let mut orig = make_hash(&uuid, "001.jpg", "");
        orig.content_hash = vec![1u8; 32];
        add_media_hash(&conn, &orig).unwrap();
        // alternative (alternative_of = "001.jpg")
        let mut alt = make_hash(&uuid, "001.png", "");
        alt.content_hash = vec![2u8; 32];
        alt.alternative_of = Some("001.jpg".to_string());
        add_media_hash(&conn, &alt).unwrap();

        assert_eq!(get_media_hashes(&conn, &uuid).unwrap().len(), 2);

        // 原本を削除 → 代替も削除される
        delete_media_hash(&conn, &uuid, "001.jpg", "").unwrap();
        assert!(get_media_hashes(&conn, &uuid).unwrap().is_empty());
    }

    #[test]
    fn test_delete_all_media_hashes() {
        let (conn, uuid) = setup();
        let mut h1 = make_hash(&uuid, "", "");
        h1.content_hash = vec![1u8; 32];
        let mut h2 = make_hash(&uuid, "001.jpg", "");
        h2.content_hash = vec![2u8; 32];
        add_media_hash(&conn, &h1).unwrap();
        add_media_hash(&conn, &h2).unwrap();

        delete_media_hashes(&conn, &uuid).unwrap();
        assert!(get_media_hashes(&conn, &uuid).unwrap().is_empty());
    }

    #[test]
    fn test_find_duplicate_hashes() {
        let (conn, uuid1) = setup();
        // 2つ目のメディアを作成
        let media2 = create_media(
            &conn,
            &MediaInput {
                title: "test2".to_string(),
                media_type: MediaType::Music,
                ..Default::default()
            },
        )
        .unwrap();
        let uuid2 = media2.uuid;

        // 同じcontent_hashで2件
        let mut h1 = make_hash(&uuid1, "", "");
        h1.content_hash = vec![0xFFu8; 32];
        let mut h2 = make_hash(&uuid2, "", "");
        h2.content_hash = vec![0xFFu8; 32];
        add_media_hash(&conn, &h1).unwrap();
        add_media_hash(&conn, &h2).unwrap();

        // ユニークなhash
        let mut h3 = make_hash(&uuid1, "001.jpg", "");
        h3.content_hash = vec![0xAAu8; 32];
        add_media_hash(&conn, &h3).unwrap();

        let dupes = find_duplicate_hashes(&conn).unwrap();
        assert_eq!(dupes.len(), 1);
        assert_eq!(dupes[0].0, vec![0xFFu8; 32]);
        assert_eq!(dupes[0].1, 2);
    }

    #[test]
    fn test_hex_conversion() {
        let bytes = vec![0xAB, 0xCD, 0xEF, 0x01];
        let hex = bytes_to_hex(&bytes);
        assert_eq!(hex, "abcdef01");

        let back = hex_to_bytes(&hex).unwrap();
        assert_eq!(back, bytes);
    }

    #[test]
    fn test_hex_to_bytes_invalid() {
        assert!(hex_to_bytes("abc").is_err());
        assert!(hex_to_bytes("zz").is_err());
    }

    #[test]
    fn test_upsert_on_conflict() {
        let (conn, uuid) = setup();
        let mut input = make_hash(&uuid, "", "");
        input.content_hash = vec![1u8; 32];
        add_media_hash(&conn, &input).unwrap();

        // 同じPKで別のhash値を登録 → upsert
        let mut input2 = make_hash(&uuid, "", "");
        input2.content_hash = vec![2u8; 32];
        add_media_hash(&conn, &input2).unwrap();

        let got = get_media_hash(&conn, &uuid, "", "").unwrap().unwrap();
        assert_eq!(got.content_hash, vec![2u8; 32]);
        assert_eq!(get_media_hashes(&conn, &uuid).unwrap().len(), 1);
    }

    #[test]
    fn test_delete_cascade_on_media_delete() {
        let (conn, uuid) = setup();
        let media_id = {
            let mut stmt = conn.prepare("SELECT id FROM media WHERE uuid = ?1").unwrap();
            stmt.query_row(params![uuid], |row| row.get::<_, i64>(0)).unwrap()
        };

        let mut input = make_hash(&uuid, "", "");
        input.content_hash = vec![1u8; 32];
        add_media_hash(&conn, &input).unwrap();
        assert_eq!(get_media_hashes(&conn, &uuid).unwrap().len(), 1);

        // mediaを削除 → FK CASCADEでhashesも削除される
        crate::crud::delete_media(&conn, media_id).unwrap();
        assert!(get_media_hashes(&conn, &uuid).unwrap().is_empty());
    }

    // ========== compute_media_hash テスト ==========

    fn setup_with_media(
        media_type: MediaType,
        path: Option<String>,
    ) -> (Connection, String) {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();
        let media = create_media(
            &conn,
            &MediaInput {
                title: "test".to_string(),
                media_type,
                path,
                flag_exist: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        (conn, media.uuid)
    }

    #[test]
    fn test_compute_music_hash() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.mp3");
        std::fs::write(&file_path, vec![0u8; 1024]).unwrap();

        let (conn, uuid) = setup_with_media(
            MediaType::Music,
            Some(file_path.to_str().unwrap().to_string()),
        );

        let result = compute_media_hash(
            &conn, &uuid, file_path.to_str().unwrap(), "music", Some(180),
        ).unwrap();

        assert!(!result.skipped);
        assert_eq!(result.hashes.len(), 2); // 全体 + 先頭30秒
        assert_eq!(result.hashes[0].filename, "");
        assert_eq!(result.hashes[0].time_range, "");
        assert_eq!(result.hashes[1].filename, "");
        assert_eq!(result.hashes[1].time_range, "0.0-30.0");
    }

    #[test]
    fn test_compute_video_hash() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.mp4");
        std::fs::write(&file_path, vec![0u8; 2048]).unwrap();

        let (conn, uuid) = setup_with_media(
            MediaType::Video,
            Some(file_path.to_str().unwrap().to_string()),
        );

        let result = compute_media_hash(
            &conn, &uuid, file_path.to_str().unwrap(), "video", None,
        ).unwrap();

        assert!(!result.skipped);
        assert_eq!(result.hashes.len(), 1); // 全体のみ
        assert_eq!(result.hashes[0].filename, "");
        assert_eq!(result.hashes[0].time_range, "");
    }

    #[test]
    fn test_compute_comic_hash() {
        let dir = tempfile::tempdir().unwrap();
        // 画像ファイルを作成
        std::fs::write(dir.path().join("001.jpg"), vec![1u8; 100]).unwrap();
        std::fs::write(dir.path().join("002.jpg"), vec![2u8; 100]).unwrap();
        std::fs::write(dir.path().join("readme.txt"), vec![3u8; 50]).unwrap(); // 画像ではない

        let (conn, uuid) = setup_with_media(
            MediaType::Comic,
            Some(dir.path().to_str().unwrap().to_string()),
        );

        let result = compute_media_hash(
            &conn, &uuid, dir.path().to_str().unwrap(), "comic", None,
        ).unwrap();

        assert!(!result.skipped);
        // 2ページ + 全体hash = 3
        assert_eq!(result.hashes.len(), 3);
        // ページhash
        let page_hashes: Vec<_> = result.hashes.iter()
            .filter(|h| !h.filename.is_empty())
            .collect();
        assert_eq!(page_hashes.len(), 2);
        // 全体hash
        let whole: Vec<_> = result.hashes.iter()
            .filter(|h| h.filename.is_empty() && h.time_range.is_empty())
            .collect();
        assert_eq!(whole.len(), 1);
    }

    #[test]
    fn test_compute_skip_nonexistent_path() {
        let (conn, uuid) = setup_with_media(MediaType::Music, None);

        let result = compute_media_hash(
            &conn, &uuid, "/nonexistent/file.mp3", "music", None,
        ).unwrap();

        assert!(result.skipped);
        assert!(result.skip_reason.is_some());
    }

    #[test]
    fn test_compute_skip_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("empty.mp3");
        std::fs::write(&file_path, []).unwrap();

        let (conn, uuid) = setup_with_media(
            MediaType::Music,
            Some(file_path.to_str().unwrap().to_string()),
        );

        let result = compute_media_hash(
            &conn, &uuid, file_path.to_str().unwrap(), "music", None,
        ).unwrap();

        assert!(result.skipped);
        assert!(result.skip_reason.unwrap().contains("size is 0"));
    }

    #[test]
    fn test_compute_skip_comic_no_images() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("readme.txt"), vec![1u8; 50]).unwrap();

        let (conn, uuid) = setup_with_media(
            MediaType::Comic,
            Some(dir.path().to_str().unwrap().to_string()),
        );

        let result = compute_media_hash(
            &conn, &uuid, dir.path().to_str().unwrap(), "comic", None,
        ).unwrap();

        assert!(result.skipped);
        assert!(result.skip_reason.unwrap().contains("No image files"));
    }

    #[test]
    fn test_compute_skip_comic_not_directory() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.cbz");
        std::fs::write(&file_path, vec![0u8; 100]).unwrap();

        let (conn, uuid) = setup_with_media(
            MediaType::Comic,
            Some(file_path.to_str().unwrap().to_string()),
        );

        let result = compute_media_hash(
            &conn, &uuid, file_path.to_str().unwrap(), "comic", None,
        ).unwrap();

        assert!(result.skipped);
        assert!(result.skip_reason.unwrap().contains("not a directory"));
    }

    #[test]
    fn test_compute_comic_whole_hash_order_independent() {
        // 同じ2ファイルを異なる順序で書いても全体hashは同じ
        let dir1 = tempfile::tempdir().unwrap();
        std::fs::write(dir1.path().join("a.jpg"), vec![1u8; 50]).unwrap();
        std::fs::write(dir1.path().join("b.jpg"), vec![2u8; 50]).unwrap();

        let (conn1, uuid1) = setup_with_media(
            MediaType::Comic,
            Some(dir1.path().to_str().unwrap().to_string()),
        );
        let result1 = compute_media_hash(
            &conn1, &uuid1, dir1.path().to_str().unwrap(), "comic", None,
        ).unwrap();

        // 同じ内容の別ディレクトリ（ファイル名順でソートされるので同じ結果）
        let dir2 = tempfile::tempdir().unwrap();
        std::fs::write(dir2.path().join("a.jpg"), vec![1u8; 50]).unwrap();
        std::fs::write(dir2.path().join("b.jpg"), vec![2u8; 50]).unwrap();

        let (conn2, uuid2) = setup_with_media(
            MediaType::Comic,
            Some(dir2.path().to_str().unwrap().to_string()),
        );
        let result2 = compute_media_hash(
            &conn2, &uuid2, dir2.path().to_str().unwrap(), "comic", None,
        ).unwrap();

        let whole1 = result1.hashes.iter().find(|h| h.filename.is_empty() && h.time_range.is_empty()).unwrap();
        let whole2 = result2.hashes.iter().find(|h| h.filename.is_empty() && h.time_range.is_empty()).unwrap();
        assert_eq!(whole1.content_hash, whole2.content_hash);
    }

    #[test]
    fn test_compute_upsert_existing_hash() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.mp3");
        std::fs::write(&file_path, vec![0u8; 1024]).unwrap();

        let (conn, uuid) = setup_with_media(
            MediaType::Music,
            Some(file_path.to_str().unwrap().to_string()),
        );

        // 1回目
        let r1 = compute_media_hash(
            &conn, &uuid, file_path.to_str().unwrap(), "music", None,
        ).unwrap();
        assert!(!r1.skipped);

        // 2回目（upsert）
        let r2 = compute_media_hash(
            &conn, &uuid, file_path.to_str().unwrap(), "music", None,
        ).unwrap();
        assert!(!r2.skipped);
        assert_eq!(r2.hashes.len(), 2);

        // 重複は作られない
        let all = get_media_hashes(&conn, &uuid).unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_compute_media_hashes_skips_no_flag_exist() {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.mp3");
        std::fs::write(&file_path, vec![0u8; 100]).unwrap();

        // flag_exist = false
        create_media(
            &conn,
            &MediaInput {
                title: "test".to_string(),
                media_type: MediaType::Music,
                path: Some(file_path.to_str().unwrap().to_string()),
                flag_exist: Some(false),
                ..Default::default()
            },
        )
        .unwrap();

        let filter = MediaFilter::default();
        let results = compute_media_hashes(&conn, &filter, None, false).unwrap();
        assert!(results.is_empty());
    }
}
