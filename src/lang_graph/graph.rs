use std::sync::Arc;

use graph_flow::{GraphBuilder, Task};

use crate::lang_graph::tasks::*;

#[allow(clippy::unused_async)]
pub async fn build_rag_graph() -> anyhow::Result<Arc<graph_flow::Graph>> {
    // 태스크 인스턴스 생성
    let query_enhancement = Arc::new(query_enhancement::QueryEnhancementTask::new());
    let segment_parser = Arc::new(segment_parser::SegmentParserTask::new());
    let vector_search = Arc::new(vector_search::VectorSearchTask::new());
    let context_reranking = Arc::new(context_reranking::ContextRerankingTask::new());
    let rag_generation = Arc::new(rag_generation::RAGGenerationTask::new());

    /*
    let graphql_generator = Arc::new(graphql_generator::GraphQLGeneratorTask::new());
    let graphql_executor = Arc::new(graphql_executor::GraphQLExecutorTask::new(sled_db.clone()));
    let statistics_response = Arc::new(statistics_response::StatisticsResponseTask::new());
    let response_formatter = Arc::new(response_formatter::ResponseFormatterTask::new());
    */

    // 그래프 빌드 - 단일 플로우로 모든 경우 처리
    let graph = GraphBuilder::new("github_rag_workflow")
        .add_task(query_enhancement.clone())
        .add_task(segment_parser.clone())
        .add_task(vector_search.clone())
        .add_task(context_reranking.clone())
        .add_task(rag_generation.clone())
        // .add_task(graphql_generator.clone())
        // .add_task(graphql_executor.clone())
        // .add_task(statistics_response.clone())
        // .add_task(response_formatter.clone())
        // // 순차적 연결 - 각 태스크에서 내부적으로 필요한 부분만 처리
        .add_edge(query_enhancement.id(), segment_parser.id())
        .add_edge(segment_parser.id(), vector_search.id())
        .add_edge(vector_search.id(), context_reranking.id())
        .add_edge(context_reranking.id(), rag_generation.id())
        // .add_edge(graphql_generator.id(), graphql_executor.id())
        // .add_edge(graphql_executor.id(), vector_search.id())
        // .add_edge(statistics_response.id(), rag_generation.id())
        // .add_edge(rag_generation.id(), response_formatter.id())
        .build();

    Ok(Arc::new(graph))
}
