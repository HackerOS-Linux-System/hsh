use std::collections::HashMap;
use std::env;
use hk_parser::{load_hk_file, resolve_interpolations, HkConfig};
use indexmap::IndexMap;

pub fn load_shell_config() -> HkConfig {
    let home = env::var("HOME").unwrap_or_default();
    let config_path = format!("{}/.hshrc", home);
    let mut config = load_hk_file(&config_path).unwrap_or_else(|_| IndexMap::new());
    resolve_interpolations(&mut config).ok();
    config
}

pub fn get_aliases(config: &HkConfig) -> HashMap<String, String> {
    config
    .get("aliases")
    .and_then(|v| v.as_map().ok())
    .map(|m| {
        m.iter()
        .filter_map(|(k, v)| v.as_string().ok().map(|val| (k.clone(), val)))
        .collect()
    })
    .unwrap_or_default()
}

pub fn get_prompt_config(config: &HkConfig) -> HashMap<String, String> {
    config
    .get("prompt")
    .and_then(|v| v.as_map().ok())
    .map(|m| {
        m.iter()
        .filter_map(|(k, v)| v.as_string().ok().map(|val| (k.clone(), val)))
        .collect()
    })
    .unwrap_or_default()
}

/// Get prompt segment order from config, defaulting to standard order
pub fn get_segment_order(config: &HashMap<String, String>) -> Vec<String> {
    if let Some(order) = config.get("segment_order") {
        order.split(',').map(|s| s.trim().to_string()).collect()
    } else {
        vec![
            "time".to_string(),
            "dir".to_string(),
            "git".to_string(),
            "mem_cpu".to_string(),
        ]
    }
}
