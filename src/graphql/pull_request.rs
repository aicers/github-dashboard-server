use std::fmt;

use async_graphql::{
    connection::{query, Connection, EmptyFields},
    Context, Object, Result, SimpleObject,
};
use serde::{Deserialize, Serialize};

use crate::{
    database::{Database, TryFromKeyValue},
    github::pull_requests::{
        PullRequestsRepositoryPullRequestsNodes,
        PullRequestsRepositoryPullRequestsNodesReviewRequestsNodesRequestedReviewer::User,
    },
};

#[derive(SimpleObject, Serialize, Deserialize)]
pub(crate) struct PullRequest {
    pub(crate) owner: String,
    pub(crate) repo: String,
    pub(crate) number: i64,
    pub(crate) title: String,
    pub(crate) assignees: Vec<String>,
    pub(crate) reviewers: Vec<String>,
}

impl From<PullRequestsRepositoryPullRequestsNodes> for PullRequest {
    fn from(pr: PullRequestsRepositoryPullRequestsNodes) -> Self {
        let assignees: Vec<String> = pr
            .assignees
            .nodes
            .map(|nodes| nodes.into_iter().flatten().map(|node| node.login).collect())
            .unwrap_or_default();

        let reviewers = pr
            .review_requests
            .and_then(|request| {
                request.nodes.map(|nodes| {
                    nodes
                        .into_iter()
                        .flatten()
                        .filter_map(|review_request| {
                            review_request.requested_reviewer.and_then(|reviewer| {
                                if let User(user) = reviewer {
                                    Some(user.login)
                                } else {
                                    None
                                }
                            })
                        })
                        .collect()
                })
            })
            .unwrap_or_default();

        Self {
            repo: pr.repository.name,
            owner: pr.repository.owner.login,
            number: pr.number,
            title: pr.title,
            assignees,
            reviewers,
        }
    }
}

impl TryFromKeyValue for PullRequest {
    fn try_from_key_value(_: &[u8], value: &[u8]) -> anyhow::Result<Self> {
        Ok(bincode::deserialize::<PullRequest>(value)?)
    }
}

impl fmt::Display for PullRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}/{}#{}", self.owner, self.repo, self.number)
    }
}

#[derive(Default)]
pub(super) struct PullRequestQuery;

#[Object]
impl PullRequestQuery {
    async fn pull_requests(
        &self,
        ctx: &Context<'_>,
        after: Option<String>,
        before: Option<String>,
        first: Option<i32>,
        last: Option<i32>,
    ) -> Result<Connection<String, PullRequest, EmptyFields, EmptyFields>> {
        query(
            after,
            before,
            first,
            last,
            |after, before, first, last| async move {
                super::load_connection(ctx, Database::pull_requests, after, before, first, last)
            },
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::PullRequest;
    use crate::{github::pull_requests, graphql::TestSchema};

    fn load_pull_requests() -> Vec<PullRequest> {
        let fixture: serde_json::Value =
            serde_json::from_reader(std::fs::File::open("fixtures/pull_requests.json").unwrap())
                .unwrap();
        let data = fixture["data"].clone();
        let res = pull_requests::ResponseData::deserialize(data).unwrap();
        res.collect_pull_requests()
    }

    #[tokio::test]
    async fn pull_requests_empty() {
        let schema = TestSchema::new();
        let query = r"
        {
            pullRequests {
                edges {
                    node {
                        number
                    }
                }
            }
        }";
        let res = schema.execute(query).await;
        assert_eq!(res.data.to_string(), "{pullRequests: {edges: []}}");
    }

    #[tokio::test]
    async fn pull_requests_first() {
        let schema = TestSchema::new();
        let pull_requests = load_pull_requests();
        schema
            .db
            .insert_pull_requests(pull_requests, "owner", "name")
            .unwrap();

        let query = r"
        {
            pullRequests(first: 1) {
                edges {
                    node {
                        number
                    }
                }
                pageInfo {
                    hasNextPage
                }
            }
        }";
        let res = schema.execute(query).await;
        assert_eq!(
            res.data.to_string(),
            "{pullRequests: {edges: [{node: {number: 2}}], pageInfo: {hasNextPage: true}}}"
        );

        let query = r"
        {
            pullRequests(first: 5) {
                pageInfo {
                    hasNextPage
                }
            }
        }";
        let res = schema.execute(query).await;
        assert_eq!(
            res.data.to_string(),
            "{pullRequests: {pageInfo: {hasNextPage: false}}}"
        );
    }

    #[tokio::test]
    async fn pull_requests_last() {
        let schema = TestSchema::new();
        let pull_requests = load_pull_requests();
        schema
            .db
            .insert_pull_requests(pull_requests, "owner", "name")
            .unwrap();

        let query = r"
        {
            pullRequests(last: 1) {
                edges {
                    node {
                        number
                    }
                }
                pageInfo {
                    hasPreviousPage
                }
            }
        }";
        let res = schema.execute(query).await;
        assert_eq!(
            res.data.to_string(),
            "{pullRequests: {edges: [{node: {number: 5}}], pageInfo: {hasPreviousPage: true}}}"
        );

        let query = r"
        {
            pullRequests(last: 2) {
                pageInfo {
                    hasPreviousPage
                }
            }
        }";
        let res = schema.execute(query).await;
        assert_eq!(
            res.data.to_string(),
            "{pullRequests: {pageInfo: {hasPreviousPage: true}}}"
        );
    }

    #[tokio::test]
    async fn reviewers() {
        let schema = TestSchema::new();
        let pull_requests = load_pull_requests();
        schema
            .db
            .insert_pull_requests(pull_requests, "owner", "name")
            .unwrap();

        let query = r"
        {
            pullRequests(first: 1) {
                edges {
                    node {
                        number
                        reviewers
                    }
                }
                pageInfo {
                    hasPreviousPage
                }
            }
        }";
        let res = schema.execute(query).await;
        assert_eq!(
            res.data.to_string(),
            "{pullRequests: {edges: [{node: {number: 2, reviewers: [\"dayeon5470\"]}}], pageInfo: {hasPreviousPage: false}}}"
        );
    }
}
