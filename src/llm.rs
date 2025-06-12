use anyhow::Result;
use ollama_rs::{coordinator::Coordinator, generation::chat::ChatMessage, Ollama};

use crate::database::Database;
// use crate::vector_db::get_related_chunks_with_filter;

// /// Qdrant에서 owner/repo 영역 중 author가 일치하는 Chunk top_k개 가져옵니다.
// #[ollama_rs::function]
// pub(crate) async fn search_chunks_by_author(
//     owner: String,
//     repo: String,
//     author: String,
//     top_k: usize,
// ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
//     let collection = format!("{owner}/{repo}");
//     let hits = get_related_chunks_with_filter(&collection, &author, top_k).await?;
//     Ok(serde_json::to_string(&hits)?)
// }

/// Count how many issues a given GitHub user opened in this repository.
#[ollama_rs::function]
pub(crate) async fn count_issues_by_author_fn(
    owner: String,
    repo: String,
    author: String,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let db_path = std::env::var("DB_PATH").unwrap_or_else(|_| "github-dashboard".into());
    let db = Database::connect(std::path::Path::new(&db_path))
        .map_err(|e| format!("DB open error: {e}"))?;
    let count = db.count_issues_by_author(&owner, &repo, &author);
    Ok(count.to_string())
}

/// A RAG model plus a lightweight translation model for Korean->English.
pub(crate) struct Localmodel {
    /// Coordinator for RAG with function-calling
    rag_coordinator: Coordinator<Vec<ChatMessage>>,
}

impl Localmodel {
    /// Create Localmodel with a heavy RAG model and a lighter translation model.
    pub(crate) fn new(rag_model: &str, owner: &str, repo: &str) -> Self {
        let client = Ollama::default();

        // RAG coordinator with function-calling and repo context
        let mut rag_history = Vec::new();
        let sys = format!(
            "You are a RAG assistant for repository '{owner}/{repo}'. \
            Before answering, show your reasoning. \
            Then call count_issues_by_author_fn if needed, and finally state your answer. \
            All calls to count_issues_by_author_fn must use owner='{owner}' and repo='{repo}'.",
        );

        rag_history.push(ChatMessage::system(sys));
        let rag_coordinator = Coordinator::<Vec<ChatMessage>>::new(
            client.clone(),
            rag_model.to_string(),
            rag_history,
        )
        .add_tool(count_issues_by_author_fn);
        Self { rag_coordinator }
    }

    /// Build the RAG prompt, instructing language.
    pub(crate) fn combine_question_and_chunks(question: &str, chunks: &[String]) -> String {
        let lang_instr = if question.chars().any(|c| ('가'..='힣').contains(&c)) {
            "질문이 한국어이므로, 답변도 한국어로 해 주세요."
        } else {
            "Please answer in English."
        };
        format!(
            "{lang_instr}\n\nAnswer the question based on the following references and calls:\n\nQuestion:\n{question}\n\nReferences:\n{refs}",
            lang_instr = lang_instr,
            question = question,
            refs = chunks.join("\n---\n"),
        )
    }

    /// Generate the final RAG response; the model can call our function with defaults.
    pub(crate) async fn generate_response(&mut self, combined_input: &str) -> Result<String> {
        let user_msg = ChatMessage::user(combined_input.to_string());
        let resp = self.rag_coordinator.chat(vec![user_msg]).await?;
        Ok(resp.message.content.clone())
    }
}
