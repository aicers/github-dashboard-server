use std::fmt;

use anyhow::Context as AnyhowContext;
use async_graphql::{
    connection::{query, Connection, EmptyFields},
    scalar, Context, Object, Result, SimpleObject,
};
use serde::{Deserialize, Serialize};

use crate::{
    api,
    api::DateTimeUtc,
    database::{self, Database, TryFromKeyValue},
    outbound::{
        issues::{IssueState, PullRequestState},
        GitHubIssue,
    },
};

scalar!(IssueState);
scalar!(PullRequestState);

#[derive(SimpleObject)]
pub(crate) struct Issue {
    pub(crate) id: String,
    pub(crate) owner: String,
    pub(crate) repo: String,
    pub(crate) number: i32,
    pub(crate) title: String,
    pub(crate) body: String,
    pub(crate) state: IssueState,
    pub(crate) author: String,
    pub(crate) assignees: Vec<String>,
    pub(crate) labels: Vec<String>,
    pub(crate) comments: CommentConnection,
    pub(crate) project_items: ProjectV2ItemConnection,
    pub(crate) sub_issues: SubIssueConnection,
    pub(crate) parent: Option<ParentIssue>,
    pub(crate) url: String,
    pub(crate) closed_by_pull_requests: Vec<PullRequestRef>,
    pub(crate) created_at: DateTimeUtc,
    pub(crate) updated_at: DateTimeUtc,
    pub(crate) closed_at: Option<DateTimeUtc>,
}

#[derive(SimpleObject, Debug, Default)]
pub(crate) struct CommentConnection {
    pub(crate) total_count: i32,
    pub(crate) nodes: Vec<Comment>,
}

#[derive(SimpleObject, Debug)]
pub(crate) struct Comment {
    pub(crate) id: String,
    pub(crate) author: String,
    pub(crate) body: String,
    pub(crate) created_at: DateTimeUtc,
    pub(crate) updated_at: DateTimeUtc,
    pub(crate) repository_name: String,
    pub(crate) url: String,
}

#[derive(SimpleObject, Debug, Default)]
pub(crate) struct ProjectV2ItemConnection {
    pub(crate) total_count: i32,
    pub(crate) nodes: Vec<ProjectV2Item>,
}

#[derive(SimpleObject, Debug)]
pub(crate) struct ProjectV2Item {
    pub(crate) id: String,
    pub(crate) todo_status: Option<String>,
    pub(crate) todo_priority: Option<String>,
    pub(crate) todo_size: Option<String>,
    pub(crate) todo_initiation_option: Option<String>,
    pub(crate) todo_pending_days: Option<f64>,
}

#[derive(SimpleObject, Debug, Default)]
pub(crate) struct SubIssueConnection {
    pub(crate) total_count: i32,
    pub(crate) nodes: Vec<SubIssue>,
}

#[derive(SimpleObject, Debug)]
pub(crate) struct SubIssue {
    pub(crate) id: String,
    pub(crate) number: i32,
    pub(crate) title: String,
    pub(crate) state: IssueState,
    pub(crate) author: String,
    pub(crate) assignees: Vec<String>,
    pub(crate) created_at: DateTimeUtc,
    pub(crate) updated_at: DateTimeUtc,
    pub(crate) closed_at: Option<DateTimeUtc>,
}

#[derive(SimpleObject, Serialize, Deserialize, Debug)]
pub(crate) struct ParentIssue {
    pub(crate) id: String,
    pub(crate) number: i32,
    pub(crate) title: String,
}

#[derive(SimpleObject, Debug)]
pub(crate) struct PullRequestRef {
    pub(crate) number: i32,
    pub(crate) state: PullRequestState,
    pub(crate) author: String,
    pub(crate) created_at: DateTimeUtc,
    pub(crate) updated_at: DateTimeUtc,
    pub(crate) closed_at: Option<DateTimeUtc>,
    pub(crate) url: String,
}

impl TryFromKeyValue for Issue {
    fn try_from_key_value(key: &[u8], value: &[u8]) -> anyhow::Result<Self> {
        let (owner, repo, number) = database::parse_key(key)
            .with_context(|| format!("invalid key in database: {key:02x?}"))?;
        let issue: GitHubIssue = bincode::deserialize(value)?;
        Ok(Issue {
            id: issue.id,
            owner,
            repo,
            number,
            title: issue.title,
            body: issue.body,
            state: issue.state,
            author: issue.author,
            assignees: issue.assignees,
            labels: issue.labels,
            comments: CommentConnection {
                total_count: issue.comments.total_count,
                nodes: issue
                    .comments
                    .nodes
                    .into_iter()
                    .map(|comment| Comment {
                        id: comment.id,
                        author: comment.author,
                        body: comment.body,
                        repository_name: comment.repository_name,
                        url: comment.url,
                        created_at: DateTimeUtc(comment.created_at),
                        updated_at: DateTimeUtc(comment.updated_at),
                    })
                    .collect(),
            },
            project_items: ProjectV2ItemConnection {
                total_count: issue.project_items.total_count,
                nodes: issue
                    .project_items
                    .nodes
                    .into_iter()
                    .map(|item| ProjectV2Item {
                        id: item.id,
                        todo_status: item.todo_status,
                        todo_priority: item.todo_priority,
                        todo_size: item.todo_size,
                        todo_initiation_option: item.todo_initiation_option,
                        todo_pending_days: item.todo_pending_days,
                    })
                    .collect(),
            },
            sub_issues: SubIssueConnection {
                total_count: issue.sub_issues.total_count,
                nodes: issue
                    .sub_issues
                    .nodes
                    .into_iter()
                    .map(|sub| SubIssue {
                        id: sub.id,
                        number: sub.number,
                        title: sub.title,
                        state: sub.state,
                        author: sub.author,
                        assignees: sub.assignees,
                        created_at: DateTimeUtc(sub.created_at),
                        updated_at: DateTimeUtc(sub.updated_at),
                        closed_at: sub.closed_at.map(DateTimeUtc),
                    })
                    .collect(),
            },
            parent: issue.parent.map(|p| ParentIssue {
                id: p.id,
                number: p.number,
                title: p.title,
            }),
            url: issue.url,
            closed_by_pull_requests: issue
                .closed_by_pull_requests
                .into_iter()
                .map(|pr| PullRequestRef {
                    number: pr.number,
                    state: pr.state,
                    author: pr.author,
                    url: pr.url,
                    created_at: DateTimeUtc(pr.created_at),
                    updated_at: DateTimeUtc(pr.updated_at),
                    closed_at: pr.closed_at.map(DateTimeUtc),
                })
                .collect(),
            created_at: DateTimeUtc(issue.created_at),
            updated_at: DateTimeUtc(issue.updated_at),
            closed_at: issue.closed_at.map(DateTimeUtc),
        })
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
                api::load_connection(ctx, Database::issues, after, before, first, last)
            },
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use crate::{api::TestSchema, outbound::GitHubIssue};

    fn create_issues(n: usize) -> Vec<GitHubIssue> {
        (1..=n)
            .map(|i| GitHubIssue {
                number: i.try_into().unwrap(),
                ..Default::default()
            })
            .collect()
    }

    #[tokio::test]
    async fn issues_empty() {
        let schema = TestSchema::new().await;
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
        let schema = TestSchema::new().await;
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
        let schema = TestSchema::new().await;
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
