use anyhow::{bail, Result};
use graphql_client::{GraphQLQuery, QueryBody, Response as GraphQlResponse};
use reqwest::{Client, RequestBuilder, Response};
use serde::Serialize;

const GITHUB_FETCH_SIZE: i64 = 10;
const GITHUB_URL: &str = "https://api.github.com/graphql";
const APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"),);

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/github_schema.graphql",
    query_path = "src/open_issues.graphql",
    response_derives = "Debug"
)]
pub struct OpenIssues;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/github_schema.graphql",
    query_path = "src/pull_requests.graphql"
)]
pub struct PullRequests;

#[derive(Debug)]
pub struct GitHubIssue {
    pub number: i64,
    pub title: String,
}

#[derive(Debug)]
pub struct GitHubPRs {
    pub number: i64,
    pub title: String,
}

pub async fn send_github_issue_query(
    owner: &str,
    names: &Vec<String>,
    token: &str,
) -> Result<Vec<Vec<GitHubIssue>>> {
    let mut total_issue = Vec::new();
    for name in names {
        let mut end_cur: Option<String> = None;
        let mut issues: Vec<GitHubIssue> = Vec::new();
        loop {
            let var = open_issues::Variables {
                owner: owner.to_string(),
                name: name.to_string(),
                first: Some(GITHUB_FETCH_SIZE),
                last: None,
                before: None,
                after: end_cur,
            };
            let resp_body: GraphQlResponse<open_issues::ResponseData> =
                send_query::<OpenIssues>(token, var).await?.json().await?;
            if let Some(data) = resp_body.data {
                if let Some(repository) = data.repository {
                    if let Some(nodes) = repository.issues.nodes.as_ref() {
                        for issue in nodes.iter().flatten() {
                            issues.push(GitHubIssue {
                                number: issue.number,
                                title: issue.title.to_string(),
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
    }
    Ok(total_issue)
}

pub async fn send_github_pr_query(
    owner: &str,
    names: &Vec<String>,
    token: &str,
) -> Result<Vec<Vec<GitHubPRs>>> {
    let mut total_prs = Vec::new();
    for name in names {
        let mut end_cur: Option<String> = None;
        let mut prs: Vec<GitHubPRs> = Vec::new();
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
                            prs.push(GitHubPRs {
                                number: pr.number,
                                title: pr.title.to_string(),
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
