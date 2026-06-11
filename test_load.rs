use std::sync::Arc;
fn main() {
    let config_path = std::path::PathBuf::from("/home/tmih06/.stemcell/config.toml");
    let content = std::fs::read_to_string(config_path).unwrap();
    println!("File exists: {}", content.contains("opencode_zen_free"));
}
