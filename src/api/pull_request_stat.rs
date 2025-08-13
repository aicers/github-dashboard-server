use async_graphql::{Context, InputObject, Object, Result, SimpleObject};

use crate::{
    api::{pull_request::PullRequest, DateTimeUtc},
    database::Iter,
    outbound::pull_requests::PullRequestState,
    Database,
};

#[derive(InputObject, Debug)]
pub(crate) struct PullRequestStatFilter {
    /// Filter by pull request author.
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

impl PullRequestStatFilter {
    fn filter_pull_requests(&self, prs: Iter<PullRequest>) -> Vec<PullRequest> {
        prs.into_iter()
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
            })
            .collect()
    }
}

#[derive(Default)]
pub(super) struct PullRequestStatQuery {}

#[derive(SimpleObject)]
struct PullRequestStat {
    /// The number of open pull requests.
    open_pr_count: i32,
}

#[Object]
impl PullRequestStatQuery {
    #[allow(clippy::unused_async)]
    async fn pull_request_stat(
        &self,
        ctx: &Context<'_>,
        filter: PullRequestStatFilter,
    ) -> Result<PullRequestStat> {
        let db = ctx.data::<Database>()?;
        let prs = db.pull_requests(None, None);
        let filtered = filter.filter_pull_requests(prs);
        let open_pr_count = filtered
            .iter()
            .filter(|pr| matches!(pr.state, PullRequestState::OPEN))
            .count()
            .try_into()?;

        Ok(PullRequestStat { open_pr_count })
    }
}

#[cfg(test)]
mod tests {
    use jiff::Timestamp;

    use crate::{api::TestSchema, outbound::GitHubPullRequestNode};

    fn create_pull_requests_for_repo(
        n: usize,
        owner: &str,
        repo: &str,
    ) -> Vec<GitHubPullRequestNode> {
        (0..n)
            .map(|i| GitHubPullRequestNode {
                number: i.try_into().unwrap(),
                repository: crate::outbound::RepositoryNode {
                    owner: owner.to_string(),
                    name: repo.to_string(),
                },
                ..Default::default()
            })
            .collect()
    }

    fn create_pull_requests(n: usize) -> Vec<GitHubPullRequestNode> {
        create_pull_requests_for_repo(n, "aicers", "github-dashboard-server")
    }

    fn parse(date: &str) -> Timestamp {
        date.parse().unwrap()
    }

    #[tokio::test]
    async fn open_pr_count_by_author() {
        let schema = TestSchema::new();
        let mut prs = create_pull_requests(3);
        prs[0].author = "foo".to_string();
        schema
            .db
            .insert_pull_requests(prs, "aicers", "github-dashboard-server")
            .unwrap();

        let query = r#"
        {
            pullRequestStat(filter: {author: "foo"}) {
                openPrCount
            }
        }"#;
        let data = schema.execute(query).await.data.into_json().unwrap();
        assert_eq!(data["pullRequestStat"]["openPrCount"], 1);
    }

    #[tokio::test]
    async fn open_pr_count_by_begin_end() {
        let schema = TestSchema::new();
        let mut prs = create_pull_requests(3);
        prs[1].created_at = parse("2025-01-05T00:00:00Z");
        prs[2].created_at = parse("2025-01-06T00:00:00Z");

        schema
            .db
            .insert_pull_requests(prs, "aicers", "github-dashboard-server")
            .unwrap();

        let query = r#"
        {
            pullRequestStat(filter: {begin: "2025-01-06T00:00:00Z"}) {
                openPrCount
            }
        }"#;
        let data = schema.execute(query).await.data.into_json().unwrap();
        assert_eq!(data["pullRequestStat"]["openPrCount"], 1);

        let query = r#"
        {
            pullRequestStat(filter: {begin: "2025-01-05T00:00:00Z", end: "2025-01-06T00:00:00Z"}) {
                openPrCount
            }
        }"#;
        let data = schema.execute(query).await.data.into_json().unwrap();

        assert_eq!(data["pullRequestStat"]["openPrCount"], 1);
    }

