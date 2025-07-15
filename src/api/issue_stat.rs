use async_graphql::{Context, InputObject, Object, Result, SimpleObject};

use crate::{
    api::{issue::Issue, DateTimeUtc},
    database::Iter,
    outbound::issues::IssueState,
    Database,
};

#[derive(InputObject, Debug)]
pub(crate) struct IssueStatFilter {
    /// Filter by assignee.
    assignee: Option<String>,
    /// Filter by issue author.
    author: Option<String>,
    /// Filter by repository name.
    repo: Option<String>,
    /// Start of the creation datetime range. (inclusive)
    /// Example format: "yyyy-MM-ddTHH:mm:ssZ"
    begin: Option<DateTimeUtc>,
    /// End of the creation datetime range. (exclusive)
    /// Example format: "yyyy-MM-ddTHH:mm:ssZ"
    end: Option<DateTimeUtc>,
}

impl IssueStatFilter {
    fn filter_issues(&self, issues: Iter<Issue>) -> Vec<Issue> {
        issues
            .into_iter()
            .filter_map(std::result::Result::ok)
            .filter(|issue| {
                self.author
                    .as_ref()
                    .is_none_or(|author| issue.author == *author)
                    && self.repo.as_ref().is_none_or(|repo| issue.repo == *repo)
                    && self
                        .begin
                        .as_ref()
                        .is_none_or(|begin| issue.created_at.0 >= begin.0)
                    && self
                        .end
                        .as_ref()
                        .is_none_or(|end| issue.created_at.0 < end.0)
                    && self
                        .assignee
                        .as_ref()
                        .is_none_or(|assignee| issue.assignees.iter().any(|a| a == assignee))
            })
            .collect()
    }
}

#[derive(Default)]
pub(super) struct IssueStatQuery {}

#[derive(SimpleObject)]
struct IssueStat {
    /// The number of open issues.
    open_issue_count: i32,

    /// Statistics for resolved issues.
    #[graphql(flatten)]
    resolved_issue_stat: ResolvedIssueStat,
}

#[derive(SimpleObject)]
pub(super) struct ResolvedIssueStat {
    /// The total number of resolved issues.
    /// Currently, a closed issue is considered to be resolved if
    /// (1) "to-do list" status of the issue is "Done"
    /// OR
    /// (2) the issue has one or more closing pr(s) and all of them are merged.
    resolved_issue_count: i32,
}

impl ResolvedIssueStat {
    fn new(resolved_issues: &[&Issue]) -> ResolvedIssueStat {
        let resolved_issue_count = resolved_issues
            .len()
            .try_into()
            .expect("totalCount will not exceed 2^31-1");

        ResolvedIssueStat {
            resolved_issue_count,
        }
    }
}

#[Object]
impl IssueStatQuery {
    #[allow(clippy::unused_async)]
    async fn issue_stat(&self, ctx: &Context<'_>, filter: IssueStatFilter) -> Result<IssueStat> {
        let db = ctx.data::<Database>()?;
        let issues = db.issues(None, None);
        let filtered = filter.filter_issues(issues);
        let open_issue_count = filtered
            .iter()
            .filter(|issue| matches!(issue.state, IssueState::OPEN))
            .count()
            .try_into()?;

        let resolved_issues: Vec<_> = filtered
            .iter()
            .filter(|issue| issue.is_resolved())
            .collect();
        let resolved_issue_stat = ResolvedIssueStat::new(&resolved_issues);

        Ok(IssueStat {
            open_issue_count,
            resolved_issue_stat,
        })
    }
}

#[cfg(test)]
mod tests {
    use async_graphql::{Request, Variables};
    use jiff::Timestamp;

    use crate::{
        api::TestSchema,
        outbound::{
            issues::IssueState, issues::PullRequestState, GitHubIssue, GitHubPullRequestRef,
        },
    };

    const QUERY_ISSUE_STATS: &str = r"
    query IssueStats(
        $repo: String
        $author: String
        $assignee: String
        $begin: DateTimeUtc
        $end: DateTimeUtc
    ) {
        issueStat(
            filter: {
                repo: $repo
                author: $author
                assignee: $assignee
                begin: $begin
                end: $end
            }
        ) {
            openIssueCount
            resolvedIssueCount
        }
    }";

    fn create_issues(n: usize) -> Vec<GitHubIssue> {
        (0..n)
            .map(|i| GitHubIssue {
                number: i.try_into().unwrap(),
                ..Default::default()
            })
            .collect()
    }

    fn add_closing_pr(issue: &mut GitHubIssue, created: impl Into<String>) {
        issue.closed_by_pull_requests = vec![GitHubPullRequestRef {
            number: issue.number + 100,
            state: PullRequestState::MERGED,
            author: issue
                .assignees
                .first()
                .expect("Issue has assignee at least one")
                .to_string(),
            created_at: created
                .into()
                .parse()
                .expect("Issue is closed and has correct timestamp"),
            updated_at: issue
                .closed_at
                .expect("Issue is closed and has correct timestamp"),
            closed_at: Some(
                issue
                    .closed_at
                    .expect("Issue is closed and has correct timestamp"),
            ),
            url: "<my github issue uri>".to_string(),
        }];
    }

