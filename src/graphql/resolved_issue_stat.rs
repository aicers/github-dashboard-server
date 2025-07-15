use async_graphql::{Context, Object, SimpleObject};

use crate::graphql::issue_stat::IssueStatFilter;
use crate::graphql::resolved_issue::ResolvedIssue;

#[derive(SimpleObject)]
pub(super) struct ResolvedIssueStat {
    total_count: usize,
}

#[derive(Default)]
pub(super) struct ResolvedIssueStatQuery;

#[Object]
impl ResolvedIssueStatQuery {
    #[allow(clippy::unused_async)]
    async fn resolved_issue_stat(
        &self,
        ctx: &Context<'_>,
        filter: IssueStatFilter,
    ) -> async_graphql::Result<ResolvedIssueStat> {
        let total_count = ResolvedIssue::load(ctx, &filter).len();
        Ok(ResolvedIssueStat { total_count })
    }
}
