use std::{sync::Arc, time::Duration};

use anyhow::{bail, Context, Error, Result};
use graphql_client::{GraphQLQuery, QueryBody, Response as GraphQlResponse};
use jiff::Timestamp;
use reqwest::{Client, RequestBuilder, Response};
use serde::{Deserialize, Serialize};
use tokio::time;
use tracing::error;

pub use self::pull_requests::{PullRequestReviewState, PullRequestState as PRPullRequestState};
use crate::database::DiscussionDbSchema;
use crate::{
    database::{issue::GitHubIssue, Database},
    outbound::{
        issues::IssueState,
        pull_requests::{
            PullRequestReviewDecision,
            PullRequestsRepositoryPullRequestsNodesAuthor::User as PullRequestAuthorUser,
            PullRequestsRepositoryPullRequestsNodesCommentsNodesAuthor as PRCommentAuthor,
            PullRequestsRepositoryPullRequestsNodesReviewRequestsNodesRequestedReviewer::User as PRReviewRequestedUser,
        },
    },
    settings::Repository as RepoInfo,
};

const GITHUB_FETCH_SIZE: i64 = 10;
const GITHUB_URL: &str = "https://api.github.com/graphql";
const APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"),);
const INIT_TIME: &str = "1992-06-05T00:00:00Z";

type DateTime = Timestamp;

#[allow(clippy::upper_case_acronyms)]
type URI = String;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/outbound/graphql/schema.graphql",
    query_path = "src/outbound/graphql/issues.graphql",
    response_derives = "Debug, Clone, PartialEq"
)]
pub(crate) struct Issues;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/outbound/graphql/schema.graphql",
    query_path = "src/outbound/graphql/pull_requests.graphql",
    response_derives = "Debug, Clone"
)]
pub(crate) struct PullRequests;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/outbound/graphql/schema.graphql",
    query_path = "src/outbound/graphql/discussions.graphql",
    response_derives = "Debug"
)]
pub(crate) struct Discussions;

