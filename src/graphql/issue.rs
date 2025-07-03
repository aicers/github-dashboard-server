use std::fmt;

use anyhow::Context as AnyhowContext;
use async_graphql::{
    connection::{query, Connection, EmptyFields},
    scalar, Context, Object, Result, SimpleObject,
};

use crate::{
    database::{self, Database, TryFromKeyValue},
    github::{issues::IssueState, GitHubIssue},
    graphql::DateTimeUtc,
};

scalar!(IssueState);

#[derive(SimpleObject)]
pub(crate) struct Issue {
    pub(crate) owner: String,
    pub(crate) repo: String,
    pub(crate) number: i32,
    pub(crate) title: String,
    pub(crate) author: String,
    pub(crate) created_at: DateTimeUtc,
    pub(crate) state: IssueState,
    pub(crate) assignees: Vec<String>,
}

impl TryFromKeyValue for Issue {
    fn try_from_key_value(key: &[u8], value: &[u8]) -> anyhow::Result<Self> {
        let (owner, repo, number) = database::parse_key(key)
            .with_context(|| format!("invalid key in database: {key:02x?}"))?;
        let GitHubIssue {
            title,
            author,
            created_at,
            state,
            assignees,
            ..
        } = bincode::deserialize::<GitHubIssue>(value)?;
        let issue = Issue {
            title,
            author,
            owner,
            repo,
            number: i32::try_from(number).unwrap_or(i32::MAX),
            created_at: DateTimeUtc(created_at),
            state,
            assignees,
        };
        Ok(issue)
    }
}

impl fmt::Display for Issue {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}/{}#{}", self.owner, self.repo, self.number)
    }
}

#[derive(Default)]
pub(super) struct IssueQuery;

#[Object]
impl IssueQuery {
    async fn issues(
        &self,
        ctx: &Context<'_>,
        after: Option<String>,
        before: Option<String>,
        first: Option<i32>,
        last: Option<i32>,
    ) -> Result<Connection<String, Issue, EmptyFields, EmptyFields>> {
        query(
            after,
            before,
            first,
            last,
            |after, before, first, last| async move {
                super::load_connection(ctx, Database::issues, after, before, first, last)
            },
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use crate::{github::GitHubIssue, graphql::TestSchema};

    fn create_issues(n: usize) -> Vec<GitHubIssue> {
        (1..=n)
            .map(|i| GitHubIssue {
                number: i64::try_from(i).unwrap(),
                ..Default::default()
            })
            .collect()
    }

    #[tokio::test]
    async fn issues_empty() {
        let schema = TestSchema::new();
        let query = r"
        {
            issues {
                edges {
                    node {
                        number
                    }
                }
            }
        }";
        let res = schema.execute(query).await;
        assert_eq!(res.data.to_string(), "{issues: {edges: []}}");
    }

    #[tokio::test]
    async fn issues_first() {
        let schema = TestSchema::new();
        let issues = create_issues(3);
        schema.db.insert_issues(issues, "owner", "name").unwrap();

        let query = r"
        {
            issues(first: 2) {
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
            "{issues: {edges: [{node: {number: 1}}, {node: {number: 2}}], pageInfo: {hasNextPage: true}}}"
        );

        let query = r"
        {
            issues(first: 5) {
                pageInfo {
                    hasNextPage
                }
            }
        }";
        let res = schema.execute(query).await;
        assert_eq!(
            res.data.to_string(),
            "{issues: {pageInfo: {hasNextPage: false}}}"
        );
    }

    #[tokio::test]
    async fn issues_last() {
        let schema = TestSchema::new();
        let issues = create_issues(3);
        schema.db.insert_issues(issues, "owner", "name").unwrap();

        let query = r"
        {
            issues(last: 2) {
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
            "{issues: {edges: [{node: {number: 2}}, {node: {number: 3}}], pageInfo: {hasPreviousPage: true}}}"
        );

        let query = r"
        {
            issues(last: 5) {
                pageInfo {
                    hasPreviousPage
                }
            }
        }";
        let res = schema.execute(query).await;
        assert_eq!(
            res.data.to_string(),
            "{issues: {pageInfo: {hasPreviousPage: false}}}"
        );
    }
}
