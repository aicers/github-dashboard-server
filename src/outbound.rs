use std::{sync::Arc, time::Duration};

use anyhow::{bail, Context, Error, Result};
use graphql_client::{GraphQLQuery, QueryBody, Response as GraphQlResponse};
use jiff::Timestamp;
use reqwest::{Client, RequestBuilder, Response};
use serde::Serialize;
use tokio::time;
use tracing::error;

use crate::database::DiscussionDbSchema;
use crate::{
    database::{issue::GitHubIssue, pull_request::GitHubPullRequestNode, Database},
    outbound::issues::IssueState,
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

        // TODO: Use `let` chain instead of nested `if let Some` after migrating to Rust 2024
        if let Some(data) = resp_body.data {
            if let Some(repo) = data.repository {
                if let Some(nodes) = repo.pull_requests.nodes {
                    {
                        let mut dropped = 0usize;
                        prs.extend(nodes.into_iter().flatten().filter_map(|n| {
                            match GitHubPullRequestNode::try_from(n) {
                                Ok(pr) => Some(pr),
                                Err(e) => {
                                    tracing::warn!("Dropping PR node due to conversion error: {e}");
                                    dropped += 1;
                                    None
                                }
                            }
                        }));
                        if dropped > 0 {
                            tracing::debug!("Dropped {dropped} PR nodes from this page");
                        }

                        if !repo.pull_requests.page_info.has_next_page {
                            break;
                        }

                        end_cur = repo.pull_requests.page_info.end_cursor;
                    }
                }
            }
        }
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
