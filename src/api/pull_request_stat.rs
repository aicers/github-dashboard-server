use anyhow::Context as AnyhowContext;
use async_graphql::{Context, InputObject, Object, Result, SimpleObject};
use num_traits::ToPrimitive;

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

/// `allow(clippy::struct_field_names)`: This warning is triggered by `_count` suffix in field
/// names. In #222, an `avgMergeDays` field will be added to this struct, allowing us to remove this
/// lint allowance.
#[allow(clippy::struct_field_names)]
#[derive(SimpleObject)]
struct PullRequestStat {
    /// The number of open pull requests.
    open_pr_count: i32,
    /// The number of merged pull requests.
    merged_pr_count: i32,
    /// The average number of reviews and comments per merged pull request.
    ///
    /// This field is `None` if there are no merged pull requests.
    avg_review_comment_count: Option<f64>,
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

        let merged_prs: Vec<&PullRequest> = filtered
            .iter()
            .filter(|pr| matches!(pr.state, PullRequestState::MERGED))
            .collect();
        let merged_pr_count = merged_prs.len().try_into()?;
        // Calculate average reviews and comments for merged pull requests
        let avg_review_comment_count = if merged_prs.is_empty() {
            None
        } else {
            let total_reviews_and_comments: f64 = merged_prs
                .iter()
                .map(|pr| pr.comments_count + pr.reviews_count)
                .sum::<i32>()
                .into();

            Some(
                total_reviews_and_comments
                    / merged_prs
                        .len()
                        .to_f64()
                        .context("Failed to convert usize to f64")?,
            )
        };

        Ok(PullRequestStat {
            open_pr_count,
            merged_pr_count,
            avg_review_comment_count,
        })
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
    async fn pr_count_by_author() {
        let schema = TestSchema::new();
        let mut prs = create_pull_requests(4);
        prs[0].author = "foo".to_string();
        prs[1].author = "foo".to_string();
        prs[1].state = crate::outbound::PRPullRequestState::MERGED;
        prs[2].state = crate::outbound::PRPullRequestState::MERGED;
        prs[3].state = crate::outbound::PRPullRequestState::CLOSED;

        schema
            .db
            .insert_pull_requests(prs, "aicers", "github-dashboard-server")
            .unwrap();

        let query = r#"
        {
            pullRequestStat(filter: {author: "foo"}) {
                openPrCount
                mergedPrCount
            }
        }"#;
        let data = schema.execute(query).await.data.into_json().unwrap();
        assert_eq!(data["pullRequestStat"]["openPrCount"], 1);
        assert_eq!(data["pullRequestStat"]["mergedPrCount"], 1);
    }

    #[tokio::test]
    async fn pr_count_by_begin_end() {
        let schema = TestSchema::new();
        let mut prs = create_pull_requests(4);
        prs[1].created_at = parse("2025-01-05T00:00:00Z");
        prs[1].state = crate::outbound::PRPullRequestState::MERGED;
        prs[2].created_at = parse("2025-01-06T00:00:00Z");
        prs[3].created_at = parse("2025-01-06T00:00:00Z");
        prs[3].state = crate::outbound::PRPullRequestState::MERGED;

        schema
            .db
            .insert_pull_requests(prs, "aicers", "github-dashboard-server")
            .unwrap();

        let query = r#"
        {
            pullRequestStat(filter: {begin: "2025-01-06T00:00:00Z"}) {
                openPrCount
                mergedPrCount
            }
        }"#;
        let data = schema.execute(query).await.data.into_json().unwrap();
        assert_eq!(data["pullRequestStat"]["openPrCount"], 1);
        assert_eq!(data["pullRequestStat"]["mergedPrCount"], 1);

        let query = r#"
        {
            pullRequestStat(filter: {begin: "2025-01-05T00:00:00Z", end: "2025-01-06T00:00:00Z"}) {
                openPrCount
                mergedPrCount
            }
        }"#;
        let data = schema.execute(query).await.data.into_json().unwrap();
        assert_eq!(data["pullRequestStat"]["openPrCount"], 0);
        assert_eq!(data["pullRequestStat"]["mergedPrCount"], 1);
    }

