use std::fmt;

use anyhow::Context as AnyhowContext;
use async_graphql::{
    connection::{query, Connection, EmptyFields},
    scalar, Context, Object, Result, SimpleObject, ID,
};
use jiff::Timestamp;

use crate::graphql::DateTimeUtc;

scalar!(PullRequestState);
scalar!(PullRequestReviewState);

use crate::database::{self, Database, TryFromKeyValue};
use crate::github::{
    GitHubCommentConnection, GitHubCommitConnection, GitHubLabelConnection, GitHubPullRequests,
    GitHubReviewConnection, GitHubReviewRequestConnection, GitHubUserConnection,
    PullRequestReviewState, PullRequestState, RepositoryNode, ReviewRequestNode,
};

#[derive(SimpleObject, Debug)]
pub(crate) struct User {
    pub(crate) login: String,
}

#[derive(SimpleObject)]
pub(crate) struct Label {
    pub(crate) name: String,
}

#[derive(SimpleObject, Debug)]
pub(crate) struct Comment {
    pub(crate) body: String,
    #[graphql(name = "createdAt")]
    pub(crate) created_at: DateTimeUtc,
    #[graphql(name = "updatedAt")]
    pub(crate) updated_at: DateTimeUtc,
    pub(crate) author: Option<User>,
}

#[derive(SimpleObject)]
pub(crate) struct Review {
    pub(crate) author: Option<User>,
    pub(crate) state: PullRequestReviewState,
    pub(crate) body: Option<String>,
    pub(crate) url: String,
    #[graphql(name = "createdAt")]
    pub(crate) created_at: DateTimeUtc,
    #[graphql(name = "publishedAt")]
    pub(crate) published_at: DateTimeUtc,
    #[graphql(name = "submittedAt")]
    pub(crate) submitted_at: DateTimeUtc,
    #[graphql(name = "isMinimized")]
    pub(crate) is_minimized: bool,
    pub(crate) comments: Vec<Comment>,
}

#[derive(SimpleObject)]
pub(crate) struct CommitInfo {
    pub(crate) additions: i32,
    pub(crate) deletions: i32,
    pub(crate) message: String,
    #[graphql(name = "messageBody")]
    pub(crate) message_body: Option<String>,
    pub(crate) author: Option<User>,
    #[graphql(name = "changedFilesIfAvailable")]
    pub(crate) changed_files_if_available: Option<i32>,
    #[graphql(name = "committedDate")]
    pub(crate) committed_date: DateTimeUtc,
    pub(crate) committer: Option<User>,
}

#[derive(SimpleObject)]
pub(crate) struct PullRequest {
    pub(crate) id: ID,
    pub(crate) number: i32,
    pub(crate) title: String,
    pub(crate) body: Option<String>,
    pub(crate) state: PullRequestState,
    #[graphql(name = "assignees")]
    pub(crate) assignees: Vec<User>,
    #[graphql(name = "createdAt")]
    pub(crate) created_at: DateTimeUtc,
    #[graphql(name = "updatedAt")]
    pub(crate) updated_at: DateTimeUtc,
    #[graphql(name = "closedAt")]
    pub(crate) closed_at: Option<DateTimeUtc>,
    #[graphql(name = "mergedAt")]
    pub(crate) merged_at: Option<DateTimeUtc>,
    pub(crate) author: Option<User>,
    pub(crate) additions: i32,
    pub(crate) deletions: i32,
    pub(crate) url: String,
    pub(crate) repository_owner: User,
    pub(crate) repository_name: String,
    pub(crate) labels: Vec<Label>,
    #[graphql(name = "commentsCount")]
    pub(crate) comments_count: i32,
    pub(crate) comments: Vec<Comment>,
    #[graphql(name = "reviewDecision")]
    pub(crate) review_decision: Option<PullRequestReviewState>,
    #[graphql(name = "reviewRequests")]
    pub(crate) review_requests: Vec<User>,
    #[graphql(name = "reviewsCount")]
    pub(crate) reviews_count: i32,
    pub(crate) reviews: Vec<Review>,
    #[graphql(name = "commitsCount")]
    pub(crate) commits_count: i32,
    pub(crate) commits: Vec<CommitInfo>,
    // retained for key parsing
    #[graphql(skip)]
    owner: String,
    #[graphql(skip)]
    repo: String,
}

