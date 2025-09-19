use std::collections::BTreeMap;

use anyhow::Context as _;
use async_graphql::{Context, Enum, InputObject, Object, Result, SimpleObject};
use jiff::{SpanTotal, Unit};
use num_traits::ToPrimitive;

use crate::{
    api::{issue::Issue, DateTimeUtc, TODO_LIST_PROJECT_TITLE},
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
#[derive(Enum, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Debug)]
enum IssuePriority {
    P0,
    P1,
    P2,
    None,
}

impl From<&str> for IssuePriority {
    fn from(s: &str) -> Self {
        match s {
            "P0" => Self::P0,
            "P1" => Self::P1,
            "P2" => Self::P2,
            _ => Self::None,
        }
    }
}

#[derive(SimpleObject)]
struct IssueSizeCount {
    size: IssueSize,
    count: usize,
}
#[derive(SimpleObject, Debug)]
struct IssuePriorityCount {
    priority: IssuePriority,
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

    /// The average resolution time in days for resolved issues.
    avg_resolution_days: Option<f64>,

    /// The distribution of priorities for resolved issues.
    resolved_issue_priority_distribution: Vec<IssuePriorityCount>,
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
                    .find(|item| item.project_title == TODO_LIST_PROJECT_TITLE)
                    .and_then(|item| item.todo_size.as_deref())
                    .unwrap_or("None");
                *acc.entry(size_str.into()).or_insert(0) += 1;
                acc
            })
            .into_iter()
            .map(|(size, count)| IssueSizeCount { size, count })
            .collect();

        let resolution_days: Vec<f64> = resolved_issues
            .iter()
            .filter_map(|issue| {
                let closed_at = issue.closed_at?;
                let created_at = issue.created_at;

                let project_item = issue
                    .project_items
                    .nodes
                    .iter()
                    .find(|p| p.project_title == TODO_LIST_PROJECT_TITLE)?;

                let pending_days = project_item.todo_pending_days.unwrap_or(0.0);

                let span = created_at.0.until(closed_at.0).ok()?;
                let resolution_days = span
                    .total(SpanTotal::from(Unit::Day).days_are_24_hours())
                    .ok()?;
                let result_days = resolution_days - pending_days;

                Some(f64::max(result_days, 0.0))
            })
            .collect();

        let avg_resolution_days = if resolution_days.is_empty() {
            None
        } else {
            Some(
                resolution_days.iter().sum::<f64>()
                    / resolution_days
                        .len()
                        .to_f64()
                        .context("Failed to convert usize to f64")?,
            )
        };

        let resolved_issue_priority_distribution = resolved_issues
            .iter()
            .fold(BTreeMap::new(), |mut acc, issue| {
                let priority_str = issue
                    .project_items
                    .nodes
                    .iter()
                    .find(|p| p.project_title == super::TODO_LIST_PROJECT_TITLE)
                    .and_then(|p| p.todo_priority.as_deref())
                    .unwrap_or("None");

                *acc.entry(priority_str.into()).or_insert(0) += 1;
                acc
            })
            .into_iter()
            .map(|(priority, count)| IssuePriorityCount { priority, count })
            .collect();

        Ok(IssueStat {
            open_issue_count,
            resolved_issue_count,
            resolved_issue_size_distribution,
            avg_resolution_days,
            resolved_issue_priority_distribution,
        })
    }
}

#[cfg(test)]
mod tests {
    use jiff::Timestamp;

