use async_graphql::SimpleObject;

#[derive(SimpleObject)]
pub(super) struct TotalCountField {
    pub(super) total_count: usize,
}