    #[tokio::test]
    async fn open_pr_count_by_author_and_dates() {
        let schema = TestSchema::new();
        let mut prs = create_pull_requests(3);
        prs[1].author = "foo".to_string();
        prs[1].created_at = parse("2025-01-05T00:00:00Z");
        prs[2].created_at = parse("2025-01-06T00:00:00Z");

        schema
            .db
            .insert_pull_requests(prs, "aicers", "github-dashboard-server")
            .unwrap();
        let query = r#"
        {
            pullRequestStat(filter: {author: "foo", begin: "2025-01-05T00:00:00Z", end: "2025-01-06T00:00:00Z"}) {
                openPrCount
            }
        }"#;
        let data = schema.execute(query).await.data.into_json().unwrap();

        assert_eq!(data["pullRequestStat"]["openPrCount"], 1);
    }

    #[tokio::test]
    async fn open_pr_count_by_repo() {
        let schema = TestSchema::new();
        let server_prs = create_pull_requests_for_repo(2, "aicers", "github-dashboard-server");
        let client_prs = create_pull_requests_for_repo(1, "aicers", "github-dashboard-client");

        schema
            .db
            .insert_pull_requests(server_prs, "aicers", "github-dashboard-server")
            .unwrap();
        schema
            .db
            .insert_pull_requests(client_prs, "aicers", "github-dashboard-client")
            .unwrap();
        let query = r#"
        {
            pullRequestStat(filter: {repo: "github-dashboard-client"}) {
                openPrCount
            }
        }"#;
        let data = schema.execute(query).await.data.into_json().unwrap();

        assert_eq!(data["pullRequestStat"]["openPrCount"], 1);
    }

    #[tokio::test]
    async fn open_pr_count_by_repo_and_author() {
        let schema = TestSchema::new();
        let server_prs = create_pull_requests_for_repo(1, "aicers", "github-dashboard-server");
        let mut client_prs = create_pull_requests_for_repo(2, "aicers", "github-dashboard-client");
        client_prs[1].author = "foo".to_string();
        schema
            .db
            .insert_pull_requests(server_prs, "aicers", "github-dashboard-server")
            .unwrap();
        schema
            .db
            .insert_pull_requests(client_prs, "aicers", "github-dashboard-client")
            .unwrap();

        let query = r#"
        {
            pullRequestStat(filter: {repo: "github-dashboard-client", author: "foo"}) {
                openPrCount
            }
        }"#;
        let data = schema.execute(query).await.data.into_json().unwrap();
        assert_eq!(data["pullRequestStat"]["openPrCount"], 1);
    }

    #[tokio::test]
    async fn open_pr_count_with_closed_prs() {
        let schema = TestSchema::new();
        let mut prs = create_pull_requests(3);
        prs[1].state = crate::outbound::PRPullRequestState::CLOSED;
        prs[2].state = crate::outbound::PRPullRequestState::MERGED;

        schema
            .db
            .insert_pull_requests(prs, "aicers", "github-dashboard-server")
            .unwrap();

        let query = r"
        {
            pullRequestStat(filter: {}) {
                openPrCount
            }
        }";
        let data = schema.execute(query).await.data.into_json().unwrap();
        assert_eq!(data["pullRequestStat"]["openPrCount"], 1);
    }

    #[tokio::test]
    async fn open_pr_count_no_matches() {
        let schema = TestSchema::new();
        let prs = create_pull_requests(2);

        schema
            .db
            .insert_pull_requests(prs, "aicers", "github-dashboard-server")
            .unwrap();

        let query = "
        {
            pullRequestStat(filter: {author: \"nonexistent\"}) {
                openPrCount
            }
        }";
        let data = schema.execute(query).await.data.into_json().unwrap();
        assert_eq!(data["pullRequestStat"]["openPrCount"], 0);
    }

    #[tokio::test]
    async fn open_pr_count_empty_database() {
        let schema = TestSchema::new();

        let query = "
        {
            pullRequestStat(filter: {}) {
                openPrCount
            }
        }";
        let data = schema.execute(query).await.data.into_json().unwrap();
        assert_eq!(data["pullRequestStat"]["openPrCount"], 0);
    }
}
