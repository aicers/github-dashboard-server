use std::sync::Arc;

use graph_flow::{GraphBuilder, Task};

use crate::{
    database::Database,
    lang_graph::{
        session_keys,
        tasks::{
            context_reranking, graphql_executor, graphql_generator, query_enhancement,
            rag_generation, response_formatter, segment_parser, statistics_response,
            type_validation, vector_search,
        },
    },
};

static MODEL: &str = "gpt-oss:20b";

#[allow(clippy::unused_async)]
pub async fn build_rag_graph(database: Database) -> anyhow::Result<Arc<graph_flow::Graph>> {
    let query_enhancement = Arc::new(query_enhancement::QueryEnhancementTask::new(MODEL));
    let segment_parser = Arc::new(segment_parser::SegmentParserTask::new());
    let type_validation = Arc::new(type_validation::TypeValidationTask::new(MODEL));
    let vector_search = Arc::new(vector_search::VectorSearchTask::new(MODEL));
    let context_reranking = Arc::new(context_reranking::ContextRerankTask::new(MODEL));
    let rag_generation = Arc::new(rag_generation::RAGGenerationTask::new(MODEL));
    let graphql_generator = Arc::new(graphql_generator::GraphQLGeneratorTask::new(MODEL));
    let graphql_executor = Arc::new(graphql_executor::GraphQLExecutorTask::new(database));
    let statistics_response = Arc::new(statistics_response::StatisticsResponseTask::new(MODEL));
    let response_formatter = Arc::new(response_formatter::ResponseFormatterTask::new(MODEL));

    let graph = GraphBuilder::new("github_rag_workflow")
        .add_task(query_enhancement.clone())
        .add_task(segment_parser.clone())
        .add_task(type_validation.clone())
        .add_task(vector_search.clone())
        .add_task(context_reranking.clone())
        .add_task(rag_generation.clone())
        .add_task(graphql_generator.clone())
        .add_task(graphql_executor.clone())
        .add_task(statistics_response.clone())
        .add_task(response_formatter.clone())
        .add_edge(query_enhancement.id(), type_validation.id())
        .add_conditional_edge(
            type_validation.id(),
            |ctx| {
                ctx.get_sync::<bool>(session_keys::VALIDATION_PASS)
                    .unwrap_or(false)
            },
            segment_parser.id(),
            query_enhancement.id(),
        )
        .add_edge(segment_parser.id(), vector_search.id())
        .add_edge(vector_search.id(), context_reranking.id())
        .add_edge(context_reranking.id(), rag_generation.id())
        .add_edge(rag_generation.id(), graphql_generator.id())
        .add_edge(graphql_generator.id(), graphql_executor.id())
        .add_conditional_edge(
            graphql_executor.id(),
            |ctx| {
                ctx.get_sync::<bool>(session_keys::GRAPHQL_EXECUTE_ERROR)
                    .unwrap_or(false)
            },
            graphql_generator.id(),
            statistics_response.id(),
        )
        .add_edge(statistics_response.id(), response_formatter.id())
        .build();

    Ok(Arc::new(graph))
}
