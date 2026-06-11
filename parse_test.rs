use serde::Deserialize;
use std::fs;

#[derive(Debug, Deserialize)]
struct Config {
    providers: Providers,
}

#[derive(Debug, Deserialize)]
struct Providers {
    opencode_zen_free: Option<ProviderConfig>,
}

#[derive(Debug, Deserialize)]
struct ProviderConfig {
    enabled: bool,
    default_model: Option<String>,
}

fn main() {
    let content = fs::read_to_string("/home/tmih06/.stemcell/config.toml").unwrap();
    let config: Config = toml::from_str(&content).unwrap();
    println!("Parsed: {:?}", config.providers.opencode_zen_free);
}
