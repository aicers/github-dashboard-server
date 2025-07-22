use std::{sync::Arc, time::Duration};

use anyhow::{anyhow, bail, Result};
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
            IssueState,
            IssuesRepositoryIssuesNodes,
            IssuesRepositoryIssuesNodesAuthor,
            IssuesRepositoryIssuesNodesAssignees,
            IssuesRepositoryIssuesNodesLabels,
            IssuesRepositoryIssuesNodesComments,
            IssuesRepositoryIssuesNodesProjectItems,
            IssuesRepositoryIssuesNodesSubIssues,
            IssuesRepositoryIssuesNodesClosedByPullRequestsReferences,
            IssuesRepositoryIssuesNodesParent,
            IssuesRepositoryIssuesNodesAuthor::User as IssueAuthor,
            IssuesRepositoryIssuesNodesClosedByPullRequestsReferencesEdgesNodeAuthor::User as PullRequestRefAuthor,
            IssuesRepositoryIssuesNodesCommentsNodesAuthor::User as IssueCommentsAuthor,
            IssuesRepositoryIssuesNodesProjectItemsNodesTodoInitiationOption as TodoInitOption,
            IssuesRepositoryIssuesNodesProjectItemsNodesTodoPendingDays as TodoPendingDays,
            IssuesRepositoryIssuesNodesProjectItemsNodesTodoPriority as TodoPriority,
            IssuesRepositoryIssuesNodesProjectItemsNodesTodoSize as TodoSize,
            IssuesRepositoryIssuesNodesProjectItemsNodesTodoStatus as TodoStatus,
            IssuesRepositoryIssuesNodesSubIssuesNodesAuthor::User as SubIssueAuthor,
            PullRequestState,
        },
        pull_requests::PullRequestsRepositoryPullRequestsNodesReviewRequestsNodesRequestedReviewer::User,
    },
    settings::Repository as RepoInfo,
};
use crate::database::DiscussionDbSchema;

const GITHUB_FETCH_SIZE: i64 = 10;
const GITHUB_URL: &str = "https://api.github.com/graphql";
const APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"),);
const INIT_TIME: &str = "1992-06-05T00:00:00Z";

const GRAPHQL_ISSUE_NUMBER_ASSERTION: &str = r"
GraphQL field Issue.number is Int! type, thus always exist.
And it will not exceed 2^32.";
const GRAPHQL_PULL_REQUEST_NUMBER_ASSERTION: &str = r"
GraphQL field PullRequest.number is Int! type, thus always exist.
And it will not exceed 2^32.";
const GRAPHQL_ISSUE_CONNECTION_TOTAL_COUNT_ASSERTION: &str = r"
GraphQL field IssueConnection.totalCount is Int! type, thus always exist.
And it will not exceed 2^32.";

type DateTime = Timestamp;

#[allow(clippy::upper_case_acronyms)]
type URI = String;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/github/schema.graphql",
    query_path = "src/github/issues.graphql",
    response_derives = "Debug, Clone, PartialEq"
)]
pub(crate) struct Issues;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/github/schema.graphql",
    query_path = "src/github/pull_requests.graphql"
)]
struct PullRequests;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/github/schema.graphql",
    query_path = "src/github/discussions.graphql",
    response_derives = "Debug"
)]
pub(crate) struct Discussions;

#[allow(clippy::derivable_impls)]
impl Default for IssueState {
    fn default() -> Self {
        IssueState::OPEN
    }
}

#[derive(Debug, Default, Deserialize, Serialize, PartialEq)]
pub(super) struct GitHubIssue {
    pub(super) id: String,
    pub(super) number: i32,
    pub(super) title: String,
    pub(super) author: String,
    pub(super) body: String,
    pub(super) state: IssueState,
    pub(super) assignees: Vec<String>,
    pub(super) labels: Vec<String>,
    pub(super) comments: GitHubCommentConnection,
    pub(super) project_items: GitHubProjectV2ItemConnection,
    pub(super) sub_issues: GitHubSubIssueConnection,
    pub(super) parent: Option<GitHubParentIssue>,
    pub(super) url: String,
    pub(super) closed_by_pull_requests: Vec<GitHubPullRequestRef>,
    pub(super) created_at: Timestamp,
    pub(super) updated_at: Timestamp,
    pub(super) closed_at: Option<Timestamp>,
}

