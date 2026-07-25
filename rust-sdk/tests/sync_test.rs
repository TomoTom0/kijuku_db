//! sync（prod→stg フル複製）のテスト（設計 §4.2・TASK-50 C4）。
//!
//! `KijukuDB::replicate_db` が prod(RO) → stg(RW) の完全な複製を行うこと、
//! 既存 stg の上書きが冪等であること、src==dst を誤設定として弾くことを検証する。

use kijuku_db::{KijukuDB, MediaInput, MediaType};
use tempfile::TempDir;

/// 末尾に2件のメディアを持つ prod DB を作成し、パスを返す。
fn setup_prod(dir: &TempDir) -> (std::path::PathBuf, std::path::PathBuf) {
    let prod = dir.path().join("kijuku.db");
    let stg = dir.path().join("kijuku.stg.db");
    let prod_db = KijukuDB::open(&prod).expect("open prod");
    prod_db.migrate().expect("migrate prod");
    prod_db
        .create_media(&MediaInput {
            title: "prod メディア1".to_string(),
            media_type: MediaType::Comic,
            ..Default::default()
        })
        .expect("create media 1");
    prod_db
        .create_media(&MediaInput {
            title: "prod メディア2".to_string(),
            media_type: MediaType::Comic,
            ..Default::default()
        })
        .expect("create media 2");
    (prod, stg)
}

/// prod(RO)→stg(RW) のフル複製がデータ・スキーマ・バージョンを正しくコピーする（設計 §4.2）。
#[test]
fn test_replicate_db_copies_data_and_schema() {
    let dir = TempDir::new().unwrap();
    let (prod, stg) = setup_prod(&dir);

    let (prod_version, prod_tables) = {
        let prod_db = KijukuDB::open(&prod).unwrap();
        let v = prod_db.get_schema_version().unwrap();
        let t = prod_db.get_tables().unwrap();
        (v, t)
    };
    assert!(!stg.exists(), "sync 前は stg が存在しない");

    // sync（prod→stg）
    KijukuDB::replicate_db(&prod, &stg).unwrap();

    // stg を開いて prod と一致することを検証
    let stg_db = KijukuDB::open(&stg).unwrap();
    assert_eq!(stg_db.get_schema_version().unwrap(), prod_version);
    assert_eq!(stg_db.get_tables().unwrap(), prod_tables);
    assert_eq!(stg_db.get_media(1).unwrap().title, "prod メディア1");
    assert_eq!(stg_db.get_media(2).unwrap().title, "prod メディア2");
}

/// 既存 stg（stg 側の追加分を含む）がある状態での再 sync が、prod で完全に上書きされる（設計 §4.2）。
#[test]
fn test_replicate_db_overwrites_existing_stg() {
    let dir = TempDir::new().unwrap();
    let (prod, stg) = setup_prod(&dir);

    // 1回目の sync
    KijukuDB::replicate_db(&prod, &stg).unwrap();
    assert!(stg.exists());

    // stg 側で独自にレコードを追加（LLM 編集のシミュレート）
    {
        let stg_db = KijukuDB::open(&stg).unwrap();
        stg_db
            .create_media(&MediaInput {
                title: "stg 側の追加分（再 sync で消えるべき）".to_string(),
                media_type: MediaType::Comic,
                ..Default::default()
            })
            .expect("stg append");
    }

    // 再 sync（既存 stg + WAL/SHM 副産物が残り得る状態からの上書き）
    KijukuDB::replicate_db(&prod, &stg).unwrap();

    // 再 sync 後: stg は prod と完全一致（stg 固有の追加分は削除され、prod データのみ残る）
    let stg_db = KijukuDB::open(&stg).unwrap();
    assert_eq!(stg_db.get_media(1).unwrap().title, "prod メディア1");
    assert_eq!(stg_db.get_media(2).unwrap().title, "prod メディア2");
    assert!(
        stg_db.get_media(3).is_none(),
        "再 sync で stg 固有の追加分は prod で上書きされて削除される"
    );
}

/// src == dst は誤設定としてエラー（自己コピー防止・設計 §4.2）。
#[test]
fn test_replicate_db_rejects_same_path() {
    let dir = TempDir::new().unwrap();
    let same = dir.path().join("kijuku.db");
    let result = KijukuDB::replicate_db(&same, &same);
    assert!(result.is_err(), "src == dst はエラー");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("same path"),
        "expected same-path error, got: {}",
        err
    );
}

/// discard（stg 破棄・再 sync・設計 §4.6・TASK-45）: gate 不合格等で stg を捨てて prod から
/// 再構築する経路。処理は sync と同一（`replicate_db(prod, stg)`）のため、ここでは discard の
/// 意味論（stg の編集破棄 + prod 無傷）を検証する。CLI/remote の `discard` operation も同一処理。
#[test]
fn test_discard_restores_stg_from_prod_and_leaves_prod_intact() {
    let dir = TempDir::new().unwrap();
    let (prod, stg) = setup_prod(&dir);

    // 1. sync（書込セッション開始・設計 §6.1）
    KijukuDB::replicate_db(&prod, &stg).unwrap();

    // 2. stg に LLM 編集（promote せず破棄するシナリオのシミュレート）
    {
        let stg_db = KijukuDB::open(&stg).unwrap();
        stg_db
            .create_media(&MediaInput {
                title: "stg 側の破棄される編集".to_string(),
                media_type: MediaType::Comic,
                ..Default::default()
            })
            .expect("stg edit");
    }

    // prod の状態をキャプチャ（discard 前後で「prod は一切触られない」§4.6 を検証するため）
    let prod_titles_before: Vec<String> = {
        let prod_db = KijukuDB::open(&prod).unwrap();
        (1..=3)
            .filter_map(|i| prod_db.get_media(i).map(|m| m.title))
            .collect()
    };

    // 3. discard（stg を破棄して prod から再 sync・§4.6）。処理は `replicate_db(prod, stg)` と同一。
    KijukuDB::replicate_db(&prod, &stg).unwrap();

    // 4. stg は prod で上書き復元（stg 固有の編集は破棄され prod データのみ残る）
    let stg_db = KijukuDB::open(&stg).unwrap();
    assert_eq!(stg_db.get_media(1).unwrap().title, "prod メディア1");
    assert_eq!(stg_db.get_media(2).unwrap().title, "prod メディア2");
    assert!(
        stg_db.get_media(3).is_none(),
        "discard で stg の編集は破棄される"
    );

    // 5. prod は discard 前後で無傷（§4.6・prod は一切触られない）
    let prod_titles_after: Vec<String> = {
        let prod_db = KijukuDB::open(&prod).unwrap();
        (1..=3)
            .filter_map(|i| prod_db.get_media(i).map(|m| m.title))
            .collect()
    };
    assert_eq!(
        prod_titles_before, prod_titles_after,
        "discard で prod は無傷"
    );
    assert_eq!(
        prod_titles_after,
        vec!["prod メディア1".to_string(), "prod メディア2".to_string()]
    );
}