    use crate::{
        api::{TestSchema, TODO_LIST_PROJECT_TITLE, TODO_LIST_STATUS_DONE},
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

    #[tokio::test]
    async fn avg_resolution_days() {
        let schema = TestSchema::new();
        let owner = "aicers";
        let repo = "github-dashboard-server";
        let mut resolved_issues = create_resolved_issues(1..=2);

        // Issue 1: 10 days resolution, 2 pending days. Net: 8 days.
        resolved_issues[0].created_at = parse("2025-01-01T00:00:00Z");
        resolved_issues[0].closed_at = Some(parse("2025-01-11T00:00:00Z"));
        resolved_issues[0].project_items.nodes[0].todo_pending_days = Some(2.0);

        // Issue 2: 5 days resolution, 1 pending day. Net: 4 days.
        resolved_issues[1].created_at = parse("2025-01-01T00:00:00Z");
        resolved_issues[1].closed_at = Some(parse("2025-01-06T00:00:00Z"));
        resolved_issues[1].project_items.nodes[0].todo_pending_days = Some(1.0);

        schema
            .db
            .insert_issues(resolved_issues, owner, repo)
            .unwrap();

        let query = r"
        {
            issueStat(filter: {}) {
            avgResolutionDays
            }
        }";
        let data = schema.execute(query).await.data.into_json().unwrap();
        // Average of 8 and 4 is 6.
        assert_eq!(data["issueStat"]["avgResolutionDays"], 6.0);
    }

    #[tokio::test]
    async fn avg_resolution_days_is_not_negative() {
        let schema = TestSchema::new();
        let owner = "aicers";
        let repo = "github-dashboard-server";
        let mut resolved_issues = create_resolved_issues(1..=1);

        // Create an issue where the pending days (10 days) exceed the resolution days (5 days).
        // In this case, the net resolution days should be 0, not negative.
        // resolution_days (5.0) - pending_days (10.0) = -5.0
        resolved_issues[0].created_at = parse("2025-01-01T00:00:00Z");
        resolved_issues[0].closed_at = Some(parse("2025-01-06T00:00:00Z"));
        resolved_issues[0].project_items.nodes[0].todo_pending_days = Some(10.0);

        schema
            .db
            .insert_issues(resolved_issues, owner, repo)
            .unwrap();

        let query = r"
        {
            issueStat(filter: {}) {
                avgResolutionDays
            }
        }";
        let data = schema.execute(query).await.data.into_json().unwrap();

        // The average resolution days should be 0.0, not negative.
        assert_eq!(data["issueStat"]["avgResolutionDays"], 0.0);
    }

    #[tokio::test]
    async fn resolved_issue_priority_distribution() {
        let schema = TestSchema::new();
        let owner: &str = "aicers";
        let repo = "github-dashboard-server";

        let mut resolved_issues = create_resolved_issues(1..=10);
        // P0: 2
        resolved_issues[0].project_items.nodes[0].todo_priority = Some("P0".to_string());
        resolved_issues[1].project_items.nodes[0].todo_priority = Some("P0".to_string());
        // P1: 3
        resolved_issues[2].project_items.nodes[0].todo_priority = Some("P1".to_string());
        resolved_issues[3].project_items.nodes[0].todo_priority = Some("P1".to_string());
        resolved_issues[4].project_items.nodes[0].todo_priority = Some("P1".to_string());
        // P2: 1
        resolved_issues[5].project_items.nodes[0].todo_priority = Some("P2".to_string());
        // None: 4

        schema
            .db
            .insert_issues(resolved_issues, owner, repo)
            .unwrap();

        let query = r"
        {
            issueStat(filter: {}) {
                resolvedIssuePriorityDistribution {
                    priority
                    count
                }
            }
        }";
        let data = schema.execute(query).await.data.into_json().unwrap();
        let dist = &data["issueStat"]["resolvedIssuePriorityDistribution"];

        assert_eq!(dist.as_array().unwrap().len(), 4);
        assert_eq!(dist[0]["priority"], "P0");
        assert_eq!(dist[0]["count"], 2);
        assert_eq!(dist[1]["priority"], "P1");
        assert_eq!(dist[1]["count"], 3);
        assert_eq!(dist[2]["priority"], "P2");
        assert_eq!(dist[2]["count"], 1);
        assert_eq!(dist[3]["priority"], "NONE");
        assert_eq!(dist[3]["count"], 4);
    }
}