    #[tokio::test]
    async fn pr_count_by_author_and_dates() {
        let schema = TestSchema::new();
        let mut prs = create_pull_requests(4);
        prs[1].author = "foo".to_string();
        prs[1].created_at = parse("2025-01-05T00:00:00Z");
        prs[2].created_at = parse("2025-01-06T00:00:00Z");
        prs[3].author = "foo".to_string();
        prs[3].created_at = parse("2025-01-05T00:00:00Z");
        prs[3].state = crate::outbound::PRPullRequestState::MERGED;

        schema
            .db
            .insert_pull_requests(prs, "aicers", "github-dashboard-server")
            .unwrap();

        let query = r#"
        {
            pullRequestStat(filter: {author: "foo", begin: "2025-01-05T00:00:00Z", end: "2025-01-06T00:00:00Z"}) {
                openPrCount
                mergedPrCount
            }
        }"#;
        let data = schema.execute(query).await.data.into_json().unwrap();
        assert_eq!(data["pullRequestStat"]["openPrCount"], 1);
        assert_eq!(data["pullRequestStat"]["mergedPrCount"], 1);
    }

    #[tokio::test]
    async fn pr_count_by_repo() {
        let schema = TestSchema::new();
        let mut server_prs = create_pull_requests_for_repo(2, "aicers", "github-dashboard-server");
        let mut client_prs = create_pull_requests_for_repo(3, "aicers", "github-dashboard-client");

        server_prs[0].state = crate::outbound::PRPullRequestState::MERGED;
        client_prs[1].state = crate::outbound::PRPullRequestState::MERGED;
        client_prs[2].state = crate::outbound::PRPullRequestState::CLOSED;

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
                mergedPrCount
            }
        }"#;
        let data = schema.execute(query).await.data.into_json().unwrap();
        assert_eq!(data["pullRequestStat"]["openPrCount"], 1);
        assert_eq!(data["pullRequestStat"]["mergedPrCount"], 1);
    }

    #[tokio::test]
    async fn pr_count_by_repo_and_author() {
        let schema = TestSchema::new();
        let mut server_prs = create_pull_requests_for_repo(1, "aicers", "github-dashboard-server");
        let mut client_prs = create_pull_requests_for_repo(3, "aicers", "github-dashboard-client");

        server_prs[0].state = crate::outbound::PRPullRequestState::MERGED;
        client_prs[1].author = "foo".to_string();
        client_prs[2].author = "foo".to_string();
        client_prs[2].state = crate::outbound::PRPullRequestState::MERGED;

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
                mergedPrCount
            }
        }"#;
        let data = schema.execute(query).await.data.into_json().unwrap();
        assert_eq!(data["pullRequestStat"]["openPrCount"], 1);
        assert_eq!(data["pullRequestStat"]["mergedPrCount"], 1);
    }

    #[tokio::test]
    async fn pr_count_with_different_states() {
        let schema = TestSchema::new();
        let mut prs = create_pull_requests(4);
        prs[1].state = crate::outbound::PRPullRequestState::CLOSED;
        prs[2].state = crate::outbound::PRPullRequestState::MERGED;
        prs[3].state = crate::outbound::PRPullRequestState::MERGED;

        schema
            .db
            .insert_pull_requests(prs, "aicers", "github-dashboard-server")
            .unwrap();

        let query = r"
        {
            pullRequestStat(filter: {}) {
                openPrCount
                mergedPrCount
            }
        }";
        let data = schema.execute(query).await.data.into_json().unwrap();
        assert_eq!(data["pullRequestStat"]["openPrCount"], 1);
        assert_eq!(data["pullRequestStat"]["mergedPrCount"], 2);
    }

    #[tokio::test]
    async fn pr_count_no_matches() {
        let schema = TestSchema::new();
        let mut prs = create_pull_requests(2);
        prs[0].state = crate::outbound::PRPullRequestState::MERGED;
        prs[1].state = crate::outbound::PRPullRequestState::MERGED;

        schema
            .db
            .insert_pull_requests(prs, "aicers", "github-dashboard-server")
            .unwrap();

        let query = "
        {
            pullRequestStat(filter: {author: \"nonexistent\"}) {
                openPrCount
                mergedPrCount
            }
        }";
        let data = schema.execute(query).await.data.into_json().unwrap();
        assert_eq!(data["pullRequestStat"]["openPrCount"], 0);
        assert_eq!(data["pullRequestStat"]["mergedPrCount"], 0);
    }

    #[tokio::test]
    async fn pr_count_empty_database() {
        let schema = TestSchema::new();

        let query = "
        {
            pullRequestStat(filter: {}) {
                openPrCount
                mergedPrCount
            }
        }";
        let data = schema.execute(query).await.data.into_json().unwrap();
        assert_eq!(data["pullRequestStat"]["openPrCount"], 0);
        assert_eq!(data["pullRequestStat"]["mergedPrCount"], 0);
    }

    #[tokio::test]
    async fn avg_review_comment_count_no_merged_prs() {
        let schema = TestSchema::new();
        let prs = create_pull_requests(2); // All PRs are OPEN by default

        schema
            .db
            .insert_pull_requests(prs, "aicers", "github-dashboard-server")
            .unwrap();

        let query = "
        {
            pullRequestStat(filter: {}) {
                avgReviewCommentCount
            }
        }";
        let data = schema.execute(query).await.data.into_json().unwrap();
        assert_eq!(
            data["pullRequestStat"]["avgReviewCommentCount"],
            serde_json::Value::Null
        );
    }

    #[tokio::test]
    async fn avg_review_comment_count_single_merged_pr() {
        let schema = TestSchema::new();
        let mut prs = create_pull_requests(1);
        prs[0].state = crate::outbound::PRPullRequestState::MERGED;
        prs[0].comments.total_count = 3;
        prs[0].reviews.total_count = 2;

        schema
            .db
            .insert_pull_requests(prs, "aicers", "github-dashboard-server")
            .unwrap();

        let query = "
        {
            pullRequestStat(filter: {}) {
                avgReviewCommentCount
            }
        }";
        let data = schema.execute(query).await.data.into_json().unwrap();
        assert_eq!(data["pullRequestStat"]["avgReviewCommentCount"], 5.0);
    }

    #[tokio::test]
    async fn avg_review_comment_count_multiple_merged_prs() {
        let schema = TestSchema::new();
        let mut prs = create_pull_requests(3);

        // First merged PR: 4 comments + 1 review = 5 total
        prs[0].state = crate::outbound::PRPullRequestState::MERGED;
        prs[0].comments.total_count = 4;
        prs[0].reviews.total_count = 1;

        // Second merged PR: 2 comments + 3 reviews = 5 total
        prs[1].state = crate::outbound::PRPullRequestState::MERGED;
        prs[1].comments.total_count = 2;
        prs[1].reviews.total_count = 3;

        // Third PR is OPEN and should not affect the calculation
        prs[2].comments.total_count = 10;
        prs[2].reviews.total_count = 10;

        schema
            .db
            .insert_pull_requests(prs, "aicers", "github-dashboard-server")
            .unwrap();

        let query = "
        {
            pullRequestStat(filter: {}) {
                avgReviewCommentCount
            }
        }";
        let data = schema.execute(query).await.data.into_json().unwrap();
        // Average: (5 + 5) / 2 = 5.0
        assert_eq!(data["pullRequestStat"]["avgReviewCommentCount"], 5.0);
    }

    #[tokio::test]
    async fn avg_review_comment_count_with_zero_comments_and_reviews() {
        let schema = TestSchema::new();
        let mut prs = create_pull_requests(2);

        // Both PRs are merged but have no comments or reviews
        prs[0].state = crate::outbound::PRPullRequestState::MERGED;
        prs[0].comments.total_count = 0;
        prs[0].reviews.total_count = 0;

        prs[1].state = crate::outbound::PRPullRequestState::MERGED;
        prs[1].comments.total_count = 0;
        prs[1].reviews.total_count = 0;

        schema
            .db
            .insert_pull_requests(prs, "aicers", "github-dashboard-server")
            .unwrap();

        let query = "
        {
            pullRequestStat(filter: {}) {
                avgReviewCommentCount
            }
        }";
        let data = schema.execute(query).await.data.into_json().unwrap();
        // Average: (0 + 0) / 2 = 0.0
        assert_eq!(data["pullRequestStat"]["avgReviewCommentCount"], 0.0);
    }

    #[tokio::test]
    async fn avg_review_comment_count_mixed_states() {
        let schema = TestSchema::new();
        let mut prs = create_pull_requests(4);

        // MERGED PR: 6 comments + 2 reviews = 8 total
        prs[0].state = crate::outbound::PRPullRequestState::MERGED;
        prs[0].comments.total_count = 6;
        prs[0].reviews.total_count = 2;

        // CLOSED PR: should not be included
        prs[1].state = crate::outbound::PRPullRequestState::CLOSED;
        prs[1].comments.total_count = 5;
        prs[1].reviews.total_count = 5;

        // MERGED PR: 1 comment + 1 review = 2 total
        prs[2].state = crate::outbound::PRPullRequestState::MERGED;
        prs[2].comments.total_count = 1;
        prs[2].reviews.total_count = 1;

        // OPEN PR: should not be included
        prs[3].comments.total_count = 100;
        prs[3].reviews.total_count = 100;

        schema
            .db
            .insert_pull_requests(prs, "aicers", "github-dashboard-server")
            .unwrap();

        let query = "
        {
            pullRequestStat(filter: {}) {
                avgReviewCommentCount
            }
        }";
        let data = schema.execute(query).await.data.into_json().unwrap();
        // Average: (8 + 2) / 2 = 5.0
        assert_eq!(data["pullRequestStat"]["avgReviewCommentCount"], 5.0);
    }

    #[tokio::test]
    async fn avg_review_comment_count_with_filters() {
        let schema = TestSchema::new();
        let mut prs = create_pull_requests(3);

        // MERGED PR by author "alice": 3 comments + 3 reviews = 6 total
        prs[0].state = crate::outbound::PRPullRequestState::MERGED;
        prs[0].author = "alice".to_string();
        prs[0].comments.total_count = 3;
        prs[0].reviews.total_count = 3;

        // MERGED PR by author "bob": 4 comments + 2 reviews = 6 total
        prs[1].state = crate::outbound::PRPullRequestState::MERGED;
        prs[1].author = "bob".to_string();
        prs[1].comments.total_count = 4;
        prs[1].reviews.total_count = 2;

        // MERGED PR by author "alice": 2 comments + 4 reviews = 6 total
        prs[2].state = crate::outbound::PRPullRequestState::MERGED;
        prs[2].author = "alice".to_string();
        prs[2].comments.total_count = 2;
        prs[2].reviews.total_count = 4;

        schema
            .db
            .insert_pull_requests(prs, "aicers", "github-dashboard-server")
            .unwrap();

        // Test filtering by author "alice"
        let query = r#"
        {
            pullRequestStat(filter: {author: "alice"}) {
                avgReviewCommentCount
            }
        }"#;
        let data = schema.execute(query).await.data.into_json().unwrap();
        // Average for alice: (6 + 6) / 2 = 6.0
        assert_eq!(data["pullRequestStat"]["avgReviewCommentCount"], 6.0);

        // Test filtering by author "bob"
        let query = r#"
        {
            pullRequestStat(filter: {author: "bob"}) {
                avgReviewCommentCount
            }
        }"#;
        let data = schema.execute(query).await.data.into_json().unwrap();
        // Average for bob: 6 / 1 = 6.0
        assert_eq!(data["pullRequestStat"]["avgReviewCommentCount"], 6.0);
    }

    #[tokio::test]
    async fn avg_review_comment_count_precision() {
        let schema = TestSchema::new();
        let mut prs = create_pull_requests(3);

        // Three merged PRs with different comment/review counts to test precision
        prs[0].state = crate::outbound::PRPullRequestState::MERGED;
        prs[0].comments.total_count = 1;
        prs[0].reviews.total_count = 0; // Total: 1

        prs[1].state = crate::outbound::PRPullRequestState::MERGED;
        prs[1].comments.total_count = 0;
        prs[1].reviews.total_count = 1; // Total: 1

        prs[2].state = crate::outbound::PRPullRequestState::MERGED;
        prs[2].comments.total_count = 1;
        prs[2].reviews.total_count = 1; // Total: 2

        schema
            .db
            .insert_pull_requests(prs, "aicers", "github-dashboard-server")
            .unwrap();

        let query = "
        {
            pullRequestStat(filter: {}) {
                avgReviewCommentCount
            }
        }";
        let data = schema.execute(query).await.data.into_json().unwrap();
        // Average: (1 + 1 + 2) / 3 = 4/3 ≈ 1.3333333333333333
        let expected = 4.0 / 3.0;
        assert_eq!(data["pullRequestStat"]["avgReviewCommentCount"], expected);
    }

    #[tokio::test]
    async fn avg_review_comment_count_combined_with_open_count() {
        let schema = TestSchema::new();
        let mut prs = create_pull_requests(4);

        // OPEN PR
        prs[0].comments.total_count = 1;
        prs[0].reviews.total_count = 1;

        // MERGED PR: 2 comments + 1 review = 3 total
        prs[1].state = crate::outbound::PRPullRequestState::MERGED;
        prs[1].comments.total_count = 2;
        prs[1].reviews.total_count = 1;

        // OPEN PR
        prs[2].comments.total_count = 5;
        prs[2].reviews.total_count = 5;

        // MERGED PR: 4 comments + 3 reviews = 7 total
        prs[3].state = crate::outbound::PRPullRequestState::MERGED;
        prs[3].comments.total_count = 4;
        prs[3].reviews.total_count = 3;

        schema
            .db
            .insert_pull_requests(prs, "aicers", "github-dashboard-server")
            .unwrap();

        let query = "
        {
            pullRequestStat(filter: {}) {
                openPrCount
                avgReviewCommentCount
            }
        }";
        let data = schema.execute(query).await.data.into_json().unwrap();
        assert_eq!(data["pullRequestStat"]["openPrCount"], 2);
        // Average: (3 + 7) / 2 = 5.0
        assert_eq!(data["pullRequestStat"]["avgReviewCommentCount"], 5.0);
    }
}
