use super::{check_paging_type, connect_cursor, has_prev_next};
use crate::database::Database;
use async_graphql::{
    connection::{query, Connection, EmptyFields},
    Context, Object, Result, SimpleObject,
};
use std::fmt;

#[derive(SimpleObject)]
pub struct PullRequest {
    pub owner: String,
    pub repo: String,
    pub number: i32,
    pub title: String,
    pub assignees: Vec<String>,
    pub reviewers: Vec<String>,
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
    async fn pull_requests<'ctx>(
        &self,
        ctx: &Context<'ctx>,
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
            |after, before, first, last| async move { load(ctx, after, before, first, last) },
        )
        .await
    }
}

fn load(
    ctx: &Context<'_>,
    after: Option<String>,
    before: Option<String>,
    first: Option<usize>,
    last: Option<usize>,
) -> Result<Connection<String, PullRequest, EmptyFields, EmptyFields>> {
    let db = ctx.data::<Database>()?;
    let p_type = check_paging_type(after, before, first, last)?;
    let select_vec = db.select_pull_request_range(p_type)?;
    let (prev, next) = has_prev_next(
        select_vec.first(),
        select_vec.last(),
        db.pull_request_store(),
    )?;
    Ok(connect_cursor(select_vec, prev, next))
}

#[cfg(test)]
mod tests {
    use crate::{github::GitHubPullRequests, graphql::TestSchema};

    #[tokio::test]
    async fn pull_requests_empty() {
        let schema = TestSchema::new();
        let query = r#"
        {
            pullRequests {
                edges {
                    node {
                        number
                    }
                }
            }
        }"#;
        let res = schema.execute(&query).await;
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

        let query = r#"
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
        }"#;
        let res = schema.execute(&query).await;
        assert_eq!(
            res.data.to_string(),
            "{pullRequests: {edges: [{node: {number: 1}}],pageInfo: {hasNextPage: true}}}"
        );

        let query = r#"
        {
            pullRequests(first: 5) {
                pageInfo {
                    hasNextPage
                }
            }
        }"#;
        let res = schema.execute(&query).await;
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

        let query = r#"
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
        }"#;
        let res = schema.execute(&query).await;
        assert_eq!(
            res.data.to_string(),
            "{pullRequests: {edges: [{node: {number: 2}}],pageInfo: {hasPreviousPage: true}}}"
        );

        let query = r#"
        {
            pullRequests(last: 2) {
                pageInfo {
                    hasPreviousPage
                }
            }
        }"#;
        let res = schema.execute(&query).await;
        assert_eq!(
            res.data.to_string(),
            "{pullRequests: {pageInfo: {hasPreviousPage: false}}}"
        );
    }
}
