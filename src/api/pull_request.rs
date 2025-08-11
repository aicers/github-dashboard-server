use std::fmt;

use anyhow::Context as AnyhowContext;
use async_graphql::{
    connection::{query, Connection, EmptyFields},
    Context, Object, Result, SimpleObject,
};

use crate::{
    api,
    database::{self, Database, TryFromKeyValue},
};

#[derive(SimpleObject)]
pub(crate) struct PullRequest {
    pub(crate) owner: String,
    pub(crate) repo: String,
    pub(crate) number: i32,
    pub(crate) title: String,
    pub(crate) assignees: Vec<String>,
    pub(crate) reviewers: Vec<String>,
}

impl TryFromKeyValue for PullRequest {
    fn try_from_key_value(key: &[u8], value: &[u8]) -> anyhow::Result<Self> {
        let (owner, repo, number) = database::parse_key(key)
            .with_context(|| format!("invalid key in database: {key:02x?}"))?;
        let deserialized = bincode::deserialize::<(String, Vec<String>, Vec<String>)>(value)?;
        let (title, assignees, reviewers) = deserialized;
        let pr = PullRequest {
            owner,
            repo,
            number,
            title,
            assignees,
            reviewers,
        };
        Ok(pr)
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
                api::load_connection(ctx, Database::pull_requests, after, before, first, last)
            },
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use crate::{api::TestSchema, outbound::GitHubPullRequests};

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
        let pull_requests = vec![
            GitHubPullRequests {
                number: 1,
                title: "pull request 1".to_string(),
                assignees: vec!["assignee 1".to_string()],
                reviewers: vec!["reviewer 1".to_string()],
            },
            GitHubPullRequests {
                number: 2,
                title: "pull request 2".to_string(),
                assignees: vec!["assignee 2".to_string()],
                reviewers: vec!["reviewer 2".to_string()],
            },
        ];
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
            "{pullRequests: {edges: [{node: {number: 1}}], pageInfo: {hasNextPage: true}}}"
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
        let pull_requests = vec![
            GitHubPullRequests {
                number: 1,
                title: "pull request 1".to_string(),
                assignees: vec!["assignee 1".to_string()],
                reviewers: vec!["reviewer 1".to_string()],
            },
            GitHubPullRequests {
                number: 2,
                title: "pull request 2".to_string(),
                assignees: vec!["assignee 2".to_string()],
                reviewers: vec!["reviewer 2".to_string()],
            },
        ];
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
            "{pullRequests: {edges: [{node: {number: 2}}], pageInfo: {hasPreviousPage: true}}}"
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
            "{pullRequests: {pageInfo: {hasPreviousPage: false}}}"
        );
    }
}
