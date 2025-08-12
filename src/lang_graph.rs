pub mod graph;
pub mod session_keys;
pub mod tasks;
pub mod types;

use std::{any::type_name, sync::Arc};

use graph_flow::{FlowRunner, Graph, InMemorySessionStorage, Session, SessionStorage};
use tracing::error;

use crate::{database::Database, lang_graph::tasks::query_enhancement::QueryEnhancementTask};

pub struct GitHubRAGSystem {
    pub graph: Arc<Graph>,
    pub session_storage: Arc<dyn SessionStorage>,
}

impl GitHubRAGSystem {
    pub async fn new(database: Database) -> anyhow::Result<Self> {
        let graph = graph::build_rag_graph(database).await?;
        let session_storage = Arc::new(InMemorySessionStorage::new());

        Ok(Self {
            graph,
            session_storage,
        })
    }

    pub async fn query(&self, user_query: &str) -> anyhow::Result<String> {
        let session_id = uuid::Uuid::new_v4().to_string();
        let session =
            Session::new_from_task(session_id.clone(), type_name::<QueryEnhancementTask>());

        session
            .context
            .set(session_keys::USER_QUERY, user_query.to_string())
            .await;
        self.session_storage.save(session).await?;

        let flow_runner = FlowRunner::new(self.graph.clone(), self.session_storage.clone());

        loop {
            let result = flow_runner.run(&session_id).await;
            let session = self.session_storage.get(&session_id).await?.unwrap();

            match result {
                Ok(result) => match result.status {
                    graph_flow::ExecutionStatus::Completed => {
                        session.context.clear().await;
                        return Ok(result.response.unwrap_or_default());
                    }
                    graph_flow::ExecutionStatus::Error(e) => {
                        error!("{}", &e);
                        session.context.set("Error_Reason", e).await;
                    }
                    graph_flow::ExecutionStatus::WaitingForInput
                    | graph_flow::ExecutionStatus::Paused { .. } => {}
                },
                Err(e) => error!("{e}"),
            }
        }
    }
}
