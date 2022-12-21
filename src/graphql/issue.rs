use anyhow::Context as AnyhowContext;
use async_graphql::{
    connection::{query, Connection, EmptyFields},
    Context, Object, Result, SimpleObject,
};
use std::fmt;

use crate::database::{self, Database, TryFromKeyValue};

#[derive(SimpleObject)]
pub struct Issue {
    pub owner: String,
    pub repo: String,
    pub number: i32,
    pub title: String,
    pub author: String,
}

impl TryFromKeyValue for Issue {
    fn try_from_key_value(key: &[u8], value: &[u8]) -> anyhow::Result<Self> {
        let (owner, repo, number) = database::parse_key(key)
            .with_context(|| format!("invalid key in database: {key:02x?}"))?;
        let (title, author, _) = bincode::deserialize::<(String, String, Option<String>)>(value)
            .with_context(|| {
                format!(
                    "invalid value in database for key {:02x?}: {:02x?}",
                    key, value
                )
            })?;
        let issue = Issue {
            title,
            author,
            owner,
            repo,
            number: i32::try_from(number).unwrap_or(i32::MAX),
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
    async fn issues<'ctx>(
        &self,
        ctx: &Context<'ctx>,
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

    #[tokio::test]
    async fn issues_empty() {
        let schema = TestSchema::new();
        let query = r#"
        {
            issues {
                edges {
                    node {
                        number
                    }
                }
            }
        }"#;
        let res = schema.execute(query).await;
        assert_eq!(res.data.to_string(), "{issues: {edges: []}}");
    }

    #[tokio::test]
    async fn issues_first() {
        let schema = TestSchema::new();
        let issues = vec![
            GitHubIssue {
                number: 1,
                title: "issue 1".to_string(),
                author: "author 1".to_string(),
                closed_at: None,
            },
            GitHubIssue {
                number: 2,
                title: "issue 2".to_string(),
                author: "author 2".to_string(),
                closed_at: None,
            },
            GitHubIssue {
                number: 3,
                title: "issue 3".to_string(),
                author: "author 3".to_string(),
                closed_at: None,
            },
        ];
        schema.db.insert_issues(issues, "owner", "name").unwrap();

        let query = r#"
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
        }"#;
        let res = schema.execute(query).await;
        assert_eq!(
            res.data.to_string(),
            "{issues: {edges: [{node: {number: 1}},{node: {number: 2}}],pageInfo: {hasNextPage: true}}}"
        );

        let query = r#"
        {
            issues(first: 5) {
                pageInfo {
                    hasNextPage
                }
            }
        }"#;
        let res = schema.execute(query).await;
        assert_eq!(
            res.data.to_string(),
            "{issues: {pageInfo: {hasNextPage: false}}}"
        );
    }

    #[tokio::test]
    async fn issues_last() {
        let schema = TestSchema::new();
        let issues = vec![
            GitHubIssue {
                number: 1,
                title: "issue 1".to_string(),
                author: "author 1".to_string(),
                closed_at: None,
            },
            GitHubIssue {
                number: 2,
                title: "issue 2".to_string(),
                author: "author 2".to_string(),
                closed_at: None,
            },
            GitHubIssue {
                number: 3,
                title: "issue 3".to_string(),
                author: "author 3".to_string(),
                closed_at: None,
            },
        ];
        schema.db.insert_issues(issues, "owner", "name").unwrap();

        let query = r#"
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
        }"#;
        let res = schema.execute(query).await;
        assert_eq!(
            res.data.to_string(),
            "{issues: {edges: [{node: {number: 2}},{node: {number: 3}}],pageInfo: {hasPreviousPage: true}}}"
        );

        let query = r#"
        {
            issues(last: 5) {
                pageInfo {
                    hasPreviousPage
                }
            }
        }"#;
        let res = schema.execute(query).await;
        assert_eq!(
            res.data.to_string(),
            "{issues: {pageInfo: {hasPreviousPage: false}}}"
        );
    }
}
