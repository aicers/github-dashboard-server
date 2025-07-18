use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum QueryType {
    Quantitative,
    Qualitative,
}

impl QueryType {
    pub fn as_str(&self) -> &'static str {
        match self {
            QueryType::Quantitative => "Quantitative",
            QueryType::Qualitative => "Qualitative", // Add other variants if needed
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Segment {
    pub query_type: QueryType, // Quantitative or Qualitative
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
pub struct ParsedSegment {
    pub segment_type: String,
    pub parameters: serde_json::Value,
    pub filters: Vec<Filter>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Filter {
    pub field: String,
    pub operator: String,
    pub value: serde_json::Value,
}