    fn parse(date: &str) -> Timestamp {
        date.parse().unwrap()
    }

    #[tokio::test]
    async fn open_issue_count_by_author() {
        let schema = TestSchema::new();
        let mut issues = create_issues(3);
        issues[0].author = "foo".to_string();
        schema
            .db
            .insert_issues(issues, "aicers", "github-dashboard-server")
            .unwrap();

        let query = r#"
        {
            issueStat(filter: {author: "foo"}) {
                openIssueCount
            }
        }"#;
        let data = schema.execute(query).await.data.into_json().unwrap();
        assert_eq!(data["issueStat"]["openIssueCount"], 1);
    }

    #[tokio::test]
    async fn open_issue_count_by_begin_end() {
        let schema = TestSchema::new();
        let mut issues = create_issues(3);
        issues[1].created_at = parse("2025-01-05T00:00:00Z");
        issues[2].created_at = parse("2025-01-06T00:00:00Z");

        schema
            .db
            .insert_issues(issues, "aicers", "github-dashboard-server")
            .unwrap();

        let query = r#"
        {
            issueStat(filter: {begin: "2025-01-06T00:00:00Z"}) {
                openIssueCount
            }
        }"#;
        let data = schema.execute(query).await.data.into_json().unwrap();
        assert_eq!(data["issueStat"]["openIssueCount"], 1);

        let query = r#"
        {
            issueStat(filter: {begin: "2025-01-05T00:00:00Z", end: "2025-01-06T00:00:00Z"}) {
                openIssueCount
            }
        }"#;
        let data = schema.execute(query).await.data.into_json().unwrap();

        assert_eq!(data["issueStat"]["openIssueCount"], 1);
    }

    #[tokio::test]
    async fn open_issue_count_by_author_and_dates() {
        let schema = TestSchema::new();
        let mut issues = create_issues(3);
        issues[1].author = "foo".to_string();
        issues[1].created_at = parse("2025-01-05T00:00:00Z");
        issues[2].created_at = parse("2025-01-06T00:00:00Z");

        schema
            .db
            .insert_issues(issues, "aicers", "github-dashboard-server")
            .unwrap();
        let query = r#"
        {
            issueStat(filter: {author: "foo", begin: "2025-01-05T00:00:00Z", end: "2025-01-06T00:00:00Z"}) {
                openIssueCount
            }
        }"#;
        let data = schema.execute(query).await.data.into_json().unwrap();

        assert_eq!(data["issueStat"]["openIssueCount"], 1);
    }

    #[tokio::test]
    async fn open_issue_count_by_repo() {
        let schema = TestSchema::new();
        let server_issues = create_issues(2);
        let client_issues = create_issues(1);

        schema
            .db
            .insert_issues(server_issues, "aicers", "github-dashboard-server")
            .unwrap();
        schema
            .db
            .insert_issues(client_issues, "aicers", "github-dashboard-client")
            .unwrap();
        let query = r#"
        {
            issueStat(filter: {repo: "github-dashboard-client"}) {
                openIssueCount
            }
        }"#;
        let data = schema.execute(query).await.data.into_json().unwrap();

        assert_eq!(data["issueStat"]["openIssueCount"], 1);
    }

    #[tokio::test]
    async fn open_issue_count_by_repo_and_author() {
        let schema = TestSchema::new();
        let server_issues = create_issues(1);
        let mut client_issues = create_issues(2);
        client_issues[1].author = "foo".to_string();
        schema
            .db
            .insert_issues(server_issues, "aicers", "github-dashboard-server")
            .unwrap();
        schema
            .db
            .insert_issues(client_issues, "aicers", "github-dashboard-client")
            .unwrap();

        let query = r#"
        {
            issueStat(filter: {repo: "github-dashboard-client", author: "foo"}) {
                openIssueCount
            }
        }"#;
        let data = schema.execute(query).await.data.into_json().unwrap();
        assert_eq!(data["issueStat"]["openIssueCount"], 1);
    }

    #[tokio::test]
    async fn open_issue_count_by_assignee() {
        let schema = TestSchema::new();
        let mut issues = create_issues(3);
        issues[0].assignees = vec!["alice".to_string(), "bob".to_string()];
        issues[1].assignees = vec!["alice".to_string()];

        schema
            .db
            .insert_issues(issues, "aicers", "github-dashboard-server")
            .unwrap();

        let query = r#"
        {
            issueStat(filter: {assignee: "alice"}) {
                openIssueCount
            }
        }"#;
        let data = schema.execute(query).await.data.into_json().unwrap();
        assert_eq!(data["issueStat"]["openIssueCount"], 2);
    }

