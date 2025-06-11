use std::collections::HashMap;

use langchain_rust::{
    embedding::{Embedder, OllamaEmbedder},
    language_models::llm::LLM,
    llm::client::Ollama,
    schemas::Document,
    vectorstore::{VecStoreOptions, VectorStore},
};
use qdrant_client::qdrant::{
    value::{self, Kind},
    PointId, RetrievedPoint, Struct, Value,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::rag_sample::RagOllamaSystem;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCase {
    pub id: String,
    pub question: String,
    pub ground_truth_answer: String,
    pub relevant_doc_ids: Vec<String>,
    pub difficulty: Difficulty,
    pub category: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Difficulty {
    Easy,
    Medium,
    Hard,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationResult {
    pub test_case_question: String,
    pub generated_answer: String,
    pub retrieved_docs: Vec<String>,
    pub metrics: EvaluationMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EvaluationMetrics {
    pub recall_at_5: f64,
    pub mrr: f64,
    pub bleu_score: f64,
    pub bert_score: f64,
    pub llm_judge_score: f64,
    pub exact_match: bool,
}

pub struct RagEvaluator {
    pub vector_store: Box<dyn VectorStore>,
    pub llm: Ollama,
    pub rag: RagOllamaSystem,
    pub test_cases: Vec<TestCase>,
}

impl RagEvaluator {
    pub fn new(vector_store: Box<dyn VectorStore>, rag: RagOllamaSystem, llm: Ollama) -> Self {
        Self {
            vector_store,
            rag,
            llm,
            test_cases: Vec::new(),
        }
    }

    // 테스트 케이스 생성
    pub async fn generate_test_cases(
        &mut self,
        documents: Vec<RetrievedPoint>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        for (i, doc) in documents.iter().enumerate() {
            let test_case = self.generate_qa_from_document(doc, i).await?;
            self.test_cases.push(test_case);
        }
        Ok(())
    }
    #[allow(clippy::needless_raw_string_hashes)]
    async fn generate_qa_from_document(
        &self,
        document: &RetrievedPoint,
        id: usize,
    ) -> Result<TestCase, Box<dyn std::error::Error>> {
        let prompt = format!(
            r#"
            Please read the following document and generate a question and answer based on its content.
            Make sure the question includes a clue that clearly identifies this specific document.

            Document:
            {}

            Please format your response as follows:

            Question: [Your question here]

            Answer: [The correct answer]

            Difficulty: [Easy / Medium / Hard]

            Category: [Factual / Inference / Summary]
            "#,
            json!(document.payload),
        );

        let response = self.llm.invoke(&prompt).await?;
        let mut number = 0;
        if let Some(metadata_val) = document.payload.get("metadata") {
            if let Some(Kind::StructValue(metadata_map)) = &metadata_val.kind {
                if let Some(val) = metadata_map.fields.get("number") {
                    if let Some(Kind::IntegerValue(n)) = &val.kind {
                        number = *n as i32;
                    }
                }
            }
        }
        // let number = document.get("metadata.number").to_string();
        let test_case = TestCase {
            id: format!("test_{id}"),
            question: self.extract_question(&response)?,
            ground_truth_answer: self.extract_answer(&response),
            relevant_doc_ids: vec![number.to_string()],
            difficulty: self.extract_difficulty(&response)?,
            category: self.extract_category(&response)?,
        };

        Ok(test_case)
    }

    pub async fn evaluate_all(&self) -> Result<Vec<EvaluationResult>, Box<dyn std::error::Error>> {
        let mut results = Vec::new();

        for test_case in &self.test_cases {
            let result = self.evaluate_single_case(test_case).await?;
            results.push(result);

            println!("평가 완료: {}/{}", results.len(), self.test_cases.len());
        }

        Ok(results)
    }

    async fn evaluate_single_case(
        &self,
        test_case: &TestCase,
    ) -> Result<EvaluationResult, Box<dyn std::error::Error>> {
        // 1. RAG 시스템으로 답변 생성
        let generated_answer = self.rag.query(&test_case.question).await?;
        let mut retrieved_docs: Vec<Document> = vec![];
        let mut metrics = EvaluationMetrics::default();

        // 2. 관련 문서 검색
        retrieved_docs = self
            .vector_store
            .similarity_search(&test_case.question, 10, &VecStoreOptions::default())
            .await?;

        // 3. 각종 메트릭 계산
        metrics = self
            .calculate_metrics(test_case, &generated_answer, &retrieved_docs)
            .await?;

        Ok(EvaluationResult {
            test_case_question: test_case.question.clone(),
            generated_answer,
            retrieved_docs: retrieved_docs
                .into_iter()
                .map(|d| d.metadata.get("number").unwrap().to_string())
                .collect(),
            metrics,
        })
    }

    // 메트릭 계산
    async fn calculate_metrics(
        &self,
        test_case: &TestCase,
        generated_answer: &str,
        retrieved_docs: &[langchain_rust::schemas::Document],
    ) -> Result<EvaluationMetrics, Box<dyn std::error::Error>> {
        // Recall@5 계산
        let recall_at_5 =
            self.calculate_recall_at_k(&test_case.relevant_doc_ids, retrieved_docs, 5);

        // MRR 계산
        let mrr = self.calculate_mrr(&test_case.relevant_doc_ids, retrieved_docs);

        // BLEU 점수 계산
        let bleu_score =
            self.calculate_bleu_score(&test_case.ground_truth_answer, generated_answer);

        // BERT 점수 계산 (여기서는 단순화)
        let bert_score = self
            .calculate_bert_score(&test_case.ground_truth_answer, generated_answer)
            .await?;

        // LLM Judge 점수
        let llm_judge_score = self
            .llm_judge_evaluation(
                &test_case.question,
                &test_case.ground_truth_answer,
                generated_answer,
            )
            .await?;

        // 정확히 일치하는지 확인
        let exact_match = test_case.ground_truth_answer.trim().to_lowercase()
            == generated_answer.trim().to_lowercase();

        Ok(EvaluationMetrics {
            recall_at_5,
            mrr,
            bleu_score,
            bert_score,
            llm_judge_score,
            exact_match,
        })
    }

    // Recall@K 계산
    fn calculate_recall_at_k(
        &self,
        relevant_doc_ids: &[String],
        retrieved_docs: &[langchain_rust::schemas::Document],
        k: usize,
    ) -> f64 {
        let retrieved_ids: Vec<String> = retrieved_docs
            .iter()
            .take(k)
            .map(|doc| doc.metadata.get("number").unwrap().to_string())
            .collect();

        let relevant_found = relevant_doc_ids
            .iter()
            .filter(|id| retrieved_ids.contains(id))
            .count();

        relevant_found as f64 / relevant_doc_ids.len() as f64
    }

    // MRR 계산
    fn calculate_mrr(
        &self,
        relevant_doc_ids: &[String],
        retrieved_docs: &[langchain_rust::schemas::Document],
    ) -> f64 {
        for (i, doc) in retrieved_docs.iter().enumerate() {
            let doc_id = doc.metadata.get("number").unwrap().to_string();
            if relevant_doc_ids.contains(&doc_id) {
                return 1.0 / (i + 1) as f64;
            }
        }
        0.0
    }

    // BLEU 점수 계산 (단순화된 버전)
    fn calculate_bleu_score(&self, reference: &str, candidate: &str) -> f64 {
        let ref_words: Vec<&str> = reference.split_whitespace().collect();
        let cand_words: Vec<&str> = candidate.split_whitespace().collect();

        let matches = cand_words
            .iter()
            .filter(|word| ref_words.contains(word))
            .count();

        if cand_words.is_empty() {
            0.0
        } else {
            matches as f64 / cand_words.len() as f64
        }
    }

    // BERT 점수 계산 (임베딩 유사도 사용)
    async fn calculate_bert_score(
        &self,
        reference: &str,
        candidate: &str,
    ) -> Result<f64, Box<dyn std::error::Error>> {
        let embeddings = OllamaEmbedder::default();

        let ref_embedding = embeddings.embed_query(reference).await?;
        let cand_embedding = embeddings.embed_query(candidate).await?;

        // 코사인 유사도 계산
        let dot_product: f64 = ref_embedding
            .iter()
            .zip(cand_embedding.iter())
            .map(|(a, b)| a * b)
            .sum();

        let norm1: f64 = ref_embedding.iter().map(|x| x * x).sum::<f64>().sqrt();
        let norm2: f64 = cand_embedding.iter().map(|x| x * x).sum::<f64>().sqrt();

        Ok(dot_product / (norm1 * norm2))
    }

    // LLM Judge 평가
    async fn llm_judge_evaluation(
        &self,
        question: &str,
        ground_truth: &str,
        generated_answer: &str,
    ) -> Result<f64, Box<dyn std::error::Error>> {
        let prompt = format!(
            r"
            Please compare the following question and generated answer, and evaluate the quality of the answer on a scale of 1 to 5.

            Question: {question}
            Ground Truth: {ground_truth}
            Generated Answer: {generated_answer}

            Evaluation Criteria:

            Accuracy: Is the answer factually correct?

            Completeness: Does it sufficiently answer the question?

            Relevance: Is it relevant to the question?

            Please respond with a single number (1-5) only.
            "
        );

        let response = self.llm.invoke(&prompt).await?;
        let score = response.trim().parse::<f64>().unwrap_or(0.0);

        Ok(score / 5.0) // 0-1 범위로 정규화
    }

    // 결과 분석 및 보고서 생성
    pub fn generate_report(&self, results: &[EvaluationResult]) -> String {
        let total_tests = results.len();
        let avg_recall =
            results.iter().map(|r| r.metrics.recall_at_5).sum::<f64>() / total_tests as f64;
        let avg_mrr = results.iter().map(|r| r.metrics.mrr).sum::<f64>() / total_tests as f64;
        let avg_bleu =
            results.iter().map(|r| r.metrics.bleu_score).sum::<f64>() / total_tests as f64;
        let avg_bert =
            results.iter().map(|r| r.metrics.bert_score).sum::<f64>() / total_tests as f64;
        let avg_llm_judge = results
            .iter()
            .map(|r| r.metrics.llm_judge_score)
            .sum::<f64>()
            / total_tests as f64;
        let exact_match_rate =
            results.iter().filter(|r| r.metrics.exact_match).count() as f64 / total_tests as f64;

        format!(
            r"
            RAG 시스템 평가 보고서
            ====================

            총 테스트 케이스: {}개

            검색 성능:
            - Recall@5: {:.3}
            - MRR: {:.3}

            생성 성능:
            - BLEU Score: {:.3}
            - BERT Score: {:.3}
            - LLM Judge Score: {:.3}
            - Exact Match Rate: {:.3}

            종합 점수: {:.3}
            ",
            total_tests,
            avg_recall,
            avg_mrr,
            avg_bleu,
            avg_bert,
            avg_llm_judge,
            exact_match_rate,
            (avg_recall + avg_bleu + avg_llm_judge) / 3.0
        )
    }

    // 헬퍼 함수들
    fn extract_question(&self, response: &str) -> Result<String, Box<dyn std::error::Error>> {
        // 실제 구현에서는 정규식이나 더 정교한 파싱 사용
        match response.find("Question:") {
            Some(start) => {
                if let Some(end) = response[start..].find("\n") {
                    let question = response[start..start + end].trim();
                    return Ok(question.to_string());
                }
            }
            _ => (),
        }
        Ok("파싱 실패".to_string())
    }

    fn extract_answer(&self, response: &str) -> std::string::String {
        if let Some(start) = response.find("Answer:") {
            if let Some(end) = response[start..].find('\n') {
                let answer = response[start..start + end].trim();
                return answer.to_string();
            }
        }
        "파싱 실패".to_string()
    }

    fn extract_difficulty(&self, response: &str) -> Result<Difficulty, Box<dyn std::error::Error>> {
        if response.contains("Easy") {
            Ok(Difficulty::Easy)
        } else if response.contains("Medium") {
            Ok(Difficulty::Medium)
        } else if response.contains("Hard") {
            Ok(Difficulty::Hard)
        } else {
            Ok(Difficulty::Medium) // 기본값
        }
    }

    fn extract_category(&self, response: &str) -> Result<String, Box<dyn std::error::Error>> {
        if response.contains("사실") {
            Ok("사실".to_string())
        } else if response.contains("추론") {
            Ok("추론".to_string())
        } else if response.contains("요약") {
            Ok("요약".to_string())
        } else {
            Ok("기타".to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, fs};

    use langchain_rust::{
        embedding::OllamaEmbedder,
        language_models::llm::LLM,
        llm::client::Ollama,
        schemas::{Document, Message},
        vectorstore::{qdrant::StoreBuilder, VecStoreOptions},
    };
    use qdrant_client::{
        qdrant::{vectors::VectorsOptions, ScrollPoints, ScrollPointsBuilder},
        Qdrant,
    };
    use serde::Deserialize;
    use serde_json::{from_str, json};

    use super::RagOllamaSystem;
    use crate::rag_evaluation::RagEvaluator;
    #[tokio::test]
    async fn evaluation_test() {
        let mut rag = RagOllamaSystem::new("nomic-embed-text".to_string(), "qwen3:8b".to_string());

        rag.set_db().await;
        rag.set_chain();
        let llm = Ollama::default().with_model("qwen3:8b");
        let client = Qdrant::from_url("http://localhost:6334").build().unwrap();

        let docs = client
            .scroll(ScrollPointsBuilder::new("rag").limit(30).build())
            .await
            .unwrap()
            .result;
        let store = StoreBuilder::new()
            .embedder(OllamaEmbedder::default())
            .client(client)
            .collection_name("rag")
            .build()
            .await
            .unwrap();

        let mut evaluator = RagEvaluator::new(Box::new(store), rag, llm);
        evaluator.generate_test_cases(docs).await.unwrap();

        let results = evaluator.evaluate_all().await.unwrap();

        let report = evaluator.generate_report(&results);
        println!("{report}");

        let json_results = serde_json::to_string_pretty(&results).unwrap();
        std::fs::write("evaluation_results.json", json_results);
    }

    #[derive(Debug, Deserialize)]
    pub struct QAItem {
        pub question: String,
        pub label: String,
    }

    #[tokio::test]
    async fn query_quantitative() {
        // let llm = Ollama::default().with_model("deepseek-r1:7b");
        // let llm = Ollama::default().with_model("qwen3:8b");

        // let llm = Ollama::default().with_model("lamma3.2");
        let llm = Ollama::default().with_model("codellama:instruct ");

        let system_msg = Message::new_system_message(r#"
        You are a professional assistant trained to classify GitHub-related natural language questions.

        You must label each question as one of the following:

        1. **quantitative** – If the question asks for a number, statistic, or measurable fact.
        Examples:
        - "How many issues has user danbi2990 opened?"
        - "What is the average time to merge a PR?"

        2. **qualitative** – If the question asks for reasoning, opinion, content understanding, or subjective analysis.
        Examples:
        - "What was the reason behind rejecting the PR?"
        - "What concerns did users raise in the discussion?"

        Respond with exactly one word: "quantitative" or "qualitative".
        Do not provide explanations. Focus only on classifying the question type.
        "#.to_string());
        let data = fs::read_to_string("data/questions.json").unwrap();
        let qa_list: Vec<QAItem> = from_str(&data).unwrap();
        let mut correct = 0;

        for qa in &qa_list {
            let human_msg = Message::new_human_message(&qa.question);
            let messages = vec![system_msg.clone(), human_msg];
            let result = llm.generate(&messages).await.unwrap().generation;
            let prediction = result.trim().to_lowercase();
            let expected = qa.label.to_lowercase();

            println!(
                "Q: {}\nPredicted: {}, Expected: {}\n",
                qa.question, prediction, expected
            );

            if prediction == expected {
                correct += 1;
            }
        }
        println!(
            "Accuracy: {:.2}%",
            (correct as f64 / qa_list.len() as f64) * 100.0
        );
    }
}
