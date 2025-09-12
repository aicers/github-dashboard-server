use std::collections::BTreeMap;

use async_graphql::{Context, Enum, InputObject, Object, Result, SimpleObject};

use crate::{
    api::{issue::Issue, DateTimeUtc},
    database::Iter,
    outbound::issues::IssueState,
    Database,
};

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug, PartialOrd, Ord)]
pub enum IssueSize {
    None,
    XS,
    S,
    M,
    L,
    XL,
}

impl From<&str> for IssueSize {
    fn from(value: &str) -> Self {
        match value {
            "XS" => Self::XS,
            "S" => Self::S,
            "M" => Self::M,
            "L" => Self::L,
            "XL" => Self::XL,
            _ => Self::None,
        }
    }
}

#[derive(SimpleObject)]
struct IssueSizeCount {
    size: IssueSize,
    count: usize,
}

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

    /// The number of resolved issues.
    resolved_issue_count: i32,

    /// The distribution of resolved issues by size.
    resolved_issue_size_distribution: Vec<IssueSizeCount>,
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
        let resolved_issue_count = resolved_issues
            .len()
            .try_into()
            .expect("The number of resolved issues will not exceed i32::MAX");

        let resolved_issue_size_distribution = resolved_issues
            .iter()
            .fold(BTreeMap::new(), |mut acc, issue| {
                let size_str = issue
                    .project_items
                    .nodes
                    .iter()
                    .find(|item| item.project_title == super::issue::TODO_LIST_PROJECT_TITLE)
                    .and_then(|item| item.todo_size.as_deref())
                    .unwrap_or("None");
                *acc.entry(size_str.into()).or_insert(0) += 1;
                acc
            })
            .into_iter()
            .map(|(size, count)| IssueSizeCount { size, count })
            .collect();

        Ok(IssueStat {
            open_issue_count,
            resolved_issue_count,
            resolved_issue_size_distribution,
        })
    }
}

#[cfg(test)]
mod tests {
    use jiff::Timestamp;

    use crate::{
        api::issue::{TODO_LIST_PROJECT_TITLE, TODO_LIST_STATUS_DONE},
        api::TestSchema,
        database::issue::{GitHubIssue, GitHubProjectV2Item, GitHubProjectV2ItemConnection},
        outbound::issues::IssueState,
    };

    fn create_issues(n: usize) -> Vec<GitHubIssue> {
        (1..=n)
            .map(|i| GitHubIssue {
                number: i.try_into().unwrap(),
                ..Default::default()
            })
            .collect()
    }

