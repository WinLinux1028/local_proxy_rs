#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

mod config;

use crate::config::AppConfig;

use std::env;
use tokio::{fs::File, io::AsyncReadExt};

static ERROR_HTML: &[u8] = include_bytes!("../static/error.html");

#[tokio::main]
async fn main() {
    // 設定ファイルのパスを取得
    let mut args = env::args();
    args.next(); // 1つ目は実行ファイルのパスなので無視

    let mut config_path = String::from("config.json5");
    if let Some(last_arg) = args.last() {
        config_path = last_arg;
    }

    // 設定ファイルを読み込んでパースする
    let mut config_file = File::open(&config_path)
        .await
        .expect("Failed to open config file");

    let mut config_str = String::new();
    config_file
        .read_to_string(&mut config_str)
        .await
        .expect("Failed to read config file");

    let config: AppConfig = json5::from_str(&config_str).expect("Failed to parse config file");

    
}