#[derive(Debug, Default, Deserialize, Serialize, PartialEq)]
pub(crate) struct GitHubCommentConnection {
    pub(crate) total_count: i32,
    pub(crate) nodes: Vec<GitHubComment>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub(crate) struct GitHubComment {
    pub(crate) id: String,
    pub(crate) author: String,
    pub(crate) body: String,
    pub(crate) created_at: Timestamp,
    pub(crate) updated_at: Timestamp,
    pub(crate) repository_name: String,
    pub(crate) url: String,
}

#[derive(Debug, Default, Deserialize, Serialize, PartialEq)]
pub(crate) struct GitHubProjectV2ItemConnection {
    pub(crate) total_count: i32,
    pub(crate) nodes: Vec<GitHubProjectV2Item>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub(crate) struct GitHubProjectV2Item {
    pub(crate) id: String,
    pub(crate) todo_status: Option<String>,
    pub(crate) todo_priority: Option<String>,
    pub(crate) todo_size: Option<String>,
    pub(crate) todo_initiation_option: Option<String>,
    pub(crate) todo_pending_days: Option<f64>,
}

#[derive(Debug, Default, Deserialize, Serialize, PartialEq)]
pub(crate) struct GitHubSubIssueConnection {
    pub(crate) total_count: i32,
    pub(crate) nodes: Vec<GitHubSubIssue>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub(crate) struct GitHubSubIssue {
    pub(crate) id: String,
    pub(crate) number: i32,
    pub(crate) title: String,
    pub(crate) state: IssueState,
    pub(crate) author: String,
    pub(crate) assignees: Vec<String>,
    pub(crate) created_at: Timestamp,
    pub(crate) updated_at: Timestamp,
    pub(crate) closed_at: Option<Timestamp>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub(crate) struct GitHubParentIssue {
    pub(crate) id: String,
    pub(crate) number: i32,
    pub(crate) title: String,
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub(crate) struct GitHubPullRequestRef {
    pub(crate) number: i32,
    pub(crate) state: PullRequestState,
    pub(crate) author: String,
    pub(crate) created_at: Timestamp,
    pub(crate) updated_at: Timestamp,
    pub(crate) closed_at: Option<Timestamp>,
    pub(crate) url: String,
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

            let mut re_itv = time::interval(retry);
            loop {
                re_itv.tick().await;
                match send_github_discussion_query(&repoinfo.owner, &repoinfo.name, &token).await {
                    Ok(resps) => {
                        if let Err(error) =
                            db.insert_discussions(resps, &repoinfo.owner, &repoinfo.name)
                        {
                            error!("Problem while insert Sled Database. {}", error);
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
) -> Result<Vec<Vec<GitHubIssue>>> {
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

        if !issue_resp.has_next_page {
            total_issue.push(issue_resp.issues);
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
                            number: pr
                                .number
                                .try_into()
                                .expect(GRAPHQL_PULL_REQUEST_NUMBER_ASSERTION),
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
    type Error = anyhow::Error;

    fn try_from(value: GraphQlResponse<issues::ResponseData>) -> anyhow::Result<Self> {
        let repo = value
            .data
            .expect("Response data should exist, although when it is empty or error.")
            .repository
            .ok_or(anyhow!("Wrong repository."))?;
        let nodes = repo
            .issues
            .nodes
            .expect("This field will be always returned even if no issue exist");

        let issues = nodes.into_iter().flatten().map(GitHubIssue::from).collect();
        let has_next_page = repo.issues.page_info.has_next_page;
        let end_cursor = repo.issues.page_info.end_cursor;

        Ok(Self {
            issues,
            has_next_page,
            end_cursor,
        })
    }
}

/// Convert one single *Issue* of GitHub GraphQL API to our internal data structure (`GitHubIssue`)
impl From<IssuesRepositoryIssuesNodes> for GitHubIssue {
    fn from(issue: IssuesRepositoryIssuesNodes) -> Self {
        Self {
            id: issue.id,
            number: issue
                .number
                .try_into()
                .expect(GRAPHQL_ISSUE_NUMBER_ASSERTION),
            title: issue.title,
            author: String::from(issue.author.expect("Author of GitHub issue always exist.")),
            body: issue.body,
            state: issue.state,
            assignees: Vec::<String>::from(issue.assignees),
            labels: issue.labels.map(Vec::<String>::from).unwrap_or_default(), // vec![]
            comments: GitHubCommentConnection::from(issue.comments),
            project_items: GitHubProjectV2ItemConnection::from(issue.project_items),
            sub_issues: GitHubSubIssueConnection::from(issue.sub_issues),
            parent: issue.parent.map(GitHubParentIssue::from),
            url: issue.url,
            closed_by_pull_requests: issue
                .closed_by_pull_requests_references
                .map(Vec::<GitHubPullRequestRef>::from)
                .unwrap_or_default(), // vec![]
            created_at: issue.created_at,
            updated_at: issue.updated_at,
            closed_at: issue.closed_at,
        }
    }
}

impl From<IssuesRepositoryIssuesNodesAuthor> for String {
    fn from(author: IssuesRepositoryIssuesNodesAuthor) -> Self {
        match author {
            IssueAuthor(user) => user.login,
            _ => String::new(),
        }
    }
}

impl From<IssuesRepositoryIssuesNodesAssignees> for Vec<String> {
    fn from(assignees: IssuesRepositoryIssuesNodesAssignees) -> Self {
        assignees
            .nodes
            .unwrap_or_default() // vec![]
            .into_iter()
            .flatten()
            .map(|user| user.login)
            .collect()
    }
}

impl From<IssuesRepositoryIssuesNodesLabels> for Vec<String> {
    fn from(labels: IssuesRepositoryIssuesNodesLabels) -> Self {
        labels
            .nodes
            .unwrap_or_default() // vec![]
            .into_iter()
            .flatten()
            .map(|label| label.name)
            .collect()
    }
}

impl From<IssuesRepositoryIssuesNodesComments> for GitHubCommentConnection {
    fn from(comments: IssuesRepositoryIssuesNodesComments) -> Self {
        Self {
            total_count: comments
                .total_count
                .try_into()
                .expect("Total count will not exceed 2^32."),
            nodes: comments
                .nodes
                .unwrap_or_default() // vec![]
                .into_iter()
                .flatten()
                .map(|comment| GitHubComment {
                    author: match comment.author {
                        Some(IssueCommentsAuthor(u)) => u.login,
                        _ => String::new(),
                    },
                    body: comment.body,
                    created_at: comment.created_at,
                    id: comment.id,
                    repository_name: comment.repository.name,
                    updated_at: comment.updated_at,
                    url: comment.url,
                })
                .collect(),
        }
    }
}

impl From<IssuesRepositoryIssuesNodesProjectItems> for GitHubProjectV2ItemConnection {
    fn from(project_items: IssuesRepositoryIssuesNodesProjectItems) -> Self {
        Self {
            total_count: project_items
                .total_count
                .try_into()
                .expect("totalCount will not exceed 2^32."),
            nodes: project_items
                .nodes
                .unwrap_or_default() // vec![]
                .into_iter()
                .flatten()
                .map(|node| GitHubProjectV2Item {
                    id: node.id,
                    todo_status: node.todo_status.and_then(|status| match status {
                        TodoStatus::ProjectV2ItemFieldSingleSelectValue(inner) => inner.name,
                        _ => None,
                    }),
                    todo_priority: node.todo_priority.and_then(|priority| match priority {
                        TodoPriority::ProjectV2ItemFieldSingleSelectValue(inner) => inner.name,
                        _ => None,
                    }),
                    todo_size: node.todo_size.and_then(|size| match size {
                        TodoSize::ProjectV2ItemFieldSingleSelectValue(inner) => inner.name,
                        _ => None,
                    }),
                    todo_initiation_option: node.todo_initiation_option.and_then(
                        |init| match init {
                            TodoInitOption::ProjectV2ItemFieldSingleSelectValue(inner) => {
                                inner.name
                            }
                            _ => None,
                        },
                    ),
                    todo_pending_days: node.todo_pending_days.and_then(|days| match days {
                        TodoPendingDays::ProjectV2ItemFieldNumberValue(inner) => inner.number,
                        _ => None,
                    }),
                })
                .collect(),
        }
    }
}

impl From<IssuesRepositoryIssuesNodesSubIssues> for GitHubSubIssueConnection {
    fn from(sub_issues: IssuesRepositoryIssuesNodesSubIssues) -> Self {
        Self {
            total_count: sub_issues
                .total_count
                .try_into()
                .expect(GRAPHQL_ISSUE_CONNECTION_TOTAL_COUNT_ASSERTION),
            nodes: sub_issues
                .nodes
                .unwrap_or_default() // vec![]
                .into_iter()
                .flatten()
                .map(|sub_issue| GitHubSubIssue {
                    id: sub_issue.id,
                    number: sub_issue
                        .number
                        .try_into()
                        .expect(GRAPHQL_ISSUE_NUMBER_ASSERTION),
                    title: sub_issue.title,
                    state: sub_issue.state,
                    created_at: sub_issue.created_at,
                    updated_at: sub_issue.updated_at,
                    closed_at: sub_issue.closed_at,
                    author: match sub_issue.author {
                        Some(SubIssueAuthor(u)) => u.login,
                        _ => String::new(),
                    },
                    assignees: sub_issue
                        .assignees
                        .nodes
                        .unwrap_or_default() // vec![]
                        .into_iter()
                        .flatten()
                        .map(|n| n.login)
                        .collect(),
                })
                .collect(),
        }
    }
}

impl From<IssuesRepositoryIssuesNodesClosedByPullRequestsReferences> for Vec<GitHubPullRequestRef> {
    fn from(closing_prs: IssuesRepositoryIssuesNodesClosedByPullRequestsReferences) -> Self {
        closing_prs
            .edges
            .unwrap_or_default() // vec![]
            .into_iter()
            .flatten()
            .filter_map(|edge| {
                edge.node.map(|node| GitHubPullRequestRef {
                    number: node
                        .number
                        .try_into()
                        .expect(GRAPHQL_PULL_REQUEST_NUMBER_ASSERTION),
                    state: node.state,
                    created_at: node.created_at,
                    updated_at: node.updated_at,
                    closed_at: node.closed_at,
                    author: match node.author {
                        Some(PullRequestRefAuthor(u)) => u.login,
                        _ => String::new(),
                    },
                    url: node.url,
                })
            })
            .collect()
    }
}

impl From<IssuesRepositoryIssuesNodesParent> for GitHubParentIssue {
    fn from(parent: IssuesRepositoryIssuesNodesParent) -> Self {
        Self {
            id: parent.id,
            number: parent
                .number
                .try_into()
                .expect(GRAPHQL_ISSUE_NUMBER_ASSERTION),
            title: parent.title,
        }
    }
}

#[cfg(test)]
mod tests {
    use graphql_client::Response as GraphQlResponse;

    use super::*;

    // Test with response where repository is wrong or does not exist
    #[test]
    fn convert_error_response_to_issue_() {
        let response_str = r#"
        {
            "data": {
                "repository": null
            },
            "errors": [
                {
                    "type": "NOT_FOUND",
                    "path": [
                        "repository"
                    ],
                    "locations": [
                        {
                            "line": 20,
                            "column": 3
                        }
                    ],
                    "message": "Could not resolve to a Repository with the name 'aicers/non-existing-repository'."
                }
            ]
        }"#;

        let graphql_response: GraphQlResponse<issues::ResponseData> =
            serde_json::from_str(response_str).expect("Valid JSON");
        let resp = GitHubIssueResponse::try_from(graphql_response);

        assert!(resp.is_err());
    }

    // Test with response where repository has 0 issue
    #[test]
    fn convert_empty_response_to_issue() {
        let response_str = r#"
        {
            "data": {
                "repository": {
                    "issues": {
                        "pageInfo": {
                            "hasNextPage": false,
                            "endCursor": null
                        },
                        "nodes": []
                    }
                }
            }
        }"#;

        let graphql_response: GraphQlResponse<issues::ResponseData> =
            serde_json::from_str(response_str).expect("Valid JSON");
        let resp = GitHubIssueResponse::try_from(graphql_response)
            .expect("Correct data, so parsing should success");

        assert_eq!(resp.has_next_page, false);
        assert_eq!(resp.end_cursor, None);
        assert_eq!(resp.issues, vec![]);
    }

    // If you set $first: 0, GitHub returns this response
    #[test]
    fn convert_response_for_page_size_0_to_issue_() {
        let response_str = r#"
        {
            "data": {
                "repository": {
                    "issues": {
                        "pageInfo": {
                            "hasNextPage": true,
                            "endCursor": null
                        },
                        "nodes": []
                    }
                }
            }
        }"#;

        let graphql_response: GraphQlResponse<issues::ResponseData> =
            serde_json::from_str(response_str).expect("Valid JSON");
        let resp = GitHubIssueResponse::try_from(graphql_response)
            .expect("Correct data, so parsing should success");

        assert_eq!(resp.has_next_page, true);
        assert_eq!(resp.end_cursor, None);
        assert_eq!(resp.issues, vec![]);
    }

    #[test]
    fn convert_response_to_issue_() {
        let response_str = r#"
        {
            "data": {
                "repository": {
                    "issues": {
                        "pageInfo": {
                            "hasNextPage": true,
                            "endCursor": "Y3Vyc29yOnYyOpK5MjAyMi0wNy0xMlQxODozMzo0MiswOTowMM5Nl-UC"
                        },
                        "nodes": [
                            {
                                "id": "I_kwDOHpM3FM5Nko9l",
                                "number": 1,
                                "title": "실행 파일 빌드 가능한 Cargo.toml 및 소스 파일 추가",
                                "body": "프로젝트 디렉토리에서 `cargo run`을 실행하면 프로젝트 이름(\"AICE GitHub Dashboard Server\")를 출력하고 종료하도록 Cargo.toml과 main.rs를 추가합니다. 코드는 `cargo clippy -- -D warnings -W clippy::pedantic`을 문제없이 통과할 수 있어야합니다.\r\n\r\n변경한 코드는 새로운 브랜치에 커밋하고 pull request로 올려주세요.",
                                "state": "CLOSED",
                                "closedAt": "2022-07-12T06:52:51Z",
                                "createdAt": "2022-07-12T02:26:17Z",
                                "updatedAt": "2022-07-12T06:52:51Z",
                                "author": {
                                    "__typename": "User",
                                    "login": "msk"
                                },
                                "assignees": {
                                    "nodes": [
                                        {
                                            "login": "MontyCoder0701"
                                        }
                                    ]
                                },
                                "labels": {
                                    "nodes": []
                                },
                                "comments": {
                                    "totalCount": 1,
                                    "nodes": [
                                        {
                                            "author": {
                                                "__typename": "User",
                                                "login": "msk"
                                            },
                                            "body": "Resolved in #3.",
                                            "createdAt": "2022-07-12T06:52:51Z",
                                            "id": "IC_kwDOHpM3FM5GaoOk",
                                            "repository": {
                                                "name": "github-dashboard-server"
                                            },
                                            "updatedAt": "2022-07-12T06:52:51Z",
                                            "url": "https://github.com/aicers/github-dashboard-server/issues/1#issuecomment-1181385636"
                                        }
                                    ]
                                },
                                "projectItems": {
                                    "totalCount": 0,
                                    "nodes": []
                                },
                                "subIssues": {
                                    "totalCount": 0,
                                    "nodes": []
                                },
                                "parent": null,
                                "url": "https://github.com/aicers/github-dashboard-server/issues/1",
                                "closedByPullRequestsReferences": {
                                    "edges": []
                                }
                            },
                            {
                                "id": "I_kwDOHpM3FM5NlYA_",
                                "number": 4,
                                "title": "웹서버 실행",
                                "body": "[warp](https://docs.rs/warp/latest/warp/)를 사용하여 현재 디렉터리의 파일 내용을 보여주는 웹서버를 실행합니다.\r\n\r\nsrc 디렉터리에 `web` 모듈(web.rs)을 추가하고, `web::serve`라는 함수를 만들어 웹서버를 시작합니다. 웹서버 시작은 [`warp::fs::dir`](https://docs.rs/warp/latest/warp/filters/fs/fn.dir.html)과 [`warp::serve`](https://docs.rs/warp/latest/warp/fn.serve.html)를 쓰면 되는데, 주의할 점이 있습니다. `warp::serve`는 `async` 함수이므로 `web::serve`도 `async` 함수여야 합니다. 마찬가지로 `web::serve`를 호출하는 `main`도 `async`여야 하는데, 이건 [`tokio::main`](https://docs.rs/tokio/latest/tokio/attr.main.html)을 쓰면  됩니다.\r\n\r\n`cargo run` 실행 후 웹브라우저에서 `http://localhost:8000/README.md`로 접속하여 이 프로젝트의 `README.md`의 내용이 나오면 성공입니다.",
                                "state": "CLOSED",
                                "closedAt": "2022-07-12T09:09:01Z",
                                "createdAt": "2022-07-12T07:16:32Z",
                                "updatedAt": "2022-07-12T09:09:01Z",
                                "author": {
                                    "__typename": "User",
                                    "login": "msk"
                                },
                                "assignees": {
                                    "nodes": [
                                        {
                                            "login": "BLYKIM"
                                        }
                                    ]
                                },
                                "labels": {
                                    "nodes": []
                                },
                                "comments": {
                                    "totalCount": 0,
                                    "nodes": []
                                },
                                "projectItems": {
                                    "totalCount": 0,
                                    "nodes": []
                                },
                                "subIssues": {
                                    "totalCount": 0,
                                    "nodes": []
                                },
                                "parent": null,
                                "url": "https://github.com/aicers/github-dashboard-server/issues/4",
                                "closedByPullRequestsReferences": {
                                    "edges": [
                                        {
                                            "node": {
                                                "number": 5,
                                                "state": "MERGED",
                                                "closedAt": "2022-07-12T09:09:01Z",
                                                "createdAt": "2022-07-12T08:27:43Z",
                                                "updatedAt": "2022-07-12T09:09:02Z",
                                                "author": {
                                                    "__typename": "User",
                                                    "login": "BLYKIM"
                                                },
                                                "url": "https://github.com/aicers/github-dashboard-server/pull/5"
                                            }
                                        }
                                    ]
                                }
                            },
                            {
                                "id": "I_kwDOHpM3FM5Nl-UC",
                                "number": 6,
                                "title": "설정 파일에서 웹서버 주소 읽기",
                                "body": "하드코딩된 서버 IP 주소와 포트를 설정 파일에서 읽은 값을 쓰도록 변경합니다.\r\n\r\n설정 파일은 다음과 같은 TOML 파일로 주어집니다.\r\n\r\n```toml\r\n[web]\r\naddress = \"127.0.0.1:8080\"\r\n```\r\n\r\n이 파일은 먼저 메모리로 읽어들인 후, [toml::from_str](https://docs.rs/toml/latest/toml/de/fn.from_str.html)로 쉽게 파싱할 수 있습니다. 파일 이름은 명령행 인자로 주어집니다.\r\n\r\n위의 설정 파일 내용을 config.toml이란 파일에 넣어 두고, `cargo run -- config.toml`을 실행한 다음, 브라우저로 `http://127.0.0.1:8080/README.md`를 방문하여 README.md의 내용이 나오면 됩니다.",
                                "state": "CLOSED",
                                "closedAt": "2022-07-13T04:49:41Z",
                                "createdAt": "2022-07-12T09:33:42Z",
                                "updatedAt": "2022-07-13T04:49:41Z",
                                "author": {
                                    "__typename": "User",
                                    "login": "msk"
                                },
                                "assignees": {
                                    "nodes": [
                                        {
                                            "login": "kimhanbeom"
                                        }
                                    ]
                                },
                                "labels": {
                                    "nodes": []
                                },
                                "comments": {
                                    "totalCount": 0,
                                    "nodes": []
                                },
                                "projectItems": {
                                    "totalCount": 0,
                                    "nodes": []
                                },
                                "subIssues": {
                                    "totalCount": 0,
                                    "nodes": []
                                },
                                "parent": null,
                                "url": "https://github.com/aicers/github-dashboard-server/issues/6",
                                "closedByPullRequestsReferences": {
                                    "edges": []
                                }
                            }
                        ]
                    }
                }
            }
        }"#;

        let graphql_response: GraphQlResponse<issues::ResponseData> =
            serde_json::from_str(response_str).expect("Valid JSON");
        let resp = GitHubIssueResponse::try_from(graphql_response)
            .expect("Correct data, so parsing should success");

        assert_eq!(resp.has_next_page, true);
        assert_eq!(
            resp.end_cursor,
            Some(String::from(
                "Y3Vyc29yOnYyOpK5MjAyMi0wNy0xMlQxODozMzo0MiswOTowMM5Nl-UC"
            ))
        );
        assert_eq!(resp.issues.len(), 3);

        let issue1 = resp.issues.get(0).expect("Issue #1");
        let issue4 = resp.issues.get(1).expect("Issue #4");
        let issue6 = resp.issues.get(2).expect("Issue #6");

        // Issue #1
        assert_eq!(issue1.number, 1);
        assert_eq!(issue1.id, "I_kwDOHpM3FM5Nko9l");
        assert_eq!(
            issue1.title,
            "실행 파일 빌드 가능한 Cargo.toml 및 소스 파일 추가"
        );
        assert_eq!(issue1.author, "msk");
        assert_eq!(issue1.body, "프로젝트 디렉토리에서 `cargo run`을 실행하면 프로젝트 \
        이름(\"AICE GitHub Dashboard Server\")를 출력하고 종료하도록 Cargo.toml과 main.rs를 추가합니다. \
        코드는 `cargo clippy -- -D warnings -W clippy::pedantic`을 문제없이 통과할 수 있어야합니다.\
        \r\n\r\n변경한 코드는 새로운 브랜치에 커밋하고 pull request로 올려주세요.");
        assert_eq!(issue1.assignees, vec!["MontyCoder0701"]);
        assert_eq!(issue1.state, IssueState::CLOSED);
        assert_eq!(issue1.labels, Vec::<String>::new());
        assert_eq!(
            issue1.comments,
            GitHubCommentConnection {
                total_count: 1,
                nodes: vec![GitHubComment {
                    id: "IC_kwDOHpM3FM5GaoOk".to_string(),
                    author: "msk".to_string(),
                    body: "Resolved in #3.".to_string(),
                    created_at: "2022-07-12T06:52:51Z".parse().unwrap(),
                    updated_at: "2022-07-12T06:52:51Z".parse().unwrap(),
                    repository_name: "github-dashboard-server".to_string(),
                    url: "https://github.com/aicers/github-dashboard-server/issues/1#issuecomment-1181385636".to_string()
                }]
            }
        );
        assert_eq!(
            issue1.project_items,
            GitHubProjectV2ItemConnection {
                total_count: 0,
                nodes: vec![]
            }
        );
        assert_eq!(
            issue1.sub_issues,
            GitHubSubIssueConnection {
                total_count: 0,
                nodes: vec![]
            }
        );
        assert_eq!(issue1.parent, None);
        assert_eq!(
            issue1.url,
            "https://github.com/aicers/github-dashboard-server/issues/1"
        );
        assert_eq!(issue1.closed_by_pull_requests, vec![]);
        assert_eq!(issue1.created_at, "2022-07-12T02:26:17Z".parse().unwrap());
        assert_eq!(issue1.updated_at, "2022-07-12T06:52:51Z".parse().unwrap());
        assert_eq!(
            issue1.closed_at,
            Some("2022-07-12T06:52:51Z".parse().unwrap())
        );

        // Issue #4
        assert_eq!(issue4.number, 4);

        // Issue #6
        assert_eq!(issue6.number, 6);
    }
}