    #[tokio::test]
    async fn resolved_issue_stat() {
        let owner = "aicers";
        let repo = "our-test-repository";
        let schema = TestSchema::new();
        let mut open_issues = create_issues(2);
        let mut closed_issues = create_issues(2);
        let mut resolved_issues = create_issues(5);

        // NOTE: Default IssueState is IssueState::OPEN.
        for closed_issue in &mut closed_issues {
            closed_issue.state = IssueState::CLOSED;
        }

        for resolved_issue in &mut resolved_issues {
            resolved_issue.state = IssueState::CLOSED;
        }

        open_issues[0].created_at = parse("2025-01-02T00:00:00Z"); // issue #2
        open_issues[1].created_at = parse("2025-01-08T00:00:00Z"); // issue #8
        closed_issues[0].created_at = parse("2025-01-05T00:00:00Z"); // issue #5
        closed_issues[1].created_at = parse("2025-01-09T00:00:00Z"); // issue #9
        resolved_issues[0].created_at = parse("2025-01-01T00:00:00Z"); // issue #1, assignee: Leo
        resolved_issues[1].created_at = parse("2025-01-03T00:00:00Z"); // issue #3, assignee: Jake
        resolved_issues[2].created_at = parse("2025-01-04T00:00:00Z"); // issue #4, assignee: Jake
        resolved_issues[3].created_at = parse("2025-01-06T00:00:00Z"); // issue #6, assignee: Leo
        resolved_issues[4].created_at = parse("2025-01-07T00:00:00Z"); // issue #7, assignee: Jake

        closed_issues[0].closed_at = Some(parse("2025-01-06T00:10:20Z"));
        closed_issues[1].closed_at = Some(parse("2025-01-27T00:10:20Z"));
        resolved_issues[0].closed_at = Some(parse("2025-01-02T00:10:20Z"));
        resolved_issues[1].closed_at = Some(parse("2025-01-12T00:10:20Z"));
        resolved_issues[2].closed_at = Some(parse("2025-01-09T00:10:20Z"));
        resolved_issues[3].closed_at = Some(parse("2025-01-08T00:10:20Z"));
        resolved_issues[4].closed_at = Some(parse("2025-01-11T00:10:20Z"));

        open_issues[0].updated_at = parse("2025-01-02T00:00:00Z");
        open_issues[1].updated_at = parse("2025-01-08T00:00:00Z");
        closed_issues[0].updated_at = parse("2025-01-06T00:11:40Z");
        closed_issues[1].updated_at = parse("2025-01-27T00:10:20Z");
        resolved_issues[0].updated_at = parse("2025-01-02T00:10:20Z");
        resolved_issues[1].updated_at = parse("2025-01-07T00:10:20Z");
        resolved_issues[2].updated_at = parse("2025-01-09T00:10:20Z");
        resolved_issues[3].updated_at = parse("2025-01-25T00:08:45Z");
        resolved_issues[4].updated_at = parse("2025-01-12T00:10:20Z");

        // All resolved issues have MERGED closing pr
        resolved_issues[0].assignees = vec!["Leo".to_string()];
        resolved_issues[1].assignees = vec!["Jake".to_string()];
        resolved_issues[2].assignees = vec!["Jake".to_string()];
        resolved_issues[3].assignees = vec!["Leo".to_string()];
        resolved_issues[4].assignees = vec!["Jake".to_string()];

        add_closing_pr(&mut resolved_issues[0], "2025-01-02T00:07:30Z");
        add_closing_pr(&mut resolved_issues[1], "2025-01-10T00:10:20Z");
        add_closing_pr(&mut resolved_issues[2], "2025-01-08T00:10:20Z");
        add_closing_pr(&mut resolved_issues[3], "2025-01-07T00:09:40Z");
        add_closing_pr(&mut resolved_issues[4], "2025-01-10T00:09:40Z");

        let mut all_issues = Vec::<GitHubIssue>::new();
        all_issues.extend(open_issues);
        all_issues.extend(closed_issues);
        all_issues.extend(resolved_issues);

        // Issue number is deteremined by *created_at*
        all_issues.sort_unstable_by_key(|issue| issue.created_at);
        all_issues.iter_mut().enumerate().for_each(|(i, issue)| {
            issue.number = (i + 1).try_into().expect("*i* will not exceed 2^32");
        });

        schema.db.insert_issues(all_issues, owner, repo).unwrap();

        let request1 = Request::new(QUERY_ISSUE_STATS)
            .variables(Variables::from_json(serde_json::json!({ "repo": repo })));
        let data1 = schema
            .schema
            .execute(request1)
            .await
            .data
            .into_json()
            .expect("Expecting valid json will be returned");

        assert_eq!(data1["issueStat"]["resolvedIssueCount"], 5);

        let request2 = Request::new(QUERY_ISSUE_STATS).variables(Variables::from_json(
            serde_json::json!({ "repo": repo, "assignee": "Leo" }),
        ));
        let data2 = schema
            .schema
            .execute(request2)
            .await
            .data
            .into_json()
            .expect("Expecting valid json will be returned");

        assert_eq!(data2["issueStat"]["resolvedIssueCount"], 2);
    }
}