impl TryFromKeyValue for PullRequest {
    #[allow(clippy::too_many_lines)]
    fn try_from_key_value(key: &[u8], value: &[u8]) -> anyhow::Result<Self> {
        let (owner, repo, _number) = database::parse_key(key)
            .with_context(|| format!("invalid key in database: {key:02x?}"))?;
        let gh: GitHubPullRequests = bincode::deserialize(value)
            .with_context(|| format!("Deserialization failed for key: {key:?}"))?;

        let labels = gh
            .labels
            .nodes
            .into_iter()
            .map(|l| Label { name: l.name })
            .collect();

        let comments = gh
            .comments
            .nodes
            .into_iter()
            .map(|c| Comment {
                body: c.body,
                created_at: DateTimeUtc(c.created_at),
                updated_at: DateTimeUtc(c.updated_at),
                author: c.author.map(|login| User { login }),
            })
            .collect();

        let reviews = gh
            .reviews
            .nodes
            .into_iter()
            .map(|r| Review {
                author: r.author.map(|login| User { login }),
                state: r.state,
                body: r.body,
                url: r.url,
                created_at: DateTimeUtc(r.created_at),
                published_at: DateTimeUtc(r.published_at),
                submitted_at: DateTimeUtc(r.submitted_at),
                is_minimized: r.is_minimized,
                comments: r
                    .comments
                    .nodes
                    .into_iter()
                    .map(|c| Comment {
                        body: c.body,
                        created_at: DateTimeUtc(c.created_at),
                        updated_at: DateTimeUtc(c.updated_at),
                        author: c.author.map(|login| User { login }),
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
                author: c.author.and_then(|cp| cp.user.map(|login| User { login })),
                changed_files_if_available: c.changed_files_if_available,
                committed_date: DateTimeUtc(c.committed_date),
                committer: c
                    .committer
                    .and_then(|cp| cp.user.map(|login| User { login })),
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
            author: gh.author.map(|login| User { login }),
            additions: gh.additions,
            deletions: gh.deletions,
            url: gh.url,
            repository_owner: User {
                login: gh.repository.owner,
            },
            repository_name: gh.repository.name,
            labels,
            comments_count: gh.comments.total_count,
            comments,
            review_decision: gh.review_decision,
            assignees: gh
                .assignees
                .nodes
                .into_iter()
                .map(|login| User { login })
                .collect(),
            review_requests: gh
                .review_requests
                .nodes
                .into_iter()
                .filter_map(|rr| rr.requested_reviewer.map(|login| User { login }))
                .collect(),
            reviews_count: gh.reviews.total_count,
            reviews,
            commits_count: gh.commits.total_count,
            commits,
            owner,
            repo,
        })
    }
}

impl fmt::Display for PullRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}/{}#{}", self.owner, self.repo, self.number)
    }
}

// Helper conversions and defaults to simplify test setup
impl From<Vec<String>> for GitHubUserConnection {
    fn from(logins: Vec<String>) -> Self {
        GitHubUserConnection { nodes: logins }
    }
}

impl From<Vec<String>> for GitHubReviewRequestConnection {
    fn from(logins: Vec<String>) -> Self {
        GitHubReviewRequestConnection {
            nodes: logins
                .into_iter()
                .map(|login| ReviewRequestNode {
                    requested_reviewer: Some(login),
                })
                .collect(),
        }
    }
}

impl Default for GitHubPullRequests {
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
            author: None,
            additions: 0,
            deletions: 0,
            url: String::new(),
            repository: RepositoryNode {
                owner: String::new(),
                name: String::new(),
            },

