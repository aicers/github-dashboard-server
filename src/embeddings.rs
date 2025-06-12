use std::io::Write;
use std::process::Command;

use anyhow::{Context, Result};
use serde::Deserialize;
use tempfile::NamedTempFile;

#[derive(Deserialize)]
struct EmbedItem {
    _input: String,
    embedding: Vec<f32>,
}

/// Embed a batch of texts by shelling out to the `tensor-tools` CLI.
pub fn get_embeddings(texts: &[String]) -> Result<Vec<Vec<f32>>> {
    // 1) Write inputs line-by-line
    let mut tf = NamedTempFile::new().context("create temp file")?;
    for t in texts {
        writeln!(tf, "{t}").context("write to temp file")?;
    }
    let path = tf.path().to_str().unwrap();

    // 2) Run the CLI:
    //    tensor-tools text embed --model-path ./models/all-MiniLM-L6-v2 --inputs <path> --output-format json
    let output = Command::new("tensor-tools")
        .args([
            "text",
            "embed",
            "--model-path",
            "./models/all-MiniLM-L6-v2",
            "--inputs",
            path,
            "--output-format",
            "json",
        ])
        .output()
        .context("failed to run tensor-tools")?;
    if !output.status.success() {
        anyhow::bail!(
            "tensor-tools failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // 3) Parse the JSONL output
    let stdout = String::from_utf8(output.stdout).context("invalid UTF-8 from tensor-tools")?;
    let mut embs = Vec::with_capacity(texts.len());
    for line in stdout.lines() {
        let item: EmbedItem = serde_json::from_str(line).context(format!("parse JSON: {line}"))?;
        embs.push(item.embedding);
    }
    Ok(embs)
}
