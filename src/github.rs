use std::{sync::Arc, time::Duration};

use anyhow::{bail, Context, Result};
use graphql_client::{GraphQLQuery, QueryBody, Response as GraphQlResponse};
use jiff::Timestamp;
use reqwest::{Client, RequestBuilder, Response};
use serde::{Deserialize, Serialize};
use tokio::time;
use tracing::error;

use crate::{
    database::Database,
    github::{
        issues::{
            IssueState, IssuesRepositoryIssuesNodesAuthor::User as IssueAuthor,
            IssuesRepositoryIssuesNodesClosedByPullRequestsReferencesEdgesNodeAuthor::User as PullRequestRefAuthor,
            IssuesRepositoryIssuesNodesCommentsNodesAuthor::User as IssueCommentsAuthor,
            IssuesRepositoryIssuesNodesProjectItemsNodesTodoInitiationOption as TodoInitOption,
            IssuesRepositoryIssuesNodesProjectItemsNodesTodoPendingDays as TodoPendingDays,
            IssuesRepositoryIssuesNodesProjectItemsNodesTodoPriority as TodoPriority,
            IssuesRepositoryIssuesNodesProjectItemsNodesTodoSize as TodoSize,
            IssuesRepositoryIssuesNodesProjectItemsNodesTodoStatus as TodoStatus,
            IssuesRepositoryIssuesNodesSubIssuesNodesAuthor::User as SubIssueAuthor,
        },
        pull_requests::PullRequestsRepositoryPullRequestsNodesReviewRequestsNodesRequestedReviewer::User,
    },
    graphql::{
        issue::{
            Comment, CommentConnection, ParentIssue, ProjectV2Item, ProjectV2ItemConnection,
            PullRequestRef, SubIssue,
        },
        DateTimeUtc,
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
    schema_path = "src/github/schema.graphql",
    query_path = "src/github/issues.graphql",
    response_derives = "Debug, Clone"
)]
pub(crate) struct Issues;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/github/schema.graphql",
    query_path = "src/github/pull_requests.graphql"
)]
struct PullRequests;

