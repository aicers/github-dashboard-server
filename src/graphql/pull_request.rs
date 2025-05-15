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
#[cfg_attr(test, derive(Default))]
pub(crate) struct PullRequest {
    pub(crate) owner: String,
    pub(crate) repo: String,
    pub(crate) number: i32,
    pub(crate) title: String,
    pub(crate) assignees: Vec<String>,
    pub(crate) reviewers: Vec<String>,
}

impl TryFrom<PullRequestsRepositoryPullRequestsNodes> for PullRequest {
    type Error = anyhow::Error;

    fn try_from(pr: PullRequestsRepositoryPullRequestsNodes) -> Result<Self, Self::Error> {
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

        Ok(Self {
            repo: pr.repository.name,
            owner: pr.repository.owner.login,
            number: i32::try_from(pr.number)?,
            title: pr.title,
            assignees,
            reviewers,
        })
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
    use super::PullRequest;
    use crate::graphql::TestSchema;

    fn create_pull_requests(n: usize) -> Vec<PullRequest> {
        (0..n)
            .map(|i| PullRequest {
                number: i32::try_from(i).unwrap(),
                ..Default::default()
            })
            .collect()
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
        let data = schema.execute(query).await.data.into_json().unwrap();
        assert_eq!(data["pullRequests"]["edges"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn pull_requests_first() {
        let schema = TestSchema::new();
        let pull_requests = create_pull_requests(2);
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
        let data = schema.execute(query).await.data.into_json().unwrap();
        assert_eq!(data["pullRequests"]["edges"].as_array().unwrap().len(), 1);
        assert_eq!(data["pullRequests"]["pageInfo"]["hasNextPage"], true);
    }

    #[tokio::test]
    async fn pull_requests_last() {
        let schema = TestSchema::new();
        let pull_requests = create_pull_requests(2);
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
        let data = schema.execute(query).await.data.into_json().unwrap();
        assert_eq!(data["pullRequests"]["edges"].as_array().unwrap().len(), 1);
        assert_eq!(data["pullRequests"]["pageInfo"]["hasPreviousPage"], true);
    }

    #[tokio::test]
    async fn reviewers() {
        let schema = TestSchema::new();
        let mut pull_requests = create_pull_requests(2);
        pull_requests[0].reviewers.push("sophie-cluml".to_string());

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
        let data = schema.execute(query).await.data.into_json().unwrap();

        assert_eq!(
            data["pullRequests"]["edges"][0]["node"]["reviewers"][0],
            "sophie-cluml"
        );
    }
}
