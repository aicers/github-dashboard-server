use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum QueryType {
    Quantitative,
    Qualitative,
    Mixed,
}

impl QueryType {
    pub fn as_str(&self) -> &'static str {
        match self {
            QueryType::Quantitative => "Quantitative",
            QueryType::Qualitative => "Qualitative",
            QueryType::Mixed => "Mixed",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Segment {
    pub id: String,
    pub query_type: QueryType,
    pub enhanced: String,
    pub intent: String,
    pub entities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnhancedQuery {
    #[serde(default)]
    pub original: String,
    pub segments: Vec<Segment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Filter {
    pub field: String,
    pub operator: String,
    pub value: serde_json::Value,
}