#[allow(clippy::derivable_impls)]
impl Default for IssueState {
    fn default() -> Self {
        IssueState::OPEN
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub(super) struct GitHubIssue {
    pub(super) id: String,
    pub(super) number: i32,
    pub(super) title: String,
    pub(super) author: String,
    pub(super) body: String,
    pub(super) state: IssueState,
    pub(super) assignees: Vec<String>,
    pub(super) labels: Vec<String>,
    pub(super) comments: CommentConnection,
    pub(super) project_items: ProjectV2ItemConnection,
    pub(super) sub_issues: Vec<SubIssue>,
    pub(super) parent: Option<ParentIssue>,
    pub(super) url: String,
    pub(super) closed_by_pull_requests: Vec<PullRequestRef>,
    pub(super) created_at: DateTimeUtc,
    pub(super) updated_at: DateTimeUtc,
    pub(super) closed_at: Option<DateTimeUtc>,
}

#[derive(Debug)]
pub(super) struct GitHubPullRequests {
    pub(super) number: i32,
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

#[allow(clippy::too_many_lines)]
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
            after: end_cur.take(),
            since: Some(since.parse::<DateTime>()?),
        };
        let resp_body: GraphQlResponse<issues::ResponseData> =
            send_query::<Issues>(token, var).await?.json().await?;
        if let Some(data) = resp_body.data {
            if let Some(repository) = data.repository {
                let nodes = repository.issues.nodes.unwrap_or_default();
                for issue in nodes.into_iter().flatten() {
                    let author = match issue.author.context("Missing issue author")? {
                        IssueAuthor(u) => u.login,
                        _ => String::new(),
                    };
                    issues.push(GitHubIssue {
                        id: issue.id,
                        number: issue.number.try_into().unwrap_or_default(),
                        title: issue.title,
                        author,
                        body: issue.body,
                        state: issue.state,
                        assignees: issue
                            .assignees
                            .nodes
                            .unwrap_or_default()
                            .into_iter()
                            .flatten()
                            .map(|n| n.login)
                            .collect(),
                        labels: issue
                            .labels
                            .and_then(|l| l.nodes)
                            .unwrap_or_default()
                            .into_iter()
                            .flatten()
                            .map(|node| node.name)
                            .collect(),
                        comments: CommentConnection {
                            total_count: issue.comments.total_count.try_into().unwrap_or_default(),
                            nodes: issue
                                .comments
                                .nodes
                                .unwrap_or_default()
                                .into_iter()
                                .flatten()
                                .map(|comment| Comment {
                                    author: match comment.author {
                                        Some(IssueCommentsAuthor(u)) => u.login,
                                        _ => String::new(),
                                    },
                                    body: comment.body,
                                    created_at: DateTimeUtc(comment.created_at),
                                    id: comment.id,
                                    repository_name: comment.repository.name,
                                    updated_at: DateTimeUtc(comment.updated_at),
                                    url: comment.url,
                                })
                                .collect(),
                        },
                        project_items: ProjectV2ItemConnection {
                            total_count: issue
                                .project_items
                                .total_count
                                .try_into()
                                .unwrap_or_default(),
                            nodes: issue
                                .project_items
                                .nodes
                                .unwrap_or_default()
                                .into_iter()
                                .flatten()
                                .map(|node| ProjectV2Item {
                                    id: node.id,
                                    todo_status: node.todo_status.and_then(|status| match status {
                                        TodoStatus::ProjectV2ItemFieldSingleSelectValue(inner) => {
                                            inner.name
                                        }
                                        _ => None,
                                    }),
                                    todo_priority: node.todo_priority.and_then(|priority| {
                                        match priority {
                                            TodoPriority::ProjectV2ItemFieldSingleSelectValue(
                                                inner,
                                            ) => inner.name,
                                            _ => None,
                                        }
                                    }),
                                    todo_size: node.todo_size.and_then(|size| match size {
                                        TodoSize::ProjectV2ItemFieldSingleSelectValue(inner) => {
                                            inner.name
                                        }
                                        _ => None,
                                    }),
                                    todo_initiation_option: node.todo_initiation_option.and_then(
                                        |init| match init {
                                            TodoInitOption::ProjectV2ItemFieldSingleSelectValue(
                                                inner,
                                            ) => inner.name,
                                            _ => None,
                                        },
                                    ),
                                    todo_pending_days: node.todo_pending_days.and_then(|days| {
                                        match days {
                                            TodoPendingDays::ProjectV2ItemFieldNumberValue(
                                                inner,
                                            ) => inner.number,
                                            _ => None,
                                        }
                                    }),
                                })
                                .collect(),
                        },
                        sub_issues: issue
                            .sub_issues
                            .nodes
                            .into_iter()
                            .flatten()
                            .flatten()
                            .map(|si| SubIssue {
                                id: si.id,
                                number: si.number.try_into().unwrap_or_default(),
                                title: si.title,
                                state: si.state,
                                created_at: DateTimeUtc(si.created_at),
                                updated_at: DateTimeUtc(si.updated_at),
                                closed_at: si.closed_at.map(DateTimeUtc),
                                author: match si.author {
                                    Some(SubIssueAuthor(u)) => u.login,
                                    _ => String::new(),
                                },
                                assignees: si
                                    .assignees
                                    .nodes
                                    .into_iter()
                                    .flatten()
                                    .flatten()
                                    .map(|n| n.login)
                                    .collect(),
                            })
                            .collect(),
                        parent: issue.parent.map(|parent| ParentIssue {
                            id: parent.id,
                            number: parent.number.try_into().unwrap_or_default(),
                            title: parent.title,
                        }),
                        url: issue.url,
                        closed_by_pull_requests: issue
                            .closed_by_pull_requests_references
                            .map(|r| r.edges)
                            .unwrap_or_default()
                            .into_iter()
                            .flatten()
                            .flatten()
                            .filter_map(|edge| {
                                edge.node.map(|node| PullRequestRef {
                                    number: node.number.try_into().unwrap_or_default(),
                                    state: node.state,
                                    created_at: DateTimeUtc(node.created_at),
                                    updated_at: DateTimeUtc(node.updated_at),
                                    closed_at: node.closed_at.map(DateTimeUtc),
                                    author: match node.author {
                                        Some(PullRequestRefAuthor(u)) => u.login,
                                        _ => String::new(),
                                    },
                                    url: node.url,
                                })
                            })
                            .collect(),
                        created_at: DateTimeUtc(issue.created_at),
                        updated_at: DateTimeUtc(issue.updated_at),
                        closed_at: issue.closed_at.map(DateTimeUtc),
                    });
                }
                if !repository.issues.page_info.has_next_page {
                    total_issue.push(issues);
                    break;
                }
                end_cur = repository.issues.page_info.end_cursor;
                continue;
            }
            bail!("Failed to parse response data");
        }
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
                            number: pr.number.try_into().unwrap_or_default(),
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
