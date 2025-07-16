use async_graphql::{Context, Object, SimpleObject};

use crate::graphql::issue_stat::IssueStatFilter;
use crate::graphql::resolved_issue::ResolvedIssue;
use crate::graphql::DateTimeUtc;

#[derive(SimpleObject)]
pub(super) struct ResolvedIssueStat {
    total_count: usize,
    latest: Option<String>,
    latest_updated: Option<String>,
    latest_closed: Option<String>,
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
        let mut resolved_issues = ResolvedIssue::load(ctx, &filter);
        let total_count = resolved_issues.len();

        // *latest*: The latest created issue (= highest issue number)
        resolved_issues.sort_unstable_by_key(|resolved_issue| resolved_issue.issue.created_at.0);
        let latest = resolved_issues.last().map(std::string::ToString::to_string);

        // *latest_updated*: The latest updated issue
        resolved_issues.sort_unstable_by_key(|resolved_issue| resolved_issue.issue.updated_at.0);
        let latest_updated = resolved_issues.last().map(std::string::ToString::to_string);

        //
        resolved_issues.sort_unstable_by_key(|resolved_issue| {
            resolved_issue
                .issue
                .closed_at
                .clone()
                .unwrap_or(DateTimeUtc(jiff::Timestamp::MAX))
                .0
        });
        let latest_closed = resolved_issues.last().map(std::string::ToString::to_string);

        Ok(ResolvedIssueStat {
            total_count,
            latest,
            latest_updated,
            latest_closed,
        })
    }
}
