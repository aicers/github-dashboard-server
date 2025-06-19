use std::{sync::Arc, time::Duration};

use anyhow::{bail, Context, Result};
use graphql_client::{GraphQLQuery, QueryBody, Response as GraphQlResponse};
use jiff::Timestamp;
use reqwest::{Client, RequestBuilder, Response};
use serde::{Deserialize, Serialize};
use tokio::time;
use tracing::error;

use crate::{database::Database, github::{
    issues::IssuesRepositoryIssuesNodesAuthor::User as userName,
    pull_requests::PullRequestsRepositoryPullRequestsNodesReviewRequestsNodesRequestedReviewer::User,
}, settings::Repository as RepoInfo};

pub(crate) use pull_requests::{PullRequestState, PullRequestReviewState};

const GITHUB_FETCH_SIZE: i64 = 10;
const GITHUB_URL: &str = "https://api.github.com/graphql";
const APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"),);
const INIT_TIME: &str = "1992-06-05T00:00:00Z";

pub(crate) type DateTime = Timestamp;

#[allow(clippy::upper_case_acronyms)]
type URI = String;
#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/github/schema.graphql",
    query_path = "src/github/issues.graphql",
    response_derives = "Debug",
    scalar = "DateTime = DateTime"
)]
struct Issues;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/github/schema.graphql",
    query_path = "src/github/pull_requests.graphql",
    response_derives = "Debug, Clone, PartialEq, Eq",
    scalar = "DateTime = DateTime",
    scalar = "URI = URI"
)]
struct PullRequests;

