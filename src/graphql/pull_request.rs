use std::fmt;

use anyhow::Context as AnyhowContext;
use async_graphql::{
    connection::{query, Connection, EmptyFields},
    scalar, Context, Object, Result, SimpleObject, ID,
};
use jiff::Timestamp;

use crate::graphql::DateTimeUtc;
use crate::{
    database::{self, Database, TryFromKeyValue},
    github::{
        GitHubCommentConnection, GitHubCommitConnection, GitHubPullRequestNode,
        GitHubReviewConnection, PRPullRequestState as PullRequestState, PullRequestReviewState,
        RepositoryNode,
    },
};
scalar!(PullRequestState);
scalar!(PullRequestReviewState);

#[derive(SimpleObject, Debug)]
pub(crate) struct PullRequestComment {
    pub(crate) body: String,
    pub(crate) created_at: DateTimeUtc,
    pub(crate) updated_at: DateTimeUtc,
    pub(crate) author: String,
}

#[derive(SimpleObject)]
pub(crate) struct Review {
    pub(crate) author: String,
    pub(crate) state: PullRequestReviewState,
    pub(crate) body: Option<String>,
    pub(crate) url: String,
    pub(crate) created_at: DateTimeUtc,
    pub(crate) published_at: DateTimeUtc,
    pub(crate) submitted_at: DateTimeUtc,
    pub(crate) is_minimized: bool,
    pub(crate) comments: Vec<PullRequestComment>,
}

#[derive(SimpleObject)]
pub(crate) struct CommitInfo {
    pub(crate) additions: i32,
    pub(crate) deletions: i32,
    pub(crate) message: String,
    pub(crate) message_body: Option<String>,
    pub(crate) author: String,
    pub(crate) changed_files_if_available: Option<i32>,
    pub(crate) committed_date: DateTimeUtc,
    pub(crate) committer: String,
}

#[derive(SimpleObject)]
pub(crate) struct PullRequest {
    pub(crate) id: ID,
    pub(crate) number: i32,
    pub(crate) title: String,
    pub(crate) body: Option<String>,
    pub(crate) state: PullRequestState,
    pub(crate) assignees: Vec<String>,
    pub(crate) created_at: DateTimeUtc,
    pub(crate) updated_at: DateTimeUtc,
    pub(crate) closed_at: Option<DateTimeUtc>,
    pub(crate) merged_at: Option<DateTimeUtc>,
    pub(crate) author: Option<String>,
    pub(crate) additions: i32,
    pub(crate) deletions: i32,
    pub(crate) url: String,
    pub(crate) repository_owner: String,
    pub(crate) repository_name: String,
    pub(crate) labels: Vec<String>,
    pub(crate) comments_count: i32,
    pub(crate) comments: Vec<PullRequestComment>,
    pub(crate) review_decision: Option<PullRequestReviewState>,
    pub(crate) review_requests: Vec<String>,
    pub(crate) reviews_count: i32,
    pub(crate) reviews: Vec<Review>,
    pub(crate) commits_count: i32,
    pub(crate) commits: Vec<CommitInfo>,
}

impl TryFromKeyValue for PullRequest {
    #[allow(clippy::too_many_lines)]
    fn try_from_key_value(key: &[u8], value: &[u8]) -> anyhow::Result<Self> {
        let _number = database::parse_key(key)
            .with_context(|| format!("invalid key in database: {key:02x?}"))?;
        let gh: GitHubPullRequestNode = bincode::deserialize(value)
            .with_context(|| format!("Deserialization failed for key: {key:?}"))?;
        let labels = gh.labels;
        let comments = gh
            .comments
            .nodes
            .into_iter()
            .map(|c| PullRequestComment {
                body: c.body,
                created_at: DateTimeUtc(c.created_at),
                updated_at: DateTimeUtc(c.updated_at),
                author: c.author.clone(),
            })
            .collect();
        let reviews = gh
            .reviews
            .nodes
            .into_iter()
            .map(|r| Review {
                author: r.author.clone(),
                state: r.state,
                body: r.body,
                url: r.url,
                created_at: DateTimeUtc(r.created_at),
                published_at: DateTimeUtc(r.published_at.unwrap_or_default()),
                submitted_at: DateTimeUtc(r.submitted_at),
                is_minimized: r.is_minimized,
                comments: r
                    .comments
                    .nodes
                    .into_iter()
                    .map(|c| PullRequestComment {
                        body: c.body,
                        created_at: DateTimeUtc(c.created_at),
                        updated_at: DateTimeUtc(c.updated_at),
                        author: c.author.clone(),
                    })
                    .collect(),
            })
            .collect();
        let commits = gh
            .commits
            .nodes
            .into_iter()
            .map(|c| CommitInfo {
                additions: c.additions,
                deletions: c.deletions,
                message: c.message,
                message_body: c.message_body,
                author: c.author.clone(),
                changed_files_if_available: c.changed_files_if_available,
                committed_date: DateTimeUtc(c.committed_date),
                committer: c.committer.clone(),
            })
            .collect();

        Ok(PullRequest {
            id: gh.id.into(),
            #[allow(clippy::cast_possible_truncation)]
            number: gh.number as i32,
            title: gh.title,
            body: gh.body,
            state: gh.state,
            created_at: DateTimeUtc(gh.created_at),
            updated_at: DateTimeUtc(gh.updated_at),
            closed_at: gh.closed_at.map(DateTimeUtc),
            merged_at: gh.merged_at.map(DateTimeUtc),
            author: Some(gh.author),
            additions: gh.additions,
            deletions: gh.deletions,
            url: gh.url,
            repository_owner: gh.repository.owner.clone(),
            repository_name: gh.repository.name,
            labels,
            comments_count: gh.comments.total_count,
            comments,
            review_decision: gh.review_decision,
            assignees: gh.assignees,
            review_requests: gh.review_requests,
            reviews_count: gh.reviews.total_count,
            reviews,
            commits_count: gh.commits.total_count,
            commits,
        })
    }
}

