use anyhow::Result;
use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use super::{Database, Iter};
use crate::api::pull_request::PullRequest;
use crate::outbound::pull_requests::{
    self, PullRequestReviewDecision, PullRequestReviewState, PullRequestState,
};

impl Database {
    pub(crate) fn insert_pull_requests(
        &self,
        resp: Vec<GitHubPullRequestNode>,
        owner: &str,
        name: &str,
    ) -> Result<()> {
        for item in resp {
            let keystr: String = format!("{owner}/{name}#{}", item.number);
            Database::insert(&keystr, item, &self.pull_request_partition)?;
        }
        Ok(())
    }

    pub(crate) fn pull_requests(
        &self,
        start: Option<&[u8]>,
        end: Option<&[u8]>,
    ) -> Iter<PullRequest> {
        let start = start.unwrap_or(b"\x00");
        if let Some(end) = end {
            Iter::new(self.pull_request_partition.range(start..end))
        } else {
            Iter::new(self.pull_request_partition.range(start..))
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GitHubPRComment {
    pub(crate) author: String,
    pub(crate) body: String,
    pub(crate) created_at: Timestamp,
    pub(crate) updated_at: Timestamp,
    pub(crate) repository_name: String,
    pub(crate) url: String,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct GitHubPRCommentConnection {
    pub(crate) total_count: i32,
    pub(crate) nodes: Vec<GitHubPRComment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitInner {
    pub(crate) additions: i32,
    pub(crate) deletions: i32,
    pub(crate) message: String,
    pub(crate) message_body: Option<String>,
    pub(crate) author: String,
    pub(crate) changed_files_if_available: Option<i32>,
    pub(crate) committed_date: Timestamp,
    pub(crate) committer: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryNode {
    pub(crate) owner: String,
    pub(crate) name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReviewNode {
    pub(crate) author: String,
    pub(crate) state: PullRequestReviewState,
    pub(crate) body: String,
    pub(crate) url: String,
    pub(crate) created_at: Timestamp,
    pub(crate) published_at: Option<Timestamp>,
    pub(crate) submitted_at: Option<Timestamp>,
    pub(crate) is_minimized: bool,
    pub(crate) comments: GitHubPRCommentConnection,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubCommitConnection {
    pub(crate) total_count: i32,
    pub(crate) nodes: Vec<CommitInner>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GitHubReviewConnection {
    pub(crate) total_count: i32,
    pub(crate) nodes: Vec<ReviewNode>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GitHubPullRequestNode {
    pub(crate) id: String,
    pub(crate) number: i32,
    pub(crate) title: String,
    pub(crate) body: String,
    pub(crate) state: PullRequestState,
    pub(crate) created_at: Timestamp,
    pub(crate) updated_at: Timestamp,
    pub(crate) closed_at: Option<Timestamp>,
    pub(crate) merged_at: Option<Timestamp>,
    pub(crate) author: String,
    pub(crate) additions: i32,
    pub(crate) deletions: i32,
    pub(crate) url: String,
    pub(crate) repository: RepositoryNode,
    pub(crate) labels: Vec<String>,
    pub(crate) comments: GitHubPRCommentConnection,
    pub(crate) review_decision: Option<PullRequestReviewState>,
    pub(crate) assignees: Vec<String>,
    pub(crate) review_requests: Vec<String>,
    pub(crate) reviews: GitHubReviewConnection,
    pub(crate) commits: GitHubCommitConnection,
}

impl From<pull_requests::PullRequestsRepositoryPullRequestsNodesAuthor> for String {
    fn from(author: pull_requests::PullRequestsRepositoryPullRequestsNodesAuthor) -> Self {
        match author {
            pull_requests::PullRequestsRepositoryPullRequestsNodesAuthor::User(u) => u.login,
            _ => String::new(),
        }
    }
}

impl From<pull_requests::PullRequestsRepositoryPullRequestsNodesLabels> for Vec<String> {
    fn from(labels: pull_requests::PullRequestsRepositoryPullRequestsNodesLabels) -> Self {
        labels
            .nodes
            .unwrap_or_default()
            .into_iter()
            .flatten()
            .map(|n| n.name)
            .collect()
    }
}

impl From<pull_requests::PullRequestsRepositoryPullRequestsNodesAssignees> for Vec<String> {
    fn from(assignees: pull_requests::PullRequestsRepositoryPullRequestsNodesAssignees) -> Self {
        assignees
            .nodes
            .unwrap_or_default()
            .into_iter()
            .flatten()
            .map(|u| u.login)
            .collect()
    }
}

impl From<pull_requests::PullRequestsRepositoryPullRequestsNodesReviewRequests> for Vec<String> {
    fn from(reqs: pull_requests::PullRequestsRepositoryPullRequestsNodesReviewRequests) -> Self {
        reqs
            .nodes
            .unwrap_or_default()
            .into_iter()
            .flatten()
            .filter_map(|rr| match rr.requested_reviewer {
                Some(
                    pull_requests::PullRequestsRepositoryPullRequestsNodesReviewRequestsNodesRequestedReviewer::User(u),
                ) => Some(u.login),
                _ => None,
            })
            .collect()
    }
}

impl TryFrom<pull_requests::PullRequestsRepositoryPullRequestsNodesReviews>
    for GitHubReviewConnection
{
    type Error = anyhow::Error;

    fn try_from(
        reviews: pull_requests::PullRequestsRepositoryPullRequestsNodesReviews,
    ) -> Result<Self> {
        let total_count: i32 = reviews.total_count.try_into()?;
        let nodes = reviews
            .nodes
            .unwrap_or_default()
            .into_iter()
            .flatten()
            .map(|node| ReviewNode {
                author: node
                    .author
                    .and_then(|a| match a {
                        pull_requests::PullRequestsRepositoryPullRequestsNodesReviewsNodesAuthor::User(
                            u,
                        ) => Some(u.login),
                        _ => None,
                    })
                    .unwrap_or_default(),
                state: node.state,
                body: node.body,
                url: node.url,
                created_at: node.created_at,
                published_at: node.published_at,
                submitted_at: node.submitted_at,
                is_minimized: node.is_minimized,
                comments: GitHubPRCommentConnection {
                    total_count: node.comments.total_count.try_into().unwrap_or_default(),
                    // FIX: assign real data, it already exists
                    nodes: vec![],
                },
            })
            .collect();

        Ok(Self { total_count, nodes })
    }
}

impl TryFrom<pull_requests::PullRequestsRepositoryPullRequestsNodesCommits>
    for GitHubCommitConnection
{
    type Error = anyhow::Error;

    fn try_from(
        commits: pull_requests::PullRequestsRepositoryPullRequestsNodesCommits,
    ) -> Result<Self> {
        let total_count: i32 = commits.total_count.try_into()?;
        let nodes = commits
            .nodes
            .unwrap_or_default()
            .into_iter()
            .flatten()
            .map(|node| {
                let commit = node.commit;
                Ok(CommitInner {
                    additions: commit.additions.try_into()?,
                    deletions: commit.deletions.try_into()?,
                    message: commit.message,
                    message_body: Some(commit.message_body),
                    author: commit
                        .author
                        .and_then(|a| a.user)
                        .map(|u| u.login)
                        .unwrap_or_default(),
                    changed_files_if_available: commit
                        .changed_files_if_available
                        .map(TryInto::try_into)
                        .transpose()?,
                    committed_date: commit.committed_date,
                    committer: commit
                        .committer
                        .and_then(|c| c.user)
                        .map(|u| u.login)
                        .unwrap_or_default(),
                })
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(Self { total_count, nodes })
    }
}

impl TryFrom<pull_requests::PullRequestsRepositoryPullRequestsNodes> for GitHubPullRequestNode {
    type Error = anyhow::Error;

    fn try_from(pr: pull_requests::PullRequestsRepositoryPullRequestsNodes) -> Result<Self> {
        let number: i32 = pr.number.try_into()?;
        let author = pr.author.map(String::from).unwrap_or_default();
        let additions: i32 = pr.additions.try_into()?;
        let deletions: i32 = pr.deletions.try_into()?;

        // Labels and assignees
        let labels = pr.labels.map(Vec::<String>::from).unwrap_or_default();
        let assignees = Vec::<String>::from(pr.assignees);
        let review_requests = pr
            .review_requests
            .map(Vec::<String>::from)
            .unwrap_or_default();

        // Comments require repository name context
        let comments_total: i32 = pr.comments.total_count.try_into()?;
        let repo_owner = pr.repository.owner.login.clone();
        let repo_name = pr.repository.name.clone();
        let comments_nodes = pr
            .comments
            .nodes
            .unwrap_or_default()
            .into_iter()
            .flatten()
            .map(|node| GitHubPRComment {
                author: match node.author {
                    Some(
                        pull_requests::PullRequestsRepositoryPullRequestsNodesCommentsNodesAuthor::User(
                            u,
                        ),
                    ) => u.login,
                    _ => String::new(),
                },
                body: node.body,
                created_at: node.created_at,
                updated_at: node.updated_at,
                repository_name: repo_name.clone(),
                // FIX: add url field to comments query and assign it here
                url: String::new(),
            })
            .collect();

        let reviews = pr
            .reviews
            .map(GitHubReviewConnection::try_from)
            .transpose()?
            .unwrap_or_else(|| GitHubReviewConnection {
                total_count: 0,
                nodes: vec![],
            });

        let commits = GitHubCommitConnection::try_from(pr.commits)?;

        let review_decision = pr.review_decision.and_then(|d| match d {
            PullRequestReviewDecision::APPROVED => Some(PullRequestReviewState::APPROVED),
            PullRequestReviewDecision::CHANGES_REQUESTED => {
                Some(PullRequestReviewState::CHANGES_REQUESTED)
            }
            PullRequestReviewDecision::REVIEW_REQUIRED => Some(PullRequestReviewState::PENDING),
            PullRequestReviewDecision::Other(_) => None,
        });

        Ok(Self {
            id: pr.id,
            number,
            title: pr.title,
            body: pr.body,
            state: pr.state,
            created_at: pr.created_at,
            updated_at: pr.updated_at,
            closed_at: pr.closed_at,
            merged_at: pr.merged_at,
            author,
            additions,
            deletions,
            url: pr.url,
            repository: RepositoryNode {
                owner: repo_owner,
                name: repo_name,
            },
            labels,
            comments: GitHubPRCommentConnection {
                total_count: comments_total,
                nodes: comments_nodes,
            },
            review_decision,
            assignees,
            review_requests,
            reviews,
            commits,
        })
    }
}
