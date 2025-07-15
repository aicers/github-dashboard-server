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
        let mut resolved_issues = ResolvedIssue::load(ctx, None, None, &filter)?;
        let total_count = resolved_issues.len();

        // *latest*: The latest created issue (= highest issue number)
        resolved_issues.sort_unstable_by_key(|resolved_issue| resolved_issue.issue.created_at.0);
        let latest = resolved_issues.last().map(std::string::ToString::to_string);

        // *latest_updated*: The latest updated issue
        resolved_issues.sort_unstable_by_key(|resolved_issue| resolved_issue.issue.updated_at.0);
        let latest_updated = resolved_issues.last().map(std::string::ToString::to_string);

        // *latest_closed*: The latest closed issue
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

#[cfg(test)]
mod tests {
    use async_graphql::{Request, Variables};

    use crate::github::GitHubPullRequestRef;
    use crate::{
        github::issues::IssueState, github::issues::PullRequestState, github::GitHubIssue,
        graphql::TestSchema,
    };

    const QUERY: &str = r"
    query ResolvedIssueStat($repo: String, $author: String, $assignee: String) {
        resolvedIssueStat(
            filter: { repo: $repo, author: $author, assignee: $assignee }
        ) {
            totalCount
            latest
            latestUpdated
            latestClosed
        }
    }";

    fn issue_a() -> GitHubIssue {
        let mut issue_a = GitHubIssue::default();

        issue_a.number = 1;
        issue_a.state = IssueState::CLOSED;
        issue_a.created_at = "1999-12-30T14:13:07Z".parse().unwrap();
        issue_a.updated_at = "2000-01-08T12:59:51Z".parse().unwrap();
        issue_a.closed_at = Some("2020-01-08T12:59:51Z".parse().unwrap());

        issue_a
    }

    fn issue_b() -> GitHubIssue {
        let mut issue_b = GitHubIssue::default();

        issue_b.number = 2;
        issue_b.state = IssueState::CLOSED;
        issue_b.created_at = "2000-01-01T00:00:00Z".parse().unwrap();
        issue_b.updated_at = "2000-01-04T20:11:54Z".parse().unwrap();
        issue_b.closed_at = Some("2000-01-04T20:11:54Z".parse().unwrap());
        issue_b.closed_by_pull_requests = vec![GitHubPullRequestRef {
            number: 6,
            state: PullRequestState::MERGED,
            author: "a coder".to_string(),
            created_at: "2000-01-04T20:11:54Z".parse().unwrap(),
            updated_at: "2000-01-04T20:11:54Z".parse().unwrap(),
            closed_at: Some("2000-01-04T20:11:54Z".parse().unwrap()),
            url: "uri to the pull request".to_string(),
        }];

        issue_b
    }

    fn issue_c() -> GitHubIssue {
        let mut issue_c = GitHubIssue::default();

        issue_c.number = 3;
        issue_c.state = IssueState::CLOSED;
        issue_c.created_at = "2000-01-02T08:09:20Z".parse().unwrap();
        issue_c.updated_at = "2000-01-02T10:34:56Z".parse().unwrap();
        issue_c.closed_at = Some("2000-01-02T10:34:56Z".parse().unwrap());
        issue_c.closed_by_pull_requests = vec![GitHubPullRequestRef {
            number: 4,
            state: PullRequestState::MERGED,
            author: "a coder".to_string(),
            created_at: "2000-01-02T10:34:56Z".parse().unwrap(),
            updated_at: "2000-01-02T10:34:56Z".parse().unwrap(),
            closed_at: Some("2000-01-02T10:34:56Z".parse().unwrap()),
            url: "uri to the pull request".to_string(),
        }];

        issue_c
    }

    #[tokio::test]
    async fn test_with_test_db() {
        let _test_schema = TestSchema::new();
        let schema = _test_schema.schema;
        let test_db = _test_schema.db;
        let owner = "repo-owner-leo";
        let name = "repo-name-toy-repo";

        let req = Request::new(QUERY)
            .variables(Variables::from_json(serde_json::json!({ "repo": name })));

        // Testing with 4 issues: default issue, issue_a, issue_b, issue_c
        // 2 of them are resolved issue: issua_b, issue_c
        let _ = test_db.insert_issues(
            vec![GitHubIssue::default(), issue_a(), issue_b(), issue_c()],
            owner,
            name,
        );

        let resp = schema.execute(req).await;
        let data = resp
            .data
            .into_json()
            .expect("Expecting valid json will be returned");

        assert_eq!(data["resolvedIssueStat"]["totalCount"], 2);
        assert_eq!(
            data["resolvedIssueStat"]["latest"],
            "repo-owner-leo/repo-name-toy-repo#3"
        );
        assert_eq!(
            data["resolvedIssueStat"]["latestUpdated"],
            "repo-owner-leo/repo-name-toy-repo#2"
        );
        assert_eq!(
            data["resolvedIssueStat"]["latestClosed"],
            "repo-owner-leo/repo-name-toy-repo#2"
        );
    }
}
