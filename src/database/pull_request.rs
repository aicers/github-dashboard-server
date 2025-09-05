use anyhow::Result;
use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use super::{Database, Iter};
use crate::api::pull_request::PullRequest;
use crate::outbound::pull_requests::{PullRequestReviewState, PullRequestState};

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
    pub(crate) body: Option<String>,
    pub(crate) url: String,
    pub(crate) created_at: Timestamp,
    pub(crate) published_at: Option<Timestamp>,
    pub(crate) submitted_at: Timestamp,
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
    pub(crate) body: Option<String>,
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
