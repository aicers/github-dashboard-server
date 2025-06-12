// generate.rs
use reqwest::Client;

pub async fn generate_answer(
    question: &str,
    contexts: &[String],
    ollama_url: &str,
    model: &str,
) -> anyhow::Result<String> {
    let mut prompt = String::new();
    for c in contexts {
        prompt.push_str(c);
        prompt.push('\n');
    }
    prompt.push_str(&format!("Question: {}\nAnswer:", question));
    let payload = serde_json::json!({
        "model": model,
        "prompt": prompt,
    });
    let ans = Client::new()
        .post(format!("{}/chat", ollama_url))
        .json(&payload)
        .send()
        .await?
        .text()
        .await?;
    Ok(ans)
}
