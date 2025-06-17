use std::{sync::Arc, time::Duration};

use anyhow::{bail, Context, Result};
use chrono::Utc;
use graphql_client::{GraphQLQuery, QueryBody, Response as GraphQlResponse};
use reqwest::{Client, RequestBuilder, Response};
use serde::{Deserialize, Serialize};
use tokio::time;
use tracing::error;

use crate::{
    database::Database,
    github::{
        issues::IssuesRepositoryIssuesNodesAuthor::User as userName,
        pull_requests::PullRequestsRepositoryPullRequestsNodesReviewRequestsNodesRequestedReviewer::User,
    },
    settings::Repository as RepoInfo,
};

const GITHUB_FETCH_SIZE: i64 = 10;
const GITHUB_URL: &str = "https://api.github.com/graphql";
const APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));
const INIT_TIME: &str = "1992-06-05T00:00:00Z";

type DateTime = String;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/github/schema.graphql",
    query_path = "src/github/issues.graphql",
    response_derives = "Debug"
)]
struct Issues;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/github/schema.graphql",
    query_path = "src/github/pull_requests.graphql"
)]
struct PullRequests;

#[derive(Debug)]
pub(super) struct GitHubIssue {
    pub(super) number: i64,
    pub(super) title: String,
    pub(super) author: String,
    pub(super) closed_at: Option<DateTime>,
}

#[derive(Debug)]
pub(super) struct GitHubPullRequests {
    pub(super) number: i64,
    pub(super) title: String,
    pub(super) assignees: Vec<String>,
    pub(super) reviewers: Vec<String>,
}

pub async fn fetch_issues(owner: &str, name: &str, token: &str) -> Result<Vec<(String, String)>> {
    let query = r"
    query($owner: String!, $name: String!, $since: DateTime!) {
      repository(owner: $owner, name: $name) {
        issues(first: 100, filterBy: {since: $since}) {
          nodes { id title body createdAt author { login } }
        }
      }
    }
    ";

    let vars = serde_json::json!({
        "owner": owner,
        "name": name,
        "since": "2024-05-01T00:00:00Z"
    });

    let client = Client::builder().user_agent(APP_USER_AGENT).build()?;

    let resp = client
        .post(GITHUB_URL)
        .bearer_auth(token)
        .json(&serde_json::json!({ "query": query, "variables": vars }))
        .send()
        .await
        .context("GitHub GraphQL request failed")?
        .error_for_status()
        .context("GitHub returned error status")?
        .json::<LightweightGraphQLResponse>()
        .await
        .context("Parsing GitHub JSON failed")?;

    let mut out = Vec::new();
    if let Some(data) = resp.data {
        if let Some(repo) = data.repository {
            for issue in repo.issues.nodes {
                let body = issue.body.unwrap_or_default();
                let author = issue
                    .author
                    .and_then(|a| a.login)
                    .unwrap_or_else(|| "unknown".to_string());
                let text = format!("Author: {}\nTitle: {}\n\n{}", author, issue.title, body);
                out.push((issue.id, text));
            }
        }
    }
    Ok(out)
}

#[derive(Deserialize)]
struct LightweightGraphQLResponse {
    data: Option<LightweightGraphQlData>,
}

#[derive(Deserialize)]
struct LightweightGraphQlData {
    repository: Option<LightweightRepoIssues>,
}

#[derive(Deserialize)]
struct LightweightRepoIssues {
    issues: LightweightIssueConnection,
}

#[derive(Deserialize)]
struct LightweightIssueConnection {
    nodes: Vec<LightweightIssueNode>,
}

#[derive(Deserialize)]
struct LightweightIssueNode {
    id: String,
    title: String,
    body: Option<String>,
    author: Option<LightweightIssueAuthor>,
}

#[derive(Deserialize)]
struct LightweightIssueAuthor {
    login: Option<String>,
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
        let since = db
            .select_db("since")
            .unwrap_or_else(|_| INIT_TIME.to_string());
        let _ = db.insert_db("since", format!("{:?}", Utc::now()));

