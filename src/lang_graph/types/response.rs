use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorSearchResult {
    pub(crate) id: String,
    pub(crate) content: String,
    pub(crate) metadata: Value,
    pub(crate) score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualitativeResult {
    pub segment_id: String,
    pub vector_search_results: Vec<VectorSearchResult>,
    pub generated_response: String,
}