    fn create_resolved_issues<R>(range: R) -> Vec<GitHubIssue>
    where
        R: Iterator<Item = usize>,
    {
        range
            .map(|i| GitHubIssue {
                number: i.try_into().unwrap(),
                state: IssueState::CLOSED,
                project_items: GitHubProjectV2ItemConnection {
                    total_count: 1,
                    nodes: vec![GitHubProjectV2Item {
                        // Essential Values to be determined as a resolved issue
                        project_title: TODO_LIST_PROJECT_TITLE.to_string(),
                        todo_status: Some(TODO_LIST_STATUS_DONE.to_string()),

                        // these fields are not used for this tests
                        project_id: "Not Used".to_string(),
                        id: "Not Used".to_string(),
                        todo_priority: None,
                        todo_size: None,
                        todo_initiation_option: None,
                        todo_pending_days: None,
                    }],
                },
                ..Default::default()
            })
            .collect()
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
    async fn resolved_issue_count() {
        let schema = TestSchema::new();
        let owner: &str = "aicers";
        let repo = "github-dashboard-server";

        schema
            .db
            .insert_issues(create_issues(3), owner, repo)
            .unwrap();
        schema
            .db
            .insert_issues(create_resolved_issues(4..=8), owner, repo)
            .unwrap();

        let query = "
        {
            issueStat(filter: {}) {
                resolvedIssueCount
            }
        }";
        let data = schema.execute(query).await.data.into_json().unwrap();
        assert_eq!(data["issueStat"]["resolvedIssueCount"], 5);
    }

    #[tokio::test]
    async fn resolved_issue_count_by_assignee() {
        let schema = TestSchema::new();
        let owner: &str = "aicers";
        let repo = "github-dashboard-server";
        let mut issues = create_issues(3);
        let mut resolved_issues = create_resolved_issues(4..=8);
        issues[0].assignees = vec!["alice".to_string(), "bob".to_string()];
        issues[1].assignees = vec!["alice".to_string()];
        resolved_issues[0].assignees = vec!["alice".to_string(), "bob".to_string()];
        resolved_issues[1].assignees = vec!["alice".to_string()];

        schema.db.insert_issues(issues, owner, repo).unwrap();
        schema
            .db
            .insert_issues(resolved_issues, owner, repo)
            .unwrap();

        let query = r#"
        {
            issueStat(filter: {assignee: "alice"}) {
                resolvedIssueCount
            }
        }"#;
        let data = schema.execute(query).await.data.into_json().unwrap();
        assert_eq!(data["issueStat"]["resolvedIssueCount"], 2);
    }

    #[tokio::test]
    async fn resolved_issue_count_by_author() {
        let schema = TestSchema::new();
        let owner: &str = "aicers";
        let repo = "github-dashboard-server";
        let mut issues = create_issues(3);
        let mut resolved_issues = create_resolved_issues(4..=8);
        issues[0].author = "alice".to_string();
        resolved_issues[0].author = "alice".to_string();

        schema.db.insert_issues(issues, owner, repo).unwrap();
        schema
            .db
            .insert_issues(resolved_issues, owner, repo)
            .unwrap();

        let query = r#"
        {
            issueStat(filter: {author: "alice"}) {
                resolvedIssueCount
            }
        }"#;
        let data = schema.execute(query).await.data.into_json().unwrap();
        assert_eq!(data["issueStat"]["resolvedIssueCount"], 1);
    }

    #[tokio::test]
    async fn resolved_issue_count_by_repo() {
        let schema = TestSchema::new();
        let owner: &str = "aicers";
        schema
            .db
            .insert_issues(create_issues(3), owner, "github-dashboard-server")
            .unwrap();
        schema
            .db
            .insert_issues(
                create_resolved_issues(4..=8),
                owner,
                "github-dashboard-server",
            )
            .unwrap();
        schema
            .db
            .insert_issues(create_issues(3), owner, "github-dashboard-client")
            .unwrap();
        schema
            .db
            .insert_issues(
                create_resolved_issues(4..=8),
                owner,
                "github-dashboard-client",
            )
            .unwrap();

        let query = r#"
        {
            issueStat(filter: {repo: "github-dashboard-server"}) {
                resolvedIssueCount
            }
        }"#;
        let data = schema.execute(query).await.data.into_json().unwrap();
        assert_eq!(data["issueStat"]["resolvedIssueCount"], 5);
    }

    // NOTE: `begin` field is inclusive (c.f. `end` field is exclusive)
    #[tokio::test]
    async fn resolved_issue_count_by_begin() {
        let schema = TestSchema::new();
        let owner: &str = "aicers";
        let repo = "github-dashboard-server";
        let mut issues = create_issues(3);
        let mut resolved_issues = create_resolved_issues(4..=8);
        issues[0].created_at = parse("2025-01-05T00:00:00Z");
        issues[1].created_at = parse("2025-01-06T00:00:00Z");
        resolved_issues[0].created_at = parse("2025-01-05T00:00:00Z");
        resolved_issues[1].created_at = parse("2025-01-06T00:00:00Z");

        schema.db.insert_issues(issues, owner, repo).unwrap();
        schema
            .db
            .insert_issues(resolved_issues, owner, repo)
            .unwrap();

        let query = r#"
        {
            issueStat(filter: {begin: "2025-01-05T00:00:00Z"}) {
                resolvedIssueCount
            }
        }"#;
        let data = schema.execute(query).await.data.into_json().unwrap();
        assert_eq!(data["issueStat"]["resolvedIssueCount"], 2);
    }

    // NOTE: `end` field is exclusive (c.f. `begin` field is inclusive)
    #[tokio::test]
    async fn resolved_issue_count_by_end() {
        let schema = TestSchema::new();
        let owner: &str = "aicers";
        let repo = "github-dashboard-server";
        let mut issues = create_issues(3);
        let mut resolved_issues = create_resolved_issues(4..=8);
        issues[0].created_at = parse("2025-01-05T00:00:00Z");
        issues[1].created_at = parse("2025-01-06T00:00:00Z");
        resolved_issues[0].created_at = parse("2025-01-05T00:00:00Z");
        resolved_issues[1].created_at = parse("2025-01-06T00:00:00Z");
        resolved_issues[2].created_at = parse("2025-01-07T00:00:00Z");
        resolved_issues[3].created_at = parse("2025-01-08T00:00:00Z");
        resolved_issues[4].created_at = parse("2025-01-09T00:00:00Z");

        schema.db.insert_issues(issues, owner, repo).unwrap();
        schema
            .db
            .insert_issues(resolved_issues, owner, repo)
            .unwrap();

        let query = r#"
        {
            issueStat(filter: {end: "2025-01-07T00:00:00Z"}) {
                resolvedIssueCount
            }
        }"#;
        let data = schema.execute(query).await.data.into_json().unwrap();
        assert_eq!(data["issueStat"]["resolvedIssueCount"], 2);
    }

    #[tokio::test]
    async fn resolved_issue_count_by_begin_and_end() {
        let schema = TestSchema::new();
        let owner: &str = "aicers";
        let repo = "github-dashboard-server";
        let mut issues = create_issues(3);
        let mut resolved_issues = create_resolved_issues(4..=8);
        issues[0].created_at = parse("2025-01-05T00:00:00Z");
        issues[1].created_at = parse("2025-01-06T00:00:00Z");
        resolved_issues[0].created_at = parse("2025-01-05T00:00:00Z");
        resolved_issues[1].created_at = parse("2025-01-06T00:00:00Z");
        resolved_issues[2].created_at = parse("2025-01-07T00:00:00Z");
        resolved_issues[3].created_at = parse("2025-01-08T00:00:00Z");
        resolved_issues[4].created_at = parse("2025-01-09T00:00:00Z");

        schema.db.insert_issues(issues, owner, repo).unwrap();
        schema
            .db
            .insert_issues(resolved_issues, owner, repo)
            .unwrap();

        let query = r#"
        {
            issueStat(filter: {begin: "2025-01-06T00:00:00Z", end: "2025-01-07T00:00:00Z"}) {
                resolvedIssueCount
            }
        }"#;
        let data = schema.execute(query).await.data.into_json().unwrap();
        assert_eq!(data["issueStat"]["resolvedIssueCount"], 1);
    }

    #[tokio::test]
    async fn resolved_issue_size_distribution() {
        let schema = TestSchema::new();
        let owner: &str = "aicers";
        let repo = "github-dashboard-server";

        let mut resolved_issues = create_resolved_issues(1..=6);
        // 1 XS
        resolved_issues[0]
            .project_items
            .nodes
            .get_mut(0)
            .unwrap()
            .todo_size = Some("XS".to_string());
        // 2 S
        resolved_issues[1]
            .project_items
            .nodes
            .get_mut(0)
            .unwrap()
            .todo_size = Some("S".to_string());
        resolved_issues[2]
            .project_items
            .nodes
            .get_mut(0)
            .unwrap()
            .todo_size = Some("S".to_string());
        // 1 M
        resolved_issues[3]
            .project_items
            .nodes
            .get_mut(0)
            .unwrap()
            .todo_size = Some("M".to_string());
        // invalid size -> None
        resolved_issues[4]
            .project_items
            .nodes
            .get_mut(0)
            .unwrap()
            .todo_size = Some("invalid".to_string());
        // no size -> None
        resolved_issues[5]
            .project_items
            .nodes
            .get_mut(0)
            .unwrap()
            .todo_size = None;

        schema
            .db
            .insert_issues(resolved_issues, owner, repo)
            .unwrap();

        let query = r"
        {
            issueStat(filter: {}) {
                resolvedIssueSizeDistribution {
                    size
                    count
                }
            }
        }";
        let data = schema.execute(query).await.data.into_json().unwrap();
        assert_eq!(
            data["issueStat"]["resolvedIssueSizeDistribution"],
            serde_json::json!([
                { "size": "NONE", "count": 2 },
                { "size": "XS", "count": 1 },
                { "size": "S", "count": 2 },
                { "size": "M", "count": 1 }
            ])
        );
    }
}
