// 모든 세션에서 사용되는 키들을 상수로 정의
pub const USER_QUERY: &str = "user_query";
pub const ENHANCED_QUERY: &str = "enhanced_query";
pub const GRAPHQL_QUERY: &str = "graphql_query";
pub const GRAPHQL_RESULT: &str = "graphql_result";
pub const VECTOR_SEARCH_RESULTS: &str = "vector_search_results";
pub const RERANKED_CONTEXTS: &str = "reranked_contexts";
pub const STATISTICS_RESPONSE: &str = "statistics_response";
pub const RAG_RESPONSE: &str = "rag_response";
pub const FINAL_RESPONSE: &str = "final_response";

// 새로운 segment 기반 처리를 위한 키들
pub const QUANTITATIVE_SEGMENTS: &str = "quantitative_segments";
pub const QUALITATIVE_SEGMENTS: &str = "qualitative_segments";
pub const MIXED_SEGMENTS: &str = "mixed_segments";
pub const SEGMENT_RESULTS: &str = "segment_results";
pub const PROCESSING_STRATEGY: &str = "processing_strategy";
