use serde_json::Value;

pub fn pretty_log(message: &str, data: &str) {
    let parsed: Value = serde_json::from_str(data).unwrap();
    tracing::info!(
        "{message} {}",
        serde_json::to_string_pretty(&parsed).unwrap()
    );
}
