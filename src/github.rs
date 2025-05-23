use std::{sync::Arc, time::Duration};

use anyhow::Result;
use chrono::Utc;
use graphql_client::{GraphQLQuery, QueryBody, Response as GraphQlResponse};
use reqwest::{Client, RequestBuilder, Response};
use serde::Serialize;
use tokio::time;
use tracing::error;

use crate::graphql::{Issue, PullRequest};
use crate::{conf::RepoInfo, database::Database};

const GITHUB_FETCH_SIZE: i64 = 10;
const GITHUB_URL: &str = "https://api.github.com/graphql";
const APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"),);
const INIT_TIME: &str = "1992-06-05T00:00:00Z";

type DateTime = String;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/github_schema.graphql",
    query_path = "src/open_issues.graphql",
    response_derives = "Debug"
)]
pub(crate) struct OpenIssues;

impl open_issues::ResponseData {
    pub(crate) fn collect_issues(self) -> Vec<Issue> {
        self.repository
            .and_then(|repository| {
                repository
                    .issues
                    .nodes
                    .map(|nodes| nodes.into_iter().flatten().map(Issue::from).collect())
            })
            .unwrap_or_default()
    }

    pub(crate) fn has_next_page(&self) -> Option<String> {
        self.repository.as_ref().and_then(|repository| {
            if repository.issues.page_info.has_next_page {
                repository.issues.page_info.end_cursor.clone()
            } else {
                None
            }
        })
    }
}

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/github_schema.graphql",
    query_path = "src/pull_requests.graphql"
)]
pub(crate) struct PullRequests;

impl pull_requests::ResponseData {
    pub(crate) fn collect_pull_requests(self) -> Vec<PullRequest> {
        self.repository
            .and_then(|repository| {
                repository
                    .pull_requests
                    .nodes
                    .map(|nodes| nodes.into_iter().flatten().map(PullRequest::from).collect())
            })
            .unwrap_or_default()
    }

    pub(crate) fn has_next_page(&self) -> Option<String> {
        self.repository.as_ref().and_then(|repository| {
            if repository.pull_requests.page_info.has_next_page {
                repository.pull_requests.page_info.end_cursor.clone()
            } else {
                None
            }
        })
    }
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
        let last_time = match db.select_db("last_time") {
            Ok(r) => r,
            Err(_) => INIT_TIME.to_string(),
        };
        if let Err(e) = db.insert_db("last_time", format!("{:?}", Utc::now())) {
            error!("Insert DateTime Error: {}", e);
        }

        for repo in repositories.iter() {
            let mut re_itv = time::interval(retry);
            loop {
                re_itv.tick().await;
                match send_github_issue_query(&repo.owner, &repo.name, &last_time, &token).await {
                    Ok(issues) => {
                        if let Err(error) = db.insert_issues(issues, &repo.owner, &repo.name) {
                            error!("Problem while insert Sled Database. {}", error);
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
                match send_github_pr_query(&repo.owner, &repo.name, &token).await {
                    Ok(prs) => {
                        if let Err(error) = db.insert_pull_requests(prs, &repo.owner, &repo.name) {
                            error!("Problem while insert Sled Database. {}", error);
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
    last_time: &str,
    token: &str,
) -> Result<Vec<Issue>> {
    let mut end_cur: Option<String> = None;
    let mut issues: Vec<Issue> = Vec::new();
    loop {
        let var = open_issues::Variables {
            owner: owner.to_string(),
            name: name.to_string(),
            first: Some(GITHUB_FETCH_SIZE),
            last: None,
            before: None,
            after: end_cur,
            lasttime: last_time.to_string(),
        };
        let response: GraphQlResponse<open_issues::ResponseData> =
            send_query::<OpenIssues>(token, var).await?.json().await?;

        end_cur = response
            .data
            .as_ref()
            .and_then(open_issues::ResponseData::has_next_page);
        issues.extend(
            response
                .data
                .map(open_issues::ResponseData::collect_issues)
                .unwrap_or_default(),
        );
        if end_cur.is_none() {
            break;
        }
    }
    Ok(issues)
}

async fn send_github_pr_query(owner: &str, name: &str, token: &str) -> Result<Vec<PullRequest>> {
    let mut end_cur: Option<String> = None;
    let mut prs: Vec<PullRequest> = Vec::new();
    loop {
        let var = pull_requests::Variables {
            owner: owner.to_string(),
            name: name.to_string(),
            first: Some(GITHUB_FETCH_SIZE),
            last: None,
            before: None,
            after: end_cur,
        };
        let response: GraphQlResponse<pull_requests::ResponseData> =
            send_query::<PullRequests>(token, var).await?.json().await?;

        end_cur = response
            .data
            .as_ref()
            .and_then(pull_requests::ResponseData::has_next_page);
        prs.extend(
            response
                .data
                .map(pull_requests::ResponseData::collect_pull_requests)
                .unwrap_or_default(),
        );
        if end_cur.is_none() {
            break;
        }
    }
    Ok(prs)
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
