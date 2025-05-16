use std::fmt;

use async_graphql::{
    connection::{query, Connection, EmptyFields},
    Context, Object, Result, SimpleObject,
};
use serde::{Deserialize, Serialize};

use crate::{
    database::{Database, TryFromKeyValue},
    github::open_issues::{
        OpenIssuesRepositoryIssuesNodes, OpenIssuesRepositoryIssuesNodesAuthor::User,
        OpenIssuesRepositoryIssuesNodesAuthorOnUser,
    },
};

#[derive(SimpleObject, Serialize, Deserialize)]
pub(crate) struct Issue {
    pub(crate) owner: String,
    pub(crate) repo: String,
    pub(crate) number: i64,
    pub(crate) title: String,
    pub(crate) author: String,
}

impl From<OpenIssuesRepositoryIssuesNodes> for Issue {
    fn from(issue: OpenIssuesRepositoryIssuesNodes) -> Self {
        let author = issue
            .author
            .map(|author| {
                if let User(OpenIssuesRepositoryIssuesNodesAuthorOnUser { login }) = author {
                    login
                } else {
                    String::new()
                }
            })
            .unwrap_or_default();
        Issue {
            author,
            owner: issue.repository.owner.login,
            repo: issue.repository.name,
            number: issue.number,
            title: issue.title,
        }
    }
}

impl TryFromKeyValue for Issue {
    fn try_from_key_value(_: &[u8], value: &[u8]) -> anyhow::Result<Self> {
        Ok(bincode::deserialize::<Issue>(value)?)
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
    use serde::Deserialize;

    use crate::{
        github::open_issues,
        graphql::{Issue, TestSchema},
    };

    fn load_issues() -> Vec<Issue> {
        let fixture: serde_json::Value =
            serde_json::from_reader(std::fs::File::open("fixtures/open_issues.json").unwrap())
                .unwrap();
        let data = fixture["data"].clone();
        let res = open_issues::ResponseData::deserialize(data).unwrap();
        res.collect_issues()
    }

    #[tokio::test]
    async fn ser_de_issues() {
        let issues = load_issues();
        let ser = bincode::serialize(&issues).unwrap();
        let de: Vec<Issue> = bincode::deserialize(&ser).unwrap();
        assert_eq!(de.len(), 3);
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
        let issues = load_issues();
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
            "{issues: {edges: [{node: {number: 106}}, {node: {number: 107}}], pageInfo: {hasNextPage: true}}}"
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
        let issues = load_issues();
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
            "{issues: {edges: [{node: {number: 107}}, {node: {number: 108}}], pageInfo: {hasPreviousPage: true}}}"
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