#[derive(Debug)]
pub struct GitHubIssue {
    pub number: i64,
    pub title: String,
    pub author: String,
    pub closed_at: Option<DateTime>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryNode {
    pub owner: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubUserConnection {
    pub nodes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabelNode {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubLabelConnection {
    pub nodes: Vec<LabelNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommentNode {
    pub body: String,
    #[serde(rename = "createdAt")]
    pub created_at: DateTime,
    #[serde(rename = "updatedAt")]
    pub updated_at: DateTime,
    pub author: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubCommentConnection {
    #[serde(rename = "totalCount")]
    pub total_count: i32,
    pub nodes: Vec<CommentNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewNode {
    pub author: Option<String>,
    pub state: PullRequestReviewState,
    pub body: Option<String>,
    pub url: String,
    #[serde(rename = "createdAt")]
    pub created_at: DateTime,
    #[serde(rename = "publishedAt")]
    pub published_at: DateTime,
    #[serde(rename = "submittedAt")]
    pub submitted_at: DateTime,
    #[serde(rename = "isMinimized")]
    pub is_minimized: bool,
    pub comments: GitHubCommentConnection,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubReviewConnection {
    #[serde(rename = "totalCount")]
    pub total_count: i32,
    pub nodes: Vec<ReviewNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewRequestNode {
    #[serde(rename = "requestedReviewer")]
    pub requested_reviewer: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubReviewRequestConnection {
    pub nodes: Vec<ReviewRequestNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitPerson {
    pub user: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitInner {
    pub additions: i32,
    pub deletions: i32,
    pub message: String,
    #[serde(rename = "messageBody")]
    pub message_body: Option<String>,
    pub author: Option<CommitPerson>,
    #[serde(rename = "changedFilesIfAvailable")]
    pub changed_files_if_available: Option<i32>,
    #[serde(rename = "committedDate")]
    pub committed_date: DateTime,
    pub committer: Option<CommitPerson>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubCommitConnection {
    #[serde(rename = "totalCount")]
    pub total_count: i32,
    #[serde(rename = "nodes")]
    pub nodes: Vec<CommitInner>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GitHubPullRequests {
    pub id: String,
    pub number: i32,
    pub title: String,
    pub body: Option<String>,
    pub state: PullRequestState,
    #[serde(rename = "createdAt")]
    pub created_at: DateTime,
    #[serde(rename = "updatedAt")]
    pub updated_at: DateTime,
    #[serde(rename = "closedAt")]
    pub closed_at: Option<DateTime>,
    #[serde(rename = "mergedAt")]
    pub merged_at: Option<DateTime>,
    pub author: Option<String>,
    pub additions: i32,
    pub deletions: i32,
    pub url: String,
    pub repository: RepositoryNode,
    pub labels: GitHubLabelConnection,
    pub comments: GitHubCommentConnection,
    #[serde(rename = "reviewDecision")]
    pub review_decision: Option<PullRequestReviewState>,
    pub assignees: GitHubUserConnection,
    #[serde(rename = "reviewRequests")]
    pub review_requests: GitHubReviewRequestConnection,
    pub reviews: GitHubReviewConnection,
    pub commits: GitHubCommitConnection,
}

pub async fn fetch_periodically(
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
                        for resp in resps {
                            if let Err(error) =
                                db.insert_issues(resp, &repoinfo.owner, &repoinfo.name)
                            {
                                error!("Problem while insert Sled Database. {}", error);
                            }
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
                        for resp in resps {
                            if let Err(error) =
                                db.insert_pull_requests(resp, &repoinfo.owner, &repoinfo.name)
                            {
                                error!("Problem while insert Sled Database. {}", error);
                            }
                        }
                        break;
                    }
                    Err(error) => {
                        error!("Problem while sending github pr query. Query retransmission is done after 5 minutes. {}", error);
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
) -> Result<Vec<Vec<GitHubIssue>>> {
    let mut total_issue = Vec::new();
    let mut end_cur: Option<String> = None;
    let mut issues: Vec<GitHubIssue> = Vec::new();
    loop {
        let since_dt = since
            .parse::<DateTime>()
            .context("Failed to parse since date")?;
        let var = issues::Variables {
            owner: owner.to_string(),
            name: name.to_string(),
            first: Some(GITHUB_FETCH_SIZE),
            last: None,
            before: None,
            after: end_cur,
            since: Some(since_dt),
        };
        let resp_body: GraphQlResponse<issues::ResponseData> =
            send_query::<Issues>(token, var).await?.json().await?;
        if let Some(data) = resp_body.data {
            if let Some(repository) = data.repository {
                if let Some(nodes) = repository.issues.nodes.as_ref() {
                    for issue in nodes.iter().flatten() {
                        let mut author: String = String::new();

                        let author_ref = issue.author.as_ref().context("Missing issue author")?;
                        if let userName(on_user) = author_ref {
                            author.clone_from(&on_user.login.clone());
                        }
                        issues.push(GitHubIssue {
                            number: issue.number,
                            title: issue.title.to_string(),
                            author,
                            closed_at: issue.closed_at,
                        });
                    }
                    if !repository.issues.page_info.has_next_page {
                        total_issue.push(issues);
                        break;
                    }
                    end_cur = repository.issues.page_info.end_cursor;
                    continue;
                }
            }
        }
        bail!("Failed to parse response data");
    }
    Ok(total_issue)
}

async fn send_github_pr_query(
    owner: &str,
    name: &str,
    token: &str,
) -> Result<Vec<Vec<GitHubPullRequests>>> {
    let mut total_prs = Vec::new();
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
            if let Some(repo) = data.repository {
                let mut batch = Vec::new();

                if let Some(nodes) = repo.pull_requests.nodes {
                    for pr in nodes.into_iter().flatten() {
                        let mut assignees_list = Vec::new();
                        if let Some(ass_nodes) = pr.assignees.nodes {
                            for node in ass_nodes.into_iter().flatten() {
                                assignees_list.push(node.login);
                            }
                        }
                        let assignees_conn = GitHubUserConnection {
                            nodes: assignees_list,
                        };

                        let mut rr_nodes = Vec::new();
                        if let Some(req_conn) = pr.review_requests {
                            if let Some(req_nodes) = req_conn.nodes {
                                for rr in req_nodes.into_iter().flatten() {
                                    if let Some(User(user_node)) = rr.requested_reviewer {
                                        rr_nodes.push(ReviewRequestNode {
                                            requested_reviewer: Some(user_node.login),
                                        });
                                    }
                                }
                            }
                        }
                        let requests_conn = GitHubReviewRequestConnection { nodes: rr_nodes };

                        let record = GitHubPullRequests {
                            number: i32::try_from(pr.number)
                                .context("pull request number out of i32 range")?,
                            title: pr.title,
                            state: pr.state.clone(),
                            assignees: assignees_conn,
                            review_requests: requests_conn,
                            ..Default::default()
                        };
                        batch.push(record);
                    }
                }

                total_prs.push(batch);
                if !repo.pull_requests.page_info.has_next_page {
                    break;
                }
                end_cur = repo.pull_requests.page_info.end_cursor;
                continue;
            }
        }

        bail!("Failed to parse response data");
    }

    Ok(total_prs)
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