            labels: GitHubLabelConnection { nodes: vec![] },
            comments: GitHubCommentConnection {
                total_count: 0,
                nodes: vec![],
            },
            review_decision: None,
            assignees: GitHubUserConnection { nodes: vec![] },
            review_requests: GitHubReviewRequestConnection { nodes: vec![] },
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
    use crate::github::{
        GitHubCommentConnection, GitHubCommitConnection, GitHubLabelConnection, GitHubPullRequests,
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
            GitHubPullRequests {
                id: "pr-1".to_string(),
                number: 1,
                title: "pull request 1".to_string(),
                body: Some(String::new()),
                state: PullRequestState::OPEN,
                created_at: "2024-01-01T00:00:00Z".parse().unwrap(),
                updated_at: "2024-01-01T00:00:00Z".parse().unwrap(),
                closed_at: None,
                merged_at: None,
                author: Some("author 1".to_string()),
                additions: 0,
                deletions: 0,
                url: String::new(),
                repository: RepositoryNode {
                    owner: "owner".to_string(),
                    name: "repo".to_string(),
                },
                labels: GitHubLabelConnection { nodes: vec![] },
                comments: GitHubCommentConnection {
                    total_count: 0,
                    nodes: vec![],
                },
                review_decision: None,
                assignees: vec!["assignee 1".to_string()].into(),
                review_requests: vec!["reviewer 1".to_string()].into(),
                reviews: GitHubReviewConnection {
                    total_count: 0,
                    nodes: vec![],
                },
                commits: GitHubCommitConnection {
                    total_count: 0,
                    nodes: vec![],
                },
            },
            GitHubPullRequests {
                id: "pr-2".to_string(),
                number: 2,
                title: "pull request 2".to_string(),
                body: Some(String::new()),
                state: PullRequestState::OPEN,
                created_at: "2024-01-01T00:00:00Z".parse().unwrap(),
                updated_at: "2024-01-01T00:00:00Z".parse().unwrap(),
                closed_at: None,
                merged_at: None,
                author: Some("author 2".to_string()),
                additions: 0,
                deletions: 0,
                url: String::new(),
                repository: RepositoryNode {
                    owner: "owner".to_string(),
                    name: "repo".to_string(),
                },
                labels: GitHubLabelConnection { nodes: vec![] },
                comments: GitHubCommentConnection {
                    total_count: 0,
                    nodes: vec![],
                },
                review_decision: None,
                assignees: vec!["assignee 2".to_string()].into(),
                review_requests: vec!["reviewer 2".to_string()].into(),
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
            GitHubPullRequests {
                id: "pr-1".to_string(),
                number: 1,
                title: "pull request 1".to_string(),
                body: Some(String::new()),
                state: PullRequestState::OPEN,
                created_at: "2024-01-01T00:00:00Z".parse().unwrap(),
                updated_at: "2024-01-01T00:00:00Z".parse().unwrap(),
                closed_at: None,
                merged_at: None,
                author: Some("author 1".to_string()),
                additions: 0,
                deletions: 0,
                url: String::new(),
                repository: RepositoryNode {
                    owner: "owner".to_string(),
                    name: "repo".to_string(),
                },
                labels: GitHubLabelConnection { nodes: vec![] },
                comments: GitHubCommentConnection {
                    total_count: 0,
                    nodes: vec![],
                },
                review_decision: None,
                assignees: vec!["assignee 1".to_string()].into(),
                review_requests: vec!["reviewer 1".to_string()].into(),
                reviews: GitHubReviewConnection {
                    total_count: 0,
                    nodes: vec![],
                },
                commits: GitHubCommitConnection {
                    total_count: 0,
                    nodes: vec![],
                },
            },
            GitHubPullRequests {
                id: "pr-2".to_string(),
                number: 2,
                title: "pull request 2".to_string(),
                body: Some(String::new()),
                state: PullRequestState::OPEN,
                created_at: "2024-01-01T00:00:00Z".parse().unwrap(),
                updated_at: "2024-01-01T00:00:00Z".parse().unwrap(),
                closed_at: None,
                merged_at: None,
                author: Some("author 2".to_string()),
                additions: 0,
                deletions: 0,
                url: String::new(),
                repository: RepositoryNode {
                    owner: "owner".to_string(),
                    name: "repo".to_string(),
                },
                labels: GitHubLabelConnection { nodes: vec![] },
                comments: GitHubCommentConnection {
                    total_count: 0,
                    nodes: vec![],
                },
                review_decision: None,
                assignees: vec!["assignee 2".to_string()].into(),
                review_requests: vec!["reviewer 2".to_string()].into(),
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
}
