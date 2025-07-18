pub mod graph;
pub mod session_keys;
// pub mod storage;
pub mod tasks;
pub mod types;
pub mod utils;

use std::{any::type_name, sync::Arc};

use graph_flow::{
    FlowRunner, Graph, GraphBuilder, InMemorySessionStorage, Session, SessionStorage, Task,
};

use crate::lang_graph::tasks::query_enhancement::QueryEnhancementTask;

// use crate::storage::{SledDB, VectorDB};

pub struct GitHubRAGSystem {
    // pub flow_runner: FlowRunner,
    pub graph: Arc<Graph>,
    pub session_storage: Arc<dyn SessionStorage>,
    // pub vector_db: Arc<dyn VectorDB>,
    // pub sled_db: Arc<dyn SledDB>,
}

impl GitHubRAGSystem {
    pub async fn new(// vector_db: Arc<dyn VectorDB>,
        // sled_db: Arc<dyn SledDB>,
    ) -> anyhow::Result<Self> {
        let graph = graph::build_rag_graph().await?;
        let session_storage = Arc::new(InMemorySessionStorage::new());
        // let flow_runner = FlowRunner::new(graph, session_storage);

        Ok(Self {
            graph,
            session_storage,
            // flow_runner,
            // vector_db,
            // sled_db,
        })
    }

    pub async fn query(&self, user_query: &str) -> anyhow::Result<String> {
        let session_id = uuid::Uuid::new_v4().to_string();
        let session =
            Session::new_from_task(session_id.clone(), type_name::<QueryEnhancementTask>());

        // 초기 쿼리 설정
        session
            .context
            .set(session_keys::USER_QUERY, user_query.to_string())
            .await;
        // self.flow_runner.storage.save(session).await?;
        self.session_storage.save(session).await?;

        let flow_runner = FlowRunner::new(self.graph.clone(), self.session_storage.clone());

        // 워크플로우 실행
        loop {
            let result = flow_runner.run(&session_id).await?;

            match result.status {
                graph_flow::ExecutionStatus::Completed => {
                    return Ok(result.response.unwrap_or_default());
                }
                graph_flow::ExecutionStatus::Error(_) => {
                    // return Err(err);
                    continue;
                }
                graph_flow::ExecutionStatus::WaitingForInput => {
                    // RAG 시스템에서는 일반적으로 사용자 입력 대기는 없음
                    continue;
                }
                graph_flow::ExecutionStatus::Paused {
                    next_task_id,
                    reason,
                } => continue,
            }
        }
    }
}
