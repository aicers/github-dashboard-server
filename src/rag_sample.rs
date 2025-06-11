use std::error::Error;
use std::hash::{DefaultHasher, Hash, Hasher};

use langchain_rust::chain::{
    Chain, ConversationalRetrieverChain, ConversationalRetrieverChainBuilder,
};
use langchain_rust::memory::SimpleMemory;
use langchain_rust::prompt_args;
use langchain_rust::schemas::Document;
use langchain_rust::vectorstore::qdrant::{Qdrant, Store, StoreBuilder};
use langchain_rust::vectorstore::{Retriever, VecStoreOptions, VectorStore};
use langchain_rust::{embedding::OllamaEmbedder, llm::client::Ollama};
use qdrant_client::qdrant::{PointStruct, UpsertPointsBuilder};
use qdrant_client::Payload;
use serde_json::{json, to_string};
use tracing::{error, info};
use uuid::Uuid;

#[derive(Default)]
pub struct RagOllamaSystem {
    pub embedder: String,
    pub llm: String,
    pub vector_store: Option<Store>,
    pub chain: Option<ConversationalRetrieverChain>,
}

impl RagOllamaSystem {
    pub fn new(embedder: String, llm: String) -> Self {
        Self {
            embedder,
            llm,
            vector_store: None,
            chain: None,
        }
    }

    pub async fn set_db(&mut self) {
        let client = Qdrant::from_url("http://localhost:6334").build().unwrap();

        let store = StoreBuilder::new()
            .embedder(OllamaEmbedder::default().with_model(&self.embedder))
            .client(client)
            .collection_name("rag")
            // .recreate_collection(true)
            .build()
            .await;

        match store {
            Ok(store) => {
                if self.vector_store.is_none() {
                    self.vector_store = Some(store);
                    info!("Success setup DB");
                } else {
                    info!("Already DB");
                }
            }
            Err(e) => {
                error!(e);
                info!("Fail setup DB");
            }
        }
    }

    pub async fn add_documents(&self, docs: &Vec<Document>, opt: &VecStoreOptions) {
        if docs.is_empty() {
            return;
        }
        if let Some(store) = &self.vector_store {
            let data = store.add_documents(docs, opt).await;
            match data {
                Ok(data) => info!("{:?}", data),
                Err(e) => error!(e),
            }
        } else {
            info!("add_document: DB is not initialized");
        }
    }

    fn generate_deterministic_uuid(input: &str) -> String {
        let mut hasher = DefaultHasher::new();
        input.hash(&mut hasher);
        let hash = hasher.finish();

        Uuid::from_u128(u128::from(hash)).to_string()
    }

    async fn add_documents_to_store_with_ids(
        store: &Store,
        docs: &[Document],
        ids: &[String],
        opt: &VecStoreOptions,
    ) -> Result<Vec<String>, Box<dyn Error>> {
        if docs.len() != ids.len() {
            error!("Doc length is not equal id length");
        }
        let embedder = opt.embedder.as_ref().unwrap_or(&store.embedder);
        let texts: Vec<String> = docs.iter().map(|d| d.page_content.clone()).collect();

        let ids = ids.iter().map(|id| Self::generate_deterministic_uuid(id));
        let vectors = embedder.embed_documents(&texts).await?.into_iter();
        let payloads = docs.iter().map(|d| {
            json!({
                &store.content_field: d.page_content,
                &store.metadata_field: d.metadata,
            })
        });

        let mut points: Vec<PointStruct> = Vec::with_capacity(docs.len());

        for (id, (vector, payload)) in ids.clone().zip(vectors.zip(payloads)) {
            let vector: Vec<f32> = vector.into_iter().map(|f| f as f32).collect();
            let point = PointStruct::new(id, vector, Payload::try_from(payload).unwrap());
            points.push(point);
        }

        store
            .client
            .upsert_points(UpsertPointsBuilder::new(&store.collection_name, points).wait(true))
            .await?;

        Ok(ids.collect())
    }

    pub async fn add_documents_with_ids(
        &self,
        docs: &[Document],
        ids: &[String],
        opt: &VecStoreOptions,
    ) {
        if docs.is_empty() || ids.is_empty() {
            info!("Emtpy Document");
            return;
        }
        info!("{:?}", ids);
        if let Some(store) = &self.vector_store {
            let data = Self::add_documents_to_store_with_ids(store, docs, ids, opt).await;
            match data {
                Ok(data) => info!("{:?}", data),
                Err(e) => error!(e),
            }
        } else {
            error!("DB is not initialized");
        }
    }

    pub fn set_chain(&mut self) {
        if let Some(store) = self.vector_store.take() {
            let chain = ConversationalRetrieverChainBuilder::new()
                .llm(Ollama::default().with_model(&self.llm))
                .rephrase_question(true)
                .memory(SimpleMemory::new().into())
                .retriever(Retriever::new(store, 10))
                .build()
                .expect("Error building ConversationalChain");

            self.chain = Some(chain);
            info!("Success set chain");
        } else {
            error!("set chain: DB is not initialized");
        }
    }

    pub async fn query(&self, question: &str) -> Result<String, Box<dyn Error>> {
        let input = prompt_args! {
            "question" => question,
        };
        if let Some(chain) = &self.chain {
            let result = chain.call(input).await?;
            let answer = to_string(&result.generation)?;
            return Ok(answer);
        }
        Err(anyhow::anyhow!("Chain is not initialized").into())
    }
}

#[cfg(test)]
mod tests {
    use langchain_rust::{schemas::Document, vectorstore::VecStoreOptions};

    use super::RagOllamaSystem;
    #[tokio::test]
    async fn query_test() {
        let mut rag = RagOllamaSystem::new("nomic-embed-text".to_string(), "llama3.2".to_string());

        rag.set_db().await;
        let docs = vec![Document::new("안녕하세요")];
        let opt = VecStoreOptions::default();
        rag.add_documents(&docs, &opt).await;

        rag.set_chain();
        let query = "안녕?";
        let answer = rag.query(query).await;
        if let Ok(answer) = answer {
            println!("{}", &answer);
        }
    }
}
