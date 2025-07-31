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
                        .is_none_or(|begin| issue.created_at >= *begin)
                    && self.end.as_ref().is_none_or(|end| issue.created_at < *end)
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

        Ok(IssueStat { open_issue_count })
    }
}

#[cfg(test)]
mod tests {
    use jiff::Timestamp;

    use crate::{api::TestSchema, outbound::GitHubIssue};

    fn create_issues(n: usize) -> Vec<GitHubIssue> {
        (0..n)
            .map(|i| GitHubIssue {
                number: i.try_into().unwrap(),
                ..Default::default()
            })
            .collect()
    }

    fn parse(date: &str) -> Timestamp {
        date.parse().unwrap()
    }

    #[tokio::test]
    async fn open_issue_count_by_author() {
        let schema = TestSchema::new().await;
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
        let schema = TestSchema::new().await;
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
        let schema = TestSchema::new().await;
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
        let schema = TestSchema::new().await;
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
        let schema = TestSchema::new().await;
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
        let schema = TestSchema::new().await;
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
}