        for repoinfo in repositories.iter() {
            let mut re_itv = time::interval(retry);
            loop {
                re_itv.tick().await;
                match send_github_issue_query(&repoinfo.owner, &repoinfo.name, &since, &token).await
                {
                    Ok(resps) => {
                        for resp in resps {
                            let _ = db.insert_issues(resp, &repoinfo.owner, &repoinfo.name);
                        }
                        break;
                    }
                    Err(error) => {
                        error!("Retrying GitHub issue query in 5 min: {}", error);
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
                            let _ = db.insert_pull_requests(resp, &repoinfo.owner, &repoinfo.name);
                        }
                        break;
                    }
                    Err(error) => {
                        error!("Retrying GitHub PR query in 5 min: {}", error);
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
        let var = issues::Variables {
            owner: owner.to_string(),
            name: name.to_string(),
            first: Some(GITHUB_FETCH_SIZE),
            last: None,
            before: None,
            after: end_cur.clone(),
            since: Some(since.to_string()),
        };
        let resp_body: GraphQlResponse<issues::ResponseData> =
            send_query::<Issues>(token, var).await?.json().await?;
        if let Some(data) = resp_body.data {
            if let Some(repository) = data.repository {
                if let Some(nodes) = repository.issues.nodes.as_ref() {
                    for issue in nodes.iter().flatten() {
                        let author_ref = issue.author.as_ref().context("Missing issue author")?;
                        let author = if let userName(on_user) = author_ref {
                            on_user.login.clone()
                        } else {
                            "unknown".to_string()
                        };
                        issues.push(GitHubIssue {
                            number: issue.number,
                            title: issue.title.to_string(),
                            author,
                            closed_at: issue.closed_at.clone(),
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
    let mut prs: Vec<GitHubPullRequests> = Vec::new();
    loop {
        let var = pull_requests::Variables {
            owner: owner.to_string(),
            name: name.to_string(),
            first: Some(GITHUB_FETCH_SIZE),
            last: None,
            before: None,
            after: end_cur.clone(),
        };
        let resp_body: GraphQlResponse<pull_requests::ResponseData> =
            send_query::<PullRequests>(token, var).await?.json().await?;
        if let Some(data) = resp_body.data {
            if let Some(repository) = data.repository {
                if let Some(nodes) = repository.pull_requests.nodes.as_ref() {
                    for pr in nodes.iter().flatten() {
                        let assignees = pr
                            .assignees
                            .nodes
                            .as_ref()
                            .map(|nodes| {
                                nodes
                                    .iter()
                                    .flatten()
                                    .map(|u| u.login.clone())
                                    .collect::<Vec<_>>()
                            })
                            .unwrap_or_default();
                        let reviewers = pr
                            .review_requests
                            .as_ref()
                            .and_then(|r| r.nodes.as_ref())
                            .map(|nodes| {
                                nodes
                                    .iter()
                                    .flatten()
                                    .filter_map(|r| {
                                        r.requested_reviewer.as_ref().and_then(|u| {
                                            if let User(on_user) = u {
                                                Some(on_user.login.clone())
                                            } else {
                                                None
                                            }
                                        })
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();

                        prs.push(GitHubPullRequests {
                            number: pr.number,
                            title: pr.title.to_string(),
                            assignees,
                            reviewers,
                        });
                    }
                    if !repository.pull_requests.page_info.has_next_page {
                        total_prs.push(prs);
                        break;
                    }
                    end_cur = repository.pull_requests.page_info.end_cursor;
                    continue;
                }
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
    Ok(client
        .post(GITHUB_URL)
        .bearer_auth(token)
        .json(request_body))
}

async fn send_query<T>(token: &str, var: T::Variables) -> Result<Response>
where
    T: GraphQLQuery,
{
    Ok(request(&T::build_query(var), token)?.send().await?)
}
