use std::fmt;

use async_graphql::{
    connection::{query, Connection, EmptyFields},
    Context, Object, Result, SimpleObject,
};
use serde::{Deserialize, Serialize};

use crate::{
    database::{Database, TryFromKeyValue},
    github::issues::{
        IssuesRepositoryIssuesNodes, IssuesRepositoryIssuesNodesAuthor::User,
        IssuesRepositoryIssuesNodesAuthorOnUser,
    },
};

#[derive(SimpleObject, Serialize, Deserialize)]
#[cfg_attr(test, derive(Default))]
pub(crate) struct Issue {
    pub(crate) owner: String,
    pub(crate) repo: String,
    pub(crate) number: i32,
    pub(crate) title: String,
    pub(crate) author: String,
}

impl TryFrom<IssuesRepositoryIssuesNodes> for Issue {
    type Error = anyhow::Error;

    fn try_from(issue: IssuesRepositoryIssuesNodes) -> Result<Self, Self::Error> {
        let author = issue
            .author
            .map(|author| {
                if let User(IssuesRepositoryIssuesNodesAuthorOnUser { login }) = author {
                    login
                } else {
                    String::new()
                }
            })
            .unwrap_or_default();
        Ok(Issue {
            author,
            owner: issue.repository.owner.login,
            repo: issue.repository.name,
            number: i32::try_from(issue.number)?,
            title: issue.title,
        })
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
    use crate::graphql::{Issue, TestSchema};

    fn create_issues(n: usize) -> Vec<Issue> {
        (0..n)
            .map(|i| Issue {
                number: i32::try_from(i).unwrap(),
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
        let data = schema.execute(query).await.data.into_json().unwrap();
        assert_eq!(data["issues"]["edges"].as_array().unwrap().len(), 0);
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
        let data = schema.execute(query).await.data.into_json().unwrap();
        assert_eq!(data["issues"]["edges"].as_array().unwrap().len(), 2);
        assert_eq!(data["issues"]["pageInfo"]["hasNextPage"], true);
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
        let data = schema.execute(query).await.data.into_json().unwrap();
        assert_eq!(data["issues"]["edges"].as_array().unwrap().len(), 2);
        assert_eq!(data["issues"]["pageInfo"]["hasPreviousPage"], true);
    }
}