#[allow(clippy::derivable_impls)]
impl Default for IssueState {
    fn default() -> Self {
        IssueState::OPEN
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct GitHubPRComment {
    pub(crate) author: String,
    pub(crate) body: String,
    pub(crate) created_at: Timestamp,
    pub(crate) updated_at: Timestamp,
    pub(crate) repository_name: String,
    pub(crate) url: String,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub(crate) struct GitHubPRCommentConnection {
    pub(crate) total_count: i32,
    pub(crate) nodes: Vec<GitHubPRComment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct CommitInner {
    pub(super) additions: i32,
    pub(super) deletions: i32,
    pub(super) message: String,
    pub(super) message_body: Option<String>,
    pub(super) author: String,
    pub(super) changed_files_if_available: Option<i32>,
    pub(super) committed_date: DateTime,
    pub(super) committer: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct RepositoryNode {
    pub(super) owner: String,
    pub(super) name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct ReviewNode {
    pub(super) author: String,
    pub(super) state: PullRequestReviewState,
    pub(super) body: Option<String>,
    pub(super) url: String,
    pub(super) created_at: DateTime,
    pub(super) published_at: Option<DateTime>,
    pub(super) submitted_at: DateTime,
    pub(super) is_minimized: bool,
    pub(super) comments: GitHubPRCommentConnection,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct GitHubCommitConnection {
    pub(super) total_count: i32,
    pub(super) nodes: Vec<CommitInner>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct GitHubReviewConnection {
    pub(super) total_count: i32,
    pub(super) nodes: Vec<ReviewNode>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct GitHubPullRequestNode {
    pub(super) id: String,
    pub(super) number: i32,
    pub(super) title: String,
    pub(super) body: Option<String>,
    pub(super) state: PRPullRequestState,
    pub(super) created_at: DateTime,
    pub(super) updated_at: DateTime,
    pub(super) closed_at: Option<DateTime>,
    pub(super) merged_at: Option<DateTime>,
    pub(super) author: String,
    pub(super) additions: i32,
    pub(super) deletions: i32,
    pub(super) url: String,
    pub(super) repository: RepositoryNode,
    pub(super) labels: Vec<String>,
    pub(super) comments: GitHubPRCommentConnection,
    pub(super) review_decision: Option<PullRequestReviewState>,
    pub(super) assignees: Vec<String>,
    pub(super) review_requests: Vec<String>,
    pub(super) reviews: GitHubReviewConnection,
    pub(super) commits: GitHubCommitConnection,
}

pub(super) async fn fetch_periodically(
    repositories: Arc<Vec<RepoInfo>>,
    token: String,
    period: Duration,
    retry: Duration,
    db: Database,
) {
    let mut itv = time::interval(period);
    loop {
        itv.tick().await;
        let since = match db.select_db("since") {
            Ok(r) => r,
            Err(_) => INIT_TIME.to_string(),
        };
        if let Err(e) = db.insert_db("since", format!("{:?}", Timestamp::now())) {
            error!("Insert DateTime Error: {}", e);
        }

        for repoinfo in repositories.iter() {
            let mut re_itv = time::interval(retry);
            loop {
                re_itv.tick().await;
                match send_github_issue_query(&repoinfo.owner, &repoinfo.name, &since, &token).await
                {
                    Ok(resps) => {
                        if let Err(error) = db.insert_issues(resps, &repoinfo.owner, &repoinfo.name)
                        {
                            error!("Problem while insert Fjall Database. {}", error);
                        }
                        break;
                    }
                    Err(error) => {
                        error!("Problem while sending github issue query. Query retransmission is done after 5 minutes. {}", error);
                    }
                }
                itv.reset();
            }

            let mut re_itv = time::interval(retry);
            loop {
                re_itv.tick().await;
                match send_github_pr_query(&repoinfo.owner, &repoinfo.name, &token).await {
                    Ok(resps) => {
                        if let Err(error) =
                            db.insert_pull_requests(resps, &repoinfo.owner, &repoinfo.name)
                        {
                            error!("Problem while insert Fjall Database. {}", error);
                        }
                        break;
                    }
                    Err(error) => {
                        error!("Problem while sending github pr query. Query retransmission is done after 5 minutes. {}", error);
                    }
                }
                itv.reset();
            }

            let mut re_itv = time::interval(retry);
            loop {
                re_itv.tick().await;
                match send_github_discussion_query(&repoinfo.owner, &repoinfo.name, &token).await {
                    Ok(resps) => {
                        if let Err(error) =
                            db.insert_discussions(resps, &repoinfo.owner, &repoinfo.name)
                        {
                            error!("Problem while insert Fjall Database. {}", error);
                        }
                        break;
                    }
                    Err(error) => {
                        error!("Problem while sending github discussion query. Query retransmission is done after 5 minutes. {}", error);
                    }
                }
                itv.reset();
            }
        }
    }
}

async fn send_github_issue_query(
    owner: &str,
    name: &str,
    since: &str,
    token: &str,
) -> Result<Vec<GitHubIssue>> {
    let mut total_issue = Vec::new();
    let mut end_cursor: Option<String> = None;
    loop {
        let var = issues::Variables {
            owner: owner.to_string(),
            name: name.to_string(),
            first: Some(GITHUB_FETCH_SIZE),
            last: None,
            before: None,
            after: end_cursor.take(),
            since: Some(since.parse::<DateTime>()?),
        };
        let resp_body: GraphQlResponse<issues::ResponseData> =
            send_query::<Issues>(token, var).await?.json().await?;

        let issue_resp = GitHubIssueResponse::try_from(resp_body)?;
        total_issue.extend(issue_resp.issues);

        if !issue_resp.has_next_page {
            break;
        }
        end_cursor = issue_resp.end_cursor;
    }

    Ok(total_issue)
}
#[allow(clippy::too_many_lines)]
async fn send_github_pr_query(
    owner: &str,
    name: &str,
    token: &str,
) -> Result<Vec<GitHubPullRequestNode>> {
    let mut prs: Vec<GitHubPullRequestNode> = Vec::new();
    let mut end_cur: Option<String> = None;
    loop {
        let var = pull_requests::Variables {
            owner: owner.to_string(),
            name: name.to_string(),
            first: Some(GITHUB_FETCH_SIZE),
            last: None,
            before: None,
            after: end_cur.take(),
        };

        let resp_body: GraphQlResponse<pull_requests::ResponseData> =
            send_query::<PullRequests>(token, var).await?.json().await?;
        if let Some(data) = resp_body.data {
            if let Some(repository) = data.repository {
                if let Some(nodes) = repository.pull_requests.nodes {
                    for pr in nodes.into_iter().flatten() {
                        let mut assignees_list = Vec::new();
                        if let Some(ass_nodes) = pr.assignees.nodes {
                            for node in ass_nodes.into_iter().flatten() {
                                assignees_list.push(node.login);
                            }
                        }
                        let mut rr_nodes = Vec::new();
                        if let Some(req_conn) = pr.review_requests {
                            if let Some(req_nodes) = req_conn.nodes {
                                for rr in req_nodes.into_iter().flatten() {
                                    if let Some(PRReviewRequestedUser(user_node)) =
                                        rr.requested_reviewer
                                    {
                                        rr_nodes.push(user_node.login);
                                    }
                                }
                            }
                        }
                        prs.push(GitHubPullRequestNode {
                            id: pr.id,
                            number: pr.number.try_into().unwrap_or_default(),
                            title: pr.title,
                            body: Some(pr.body),
                            state: pr.state,
                            created_at: pr.created_at,
                            updated_at: pr.updated_at,
                            closed_at: pr.closed_at,
                            merged_at: pr.merged_at,
                            author: match pr.author {
                            Some(PullRequestAuthorUser(user)) => user.login,
                            _ => String::new(),
                        },
                            additions: pr.additions.try_into().unwrap_or_default(),
                            deletions: pr.deletions.try_into().unwrap_or_default(),
                            url: pr.url,
                            repository: RepositoryNode {
                                owner: pr.repository.owner.login,
                                name: pr.repository.name.clone(),
                            },
                            labels: pr
                                .labels
                                .as_ref()
                                .and_then(|conn| conn.nodes.as_ref())
                                .map(|nodes| {
                                    nodes
                                        .iter()
                                        .filter_map(|n| n.as_ref().map(|node| node.name.clone()))
                                        .collect::<Vec<String>>()
                                })
                                .unwrap_or_default(),
                            comments: GitHubPRCommentConnection {
                                total_count: pr.comments.total_count.try_into().unwrap_or_default(),
                                nodes: pr
                                    .comments
                                    .nodes
                                    .as_ref()
                                    .into_iter()
                                    .flatten()
                                    .filter_map(|n| n.as_ref())
                                    .map(|node| GitHubPRComment {
                                        author: match &node.author {
                                            Some(PRCommentAuthor::User(u)) => u.login.clone(),
                                            _ => String::new(),
                                        },
                                        body: node.body.clone(),
                                        created_at: node.created_at,
                                        updated_at: node.updated_at,
                                        repository_name: pr.repository.name.clone(),
                                        url: String::new(),
                                    })
                                    .collect(),
                            },

                            review_decision: pr.review_decision.and_then(|d| match d {
                                PullRequestReviewDecision::APPROVED => Some(PullRequestReviewState::APPROVED),
                                PullRequestReviewDecision::CHANGES_REQUESTED => Some(PullRequestReviewState::CHANGES_REQUESTED),
                                PullRequestReviewDecision::REVIEW_REQUIRED => Some(PullRequestReviewState::PENDING),
                                PullRequestReviewDecision::Other(_) => None,
                            }),
                            assignees: assignees_list,
                            review_requests: rr_nodes,
                            reviews: GitHubReviewConnection {
                                total_count: pr
                                    .reviews
                                    .as_ref()
                                    .map(|r| r.total_count.try_into().unwrap_or_default())
                                    .unwrap_or_default(),
                                nodes: pr
                                    .reviews
                                    .as_ref()
                                    .and_then(|r| r.nodes.as_ref())
                                    .map(|nodes| {
                                        nodes
                                            .iter()
                                            .filter_map(|n| n.as_ref())
                                            .map(|node| ReviewNode {
                                                author: node.author.as_ref().and_then(|a| match a {
                                                    pull_requests::PullRequestsRepositoryPullRequestsNodesReviewsNodesAuthor::User(u) => Some(u.login.clone()),
                                                    _ => None,
                                                }).unwrap_or_default(),
                                                state: node.state.clone(),
                                                body: Some(node.body.clone()),
                                                url: node.url.clone(),
                                                created_at: node.created_at,
                                                published_at: node.published_at,
                                                submitted_at: node.submitted_at.unwrap_or_else(Timestamp::now),
                                                is_minimized: node.is_minimized,
                                                comments: GitHubPRCommentConnection {
                                                    total_count: node.comments.total_count.try_into().unwrap_or_default(),
                                                    nodes: vec![],
                                                },
                                            })
                                            .collect()
                                    })
                                    .unwrap_or_default(),
                            },
                            commits: GitHubCommitConnection {
                                total_count: pr
                                    .commits
                                    .total_count
                                    .try_into()
                                    .unwrap_or_default(),
                                nodes: pr
                                    .commits
                                    .nodes
                                    .as_ref()
                                    .map_or(vec![], |nodes| {
                                        nodes
                                            .iter()
                                            .filter_map(|n| n.as_ref())
                                            .map(|node| {
                                                let commit = &node.commit;
                                                CommitInner {
                                                    additions: commit.additions.try_into().unwrap_or_default(),
                                                    deletions: commit.deletions.try_into().unwrap_or_default(),
                                                    message: commit.message.clone(),
                                                    message_body: Some(commit.message_body.clone()),
                                                    author: commit
                                                        .author
                                                        .as_ref()
                                                        .and_then(|a| a.user.as_ref()).map(|u| u.login.clone())
                                                        .unwrap_or_default(),
                                                    changed_files_if_available: commit
                                                        .changed_files_if_available
                                                        .and_then(|v| v.try_into().ok()),
                                                    committed_date: commit.committed_date,
                                                    committer: commit
                                                        .committer
                                                        .as_ref()
                                                        .and_then(|c| c.user.as_ref())
                                                        .map(|user| user.login.clone())
                                                        .unwrap_or_default(),

                                                }
                                            })
                                            .collect()
                                    }),
                            }
                        });
                    }
                    if !repository.pull_requests.page_info.has_next_page {
                        break;
                    }
                    end_cur = repository.pull_requests.page_info.end_cursor;
                    continue;
                }
                end_cur = repository.pull_requests.page_info.end_cursor;
                continue;
            }
        }
        bail!("Failed to parse response data");
    }
    Ok(prs)
}

async fn send_github_discussion_query(
    owner: &str,
    name: &str,
    token: &str,
) -> Result<Vec<DiscussionDbSchema>> {
    let mut end_cur: Option<String> = None;
    let mut discussions = Vec::new();
    loop {
        let var = discussions::Variables {
            owner: owner.to_string(),
            name: name.to_string(),
            first: Some(GITHUB_FETCH_SIZE),
            last: None,
            before: None,
            after: end_cur,
        };

        let resp_body: GraphQlResponse<discussions::ResponseData> =
            send_query::<Discussions>(token, var).await?.json().await?;
        if let Some(data) = resp_body.data {
            if let Some(repository) = data.repository {
                if let Some(nodes) = repository.discussions.nodes {
                    let mut temp_discussions: Vec<DiscussionDbSchema> = nodes
                        .into_iter()
                        .flatten()
                        .filter_map(|n| DiscussionDbSchema::try_from(n).ok())
                        .collect();

                    discussions.append(&mut temp_discussions);

                    if !repository.discussions.page_info.has_next_page {
                        break;
                    }
                    end_cur = repository.discussions.page_info.end_cursor;
                    continue;
                }
                end_cur = repository.discussions.page_info.end_cursor;
                continue;
            }
        }
        bail!("Failed to parse response data");
    }
    Ok(discussions)
}

fn request<V>(request_body: &QueryBody<V>, token: &str) -> Result<RequestBuilder>
where
    V: Serialize,
{
    let client = Client::builder().user_agent(APP_USER_AGENT).build()?;
    let client = client
        .post(GITHUB_URL)
        .bearer_auth(token)
        .json(&request_body);
    Ok(client)
}

async fn send_query<T>(token: &str, var: T::Variables) -> Result<Response>
where
    T: GraphQLQuery,
{
    Ok(request(&T::build_query(var), token)?.send().await?)
}

struct GitHubIssueResponse {
    issues: Vec<GitHubIssue>,
    has_next_page: bool,
    end_cursor: Option<String>,
}

impl TryFrom<GraphQlResponse<issues::ResponseData>> for GitHubIssueResponse {
    type Error = Error;

    fn try_from(value: GraphQlResponse<issues::ResponseData>) -> Result<Self> {
        let repo = value
            .data
            .context("You might send wrong request to GitHub.")?
            .repository
            .context("No repository was found. Check your request to GitHub.")?;
        let nodes = repo
            .issues
            .nodes
            .context("Repository exists, but there is no issue for the repository.")?;
        let issues = nodes
            .into_iter()
            .flatten()
            .map(GitHubIssue::try_from)
            .collect::<Result<Vec<_>>>()?;
        let has_next_page = repo.issues.page_info.has_next_page;
        let end_cursor = repo.issues.page_info.end_cursor;

        Ok(Self {
            issues,
            has_next_page,
            end_cursor,
        })
    }
}