impl fmt::Display for PullRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.number)
    }
}

impl Default for GitHubPullRequestNode {
    fn default() -> Self {
        Self {
            id: String::new(),
            number: 0,
            title: String::new(),
            body: None,
            state: PullRequestState::OPEN,
            created_at: Timestamp::now(),
            updated_at: Timestamp::now(),
            closed_at: None,
            merged_at: None,
            author: String::new(),
            additions: 0,
            deletions: 0,
            url: String::new(),
            repository: RepositoryNode {
                owner: String::new(),
                name: String::new(),
            },

            labels: vec![],
            comments: GitHubCommentConnection {
                total_count: 0,
                nodes: vec![],
            },
            review_decision: None,
            assignees: vec![],
            review_requests: vec![],
            reviews: GitHubReviewConnection {
                total_count: 0,
                nodes: vec![],
            },
            commits: GitHubCommitConnection {
                total_count: 0,
                nodes: vec![],
            },
        }
    }
}

#[derive(Default)]
pub(super) struct PullRequestQuery;

#[Object]
impl PullRequestQuery {
    async fn pull_requests(
        &self,
        ctx: &Context<'_>,
        after: Option<String>,
        before: Option<String>,
        first: Option<i32>,
        last: Option<i32>,
    ) -> Result<Connection<String, PullRequest, EmptyFields, EmptyFields>> {
        query(
            after,
            before,
            first,
            last,
            |after, before, first, last| async move {
                super::load_connection(ctx, Database::pull_requests, after, before, first, last)
            },
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use crate::graphql::pull_request::{
        GitHubCommentConnection, GitHubCommitConnection, GitHubPullRequestNode,
        GitHubReviewConnection, PullRequestState, RepositoryNode,
    };
    use crate::graphql::TestSchema;

    #[tokio::test]
    async fn pull_requests_empty() {
        let schema = TestSchema::new();
        let query = r"
        {
            pullRequests {
                edges {
                    node {
                        number
                    }
                }
            }
        }
        ";
        let res = schema.execute(query).await;
        assert_eq!(res.data.to_string(), "{pullRequests: {edges: []}}");
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn pull_requests_first() {
        let schema = TestSchema::new();
        let pull_requests = vec![
            GitHubPullRequestNode {
                id: "pr-1".to_string(),
                number: 1,
                title: "pull request 1".to_string(),
                body: Some(String::new()),
                state: PullRequestState::OPEN,
                created_at: "2024-01-01T00:00:00Z".parse().unwrap(),
                updated_at: "2024-01-01T00:00:00Z".parse().unwrap(),
                closed_at: None,
                merged_at: None,
                author: "author 1".to_string(),
                additions: 0,
                deletions: 0,
                url: String::new(),
                repository: RepositoryNode {
                    owner: "owner".to_string(),
                    name: "repo".to_string(),
                },
                labels: vec![],
                comments: GitHubCommentConnection {
                    total_count: 0,
                    nodes: vec![],
                },
                review_decision: None,
                assignees: vec!["assignee 1".to_string()],
                review_requests: vec!["reviewer 1".to_string()],
                reviews: GitHubReviewConnection {
                    total_count: 0,
                    nodes: vec![],
                },
                commits: GitHubCommitConnection {
                    total_count: 0,
                    nodes: vec![],
                },
            },
            GitHubPullRequestNode {
                id: "pr-2".to_string(),
                number: 2,
                title: "pull request 2".to_string(),
                body: Some(String::new()),
                state: PullRequestState::OPEN,
                created_at: "2024-01-01T00:00:00Z".parse().unwrap(),
                updated_at: "2024-01-01T00:00:00Z".parse().unwrap(),
                closed_at: None,
                merged_at: None,
                author: "author 2".to_string(),
                additions: 0,
                deletions: 0,
                url: String::new(),
                repository: RepositoryNode {
                    owner: "owner".to_string(),
                    name: "repo".to_string(),
                },
                labels: vec![],
                comments: GitHubCommentConnection {
                    total_count: 0,
                    nodes: vec![],
                },
                review_decision: None,
                assignees: vec!["assignee 2".to_string()],
                review_requests: vec!["reviewer 2".to_string()],
                reviews: GitHubReviewConnection {
                    total_count: 0,
                    nodes: vec![],
                },
                commits: GitHubCommitConnection {
                    total_count: 0,
                    nodes: vec![],
                },
            },
        ];
        schema
            .db
            .insert_pull_requests(pull_requests, "owner", "name")
            .unwrap();

        let query = r"
        {
            pullRequests(first: 1) {
                edges {
                    node {
                        number
                    }
                }
                pageInfo {
                    hasNextPage
                }
            }
        }
        ";
        let res = schema.execute(query).await;
        assert_eq!(
            res.data.to_string(),
            "{pullRequests: {edges: [{node: {number: 1}}], pageInfo: {hasNextPage: true}}}"
        );

        let query = r"
        {
            pullRequests(first: 5) {
                pageInfo {
                    hasNextPage
                }
            }
        }
        ";
        let res = schema.execute(query).await;
        assert_eq!(
            res.data.to_string(),
            "{pullRequests: {pageInfo: {hasNextPage: false}}}"
        );
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn pull_requests_last() {
        let schema = TestSchema::new();
        let pull_requests = vec![
            GitHubPullRequestNode {
                id: "pr-1".to_string(),
                number: 1,
                title: "pull request 1".to_string(),
                body: Some(String::new()),
                state: PullRequestState::OPEN,
                created_at: "2024-01-01T00:00:00Z".parse().unwrap(),
                updated_at: "2024-01-01T00:00:00Z".parse().unwrap(),
                closed_at: None,
                merged_at: None,
                author: "author 1".to_string(),
                additions: 0,
                deletions: 0,
                url: String::new(),
                repository: RepositoryNode {
                    owner: "owner".to_string(),
                    name: "repo".to_string(),
                },
                labels: vec![],
                comments: GitHubCommentConnection {
                    total_count: 0,
                    nodes: vec![],
                },
                review_decision: None,
                assignees: vec!["assignee 1".to_string()],
                review_requests: vec!["reviewer 1".to_string()],
                reviews: GitHubReviewConnection {
                    total_count: 0,
                    nodes: vec![],
                },
                commits: GitHubCommitConnection {
                    total_count: 0,
                    nodes: vec![],
                },
            },
            GitHubPullRequestNode {
                id: "pr-2".to_string(),
                number: 2,
                title: "pull request 2".to_string(),
                body: Some(String::new()),
                state: PullRequestState::OPEN,
                created_at: "2024-01-01T00:00:00Z".parse().unwrap(),
                updated_at: "2024-01-01T00:00:00Z".parse().unwrap(),
                closed_at: None,
                merged_at: None,
                author: "author 2".to_string(),
                additions: 0,
                deletions: 0,
                url: String::new(),
                repository: RepositoryNode {
                    owner: "owner".to_string(),
                    name: "repo".to_string(),
                },
                labels: vec![],
                comments: GitHubCommentConnection {
                    total_count: 0,
                    nodes: vec![],
                },
                review_decision: None,
                assignees: vec!["assignee 2".to_string()],
                review_requests: vec!["reviewer 2".to_string()],
                reviews: GitHubReviewConnection {
                    total_count: 0,
                    nodes: vec![],
                },
                commits: GitHubCommitConnection {
                    total_count: 0,
                    nodes: vec![],
                },
            },
        ];
        schema
            .db
            .insert_pull_requests(pull_requests, "owner", "name")
            .unwrap();

        let query = r"
        {
            pullRequests(last: 1) {
                edges {
                    node {
                        number
                    }
                }
                pageInfo {
                    hasPreviousPage
                }
            }
        }
        ";
        let res = schema.execute(query).await;
        assert_eq!(
            res.data.to_string(),
            "{pullRequests: {edges: [{node: {number: 2}}], pageInfo: {hasPreviousPage: true}}}"
        );

        let query = r"
        {
            pullRequests(last: 2) {
                pageInfo {
                    hasPreviousPage
                }
            }
        }
        ";
        let res = schema.execute(query).await;
        assert_eq!(
            res.data.to_string(),
            "{pullRequests: {pageInfo: {hasPreviousPage: false}}}"
        );
    }

    #[tokio::test]
    async fn default_github_pull_request_node() {
        let pr = GitHubPullRequestNode::default();
        assert_eq!(pr.number, 0); // basic sanity check
        assert!(pr.id.is_empty());
        assert!(pr.repository.owner.is_empty());
    }
}
