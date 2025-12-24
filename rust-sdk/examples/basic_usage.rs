use kijuku_db::{KijukuDB, MediaInput, MediaType};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // インメモリデータベースを作成（テスト用）
    let _db = KijukuDB::open_in_memory()?;

    println!("きじゅくDB - Rust SDK サンプル");
    println!("==============================\n");

    // メディア入力データの作成
    let input = MediaInput {
        title: "サンプルコミック".to_string(),
        media_type: MediaType::Comic,
        artist: Some("サンプル作者".to_string()),
        description: Some("これはサンプルのコミックです".to_string()),
        page_count: Some(200),
        ..Default::default()
    };

    println!("メディアデータを作成:");
    println!("  タイトル: {}", input.title);
    println!("  メディアタイプ: {:?}", input.media_type);
    if let Some(ref artist) = input.artist {
        println!("  作者: {}", artist);
    }

    println!("\nデータベースの準備が完了しました。");
    println!("今後の実装で、実際のCRUD操作が可能になります。");

    Ok(())
}
