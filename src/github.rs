use std::{collections::HashMap, sync::Arc, time::Duration};

use anyhow::{bail, Context, Result};
use chrono::Utc;
use graphql_client::{GraphQLQuery, QueryBody, Response as GraphQlResponse};
use langchain_rust::{schemas::Document, vectorstore::VecStoreOptions};
use reqwest::{Client, RequestBuilder, Response};
use serde::Serialize;
use serde_json::{json, to_string};
use tokio::{sync::Mutex, time};
use tracing::{error, info};

use crate::{database::{ Database, DiscussionDbSchema}, github::{
    issues::IssuesRepositoryIssuesNodesAuthor::User as userName,
    pull_requests::PullRequestsRepositoryPullRequestsNodesReviewRequestsNodesRequestedReviewer::User,
}, rag_sample::RagOllamaSystem, settings::Repository as RepoInfo};

const GITHUB_FETCH_SIZE: i64 = 10;
const GITHUB_URL: &str = "https://api.github.com/graphql";
const APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"),);
const INIT_TIME: &str = "1992-06-05T00:00:00Z";

type DateTime = String;

#[derive(GraphQLQuery, Serialize)]
#[graphql(
    schema_path = "src/github/schema.graphql",
    query_path = "src/github/issues.graphql",
    response_derives = "Debug"
)]
struct Issues;

#[derive(GraphQLQuery, Serialize)]
#[graphql(
    schema_path = "src/github/schema.graphql",
    query_path = "src/github/pull_requests.graphql"
)]
struct PullRequests;

#[allow(clippy::upper_case_acronyms)]
type URI = String;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/github/schema.graphql",
    query_path = "src/github/discussions.graphql"
)]
pub(crate) struct Discussions;

pub trait GithubData {
    fn get_number(&self) -> i64;
    fn get_author(&self) -> String;
}

#[derive(Debug, Serialize)]
pub(super) struct GitHubIssue {
    pub(super) number: i64,
    pub(super) title: String,
    pub(super) author: String,
    pub(super) closed_at: Option<DateTime>,
}

impl GithubData for GitHubIssue {
    fn get_number(&self) -> i64 {
        self.number
    }

    fn get_author(&self) -> String {
        self.author.clone()
    }
}

#[derive(Debug, Serialize)]
pub(super) struct GitHubPullRequests {
    pub(super) number: i64,
    pub(super) title: String,
    pub(super) assignees: Vec<String>,
    pub(super) reviewers: Vec<String>,
}

impl GithubData for GitHubPullRequests {
    fn get_number(&self) -> i64 {
        self.number
    }

    fn get_author(&self) -> String {
        String::default()
    }
}

#[allow(clippy::too_many_lines)]
pub(super) async fn fetch_periodically(
    repositories: Arc<Vec<RepoInfo>>,
    token: String,
    period: Duration,
    retry: Duration,
    db: Database,
    rag: Arc<Mutex<RagOllamaSystem>>,
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
                            add_documents_to_rag(
                                &rag,
                                &repoinfo.owner,
                                &repoinfo.name,
                                &resp,
                                "Issues",
                            )
                            .await;

                            if let Err(error) =
                                db.insert_issues(resp, &repoinfo.owner, &repoinfo.name)
                            {
                                error!("Problem while insert Sled Database. {}", error);
                            } else {
                                info!("Success Appending issues");
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
                            add_documents_to_rag(
                                &rag,
                                &repoinfo.owner,
                                &repoinfo.name,
                                &resp,
                                "Pull Requests",
                            )
                            .await;
                            if let Err(error) =
                                db.insert_pull_requests(resp, &repoinfo.owner, &repoinfo.name)
                            {
                                error!("Problem while insert Sled Database. {}", error);
                            } else {
                                info!("Success Appending Pull Requests");
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

            let mut re_itv = time::interval(retry);
            loop {
                re_itv.tick().await;
                match send_github_discussion_query(&repoinfo.owner, &repoinfo.name, &token).await {
                    Ok(resps) => {
                        for resp in resps {
                            add_documents_to_rag(
                                &rag,
                                &repoinfo.owner,
                                &repoinfo.name,
                                &resp,
                                "Discussions",
                            )
                            .await;
                            if let Err(error) =
                                db.insert_discussions(resp, &repoinfo.owner, &repoinfo.name)
                            {
                                error!("Problem while insert Sled Database. {}", error);
                            } else {
                                info!("Success Appending discussions");
                            }
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
        // {
        //     let mut rag_guard = rag.lock().await;
        //     rag_guard.set_chain();
        // }
    }
}

async fn add_documents_to_rag<T>(
    rag: &Arc<Mutex<RagOllamaSystem>>,
    owner: &String,
    name: &String,
    resp: &[T],
    doc_type: &str,
) where
    T: Serialize + GithubData,
{
    let mut rag_guard = rag.lock().await;
    let docs = resp
        .iter()
        .map(|item| {
            Document::new(to_string(item).unwrap()).with_metadata({
                let mut metadata = HashMap::new();
                metadata.insert("type".to_string(), json!(doc_type));
                metadata.insert("repo".to_string(), json!(format!("{}/{}", &owner, &name)));
                metadata.insert("number".to_string(), json!(item.get_number()));
                metadata.insert("author".to_string(), json!(item.get_author()));
                metadata
            })
        })
        .collect::<Vec<Document>>();
    let ids: Vec<String> = resp
        .iter()
        .map(|item| format!("{}/{}/{}", &owner, &name, item.get_number()))
        .collect();
    rag_guard
        .add_documents_with_ids(&docs, &ids, &VecStoreOptions::default())
        .await;

    info!("[RAG] Success Appending {} {}", docs.len(), doc_type);
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

async fn send_github_discussion_query(
    owner: &str,
    name: &str,
    token: &str,
) -> Result<Vec<Vec<DiscussionDbSchema>>> {
    use crate::database::discussion::DiscussionDbSchema;

    let mut total_discussions = Vec::new();
    let mut end_cur: Option<String> = None;
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
                    let discussions = nodes
                        .into_iter()
                        .flatten()
                        .map(DiscussionDbSchema::from)
                        .collect();

                    if !repository.discussions.page_info.has_next_page {
                        total_discussions.push(discussions);
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
    Ok(total_discussions)
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
