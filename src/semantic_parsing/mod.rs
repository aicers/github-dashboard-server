#![allow(clippy::uninlined_format_args)]

use anyhow::Result;
use langchain_rust::{language_models::llm::LLM, llm::client::Ollama};

pub(crate) async fn invoke(schema: &str, prompt: &str) -> Result<String> {
    let ollama = Ollama::default().with_model("llama3.2");

    let prompt = format!(
        "You are a helpful assistant that translates natural language into GraphQL queries.\n\n\
        If you can't find the answer in the schema, just say that you don't know, don't try to make up an answer.\n\n\
        Don't explain the query, just return the GraphQL query.\n\n\
        The answer will be like query {{ ... }}\n\n\
        Don't forget the closing bracket at the end.\n\n\
        Schema:\n{}\n\n\
        Natural Language Query:\n{}\n\n\
        GraphQL Query:",
        schema, prompt
    );

    Ok(ollama.invoke(&prompt).await?)
}
