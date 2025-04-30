use std::{sync::Arc, time::Duration};

use anyhow::{bail, Context, Result};
use chrono::Utc;
use graphql_client::{GraphQLQuery, QueryBody, Response as GraphQlResponse};
use reqwest::{Client, RequestBuilder, Response};
use serde::Serialize;
use tokio::time;
use tracing::error;

use crate::{database::Database, github::{
    issues::IssuesRepositoryIssuesNodesAuthor::User as userName,
    pull_requests::PullRequestsRepositoryPullRequestsNodesReviewRequestsNodesRequestedReviewer::User,
}, settings::Repository as RepoInfo};

const GITHUB_FETCH_SIZE: i64 = 10;
const GITHUB_URL: &str = "https://api.github.com/graphql";
const APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"),);
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
        if let Err(e) = db.insert_db("since", format!("{:?}", Utc::now())) {
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
        let var = issues::Variables {
            owner: owner.to_string(),
            name: name.to_string(),
            first: Some(GITHUB_FETCH_SIZE),
            last: None,
            before: None,
            after: end_cur,
            since: Some(since.to_string()),
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
            after: end_cur,
        };

        let resp_body: GraphQlResponse<pull_requests::ResponseData> =
            send_query::<PullRequests>(token, var).await?.json().await?;
        if let Some(data) = resp_body.data {
            if let Some(repository) = data.repository {
                if let Some(nodes) = repository.pull_requests.nodes.as_ref() {
                    for pr in nodes.iter().flatten() {
                        let mut assignees: Vec<String> = Vec::new();
                        let mut reviewers: Vec<String> = Vec::new();

                        if let Some(assignees_nodes) = pr.assignees.nodes.as_ref() {
                            for pr_assignees in assignees_nodes.iter().flatten() {
                                assignees.push(pr_assignees.login.clone());
                            }
                        }
                        if let Some(reviewers_nodes) =
                            pr.review_requests.as_ref().and_then(|r| r.nodes.as_ref())
                        {
                            for pr_reviewers in reviewers_nodes.iter().flatten() {
                                if let Some(User(on_user)) =
                                    pr_reviewers.requested_reviewer.as_ref()
                                {
                                    reviewers.push(on_user.login.clone());
                                }
                            }
                        }

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
                end_cur = repository.pull_requests.page_info.end_cursor;
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
