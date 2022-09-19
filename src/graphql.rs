use crate::database::Database;
use async_graphql::{
    types::connection::{query, Connection, Edge, EmptyFields},
    Context, EmptyMutation, EmptySubscription, Object, OutputType, Result, SimpleObject,
};
use sled::Tree;
use std::fmt::Display;

pub struct Query;

pub type Schema = async_graphql::Schema<Query, EmptyMutation, EmptySubscription>;

#[derive(Debug, SimpleObject)]
pub struct Issue {
    pub owner: String,
    pub repo: String,
    pub number: i32,
    pub title: String,
    pub author: String,
}

#[derive(Debug, SimpleObject)]
pub struct PullRequest {
    pub owner: String,
    pub repo: String,
    pub number: i32,
    pub title: String,
    pub assignees: Vec<String>,
    pub reviewers: Vec<String>,
}

#[derive(Debug)]
pub enum PagingType {
    All,
    First(usize),
    Last(usize),
    AfterFirst(String, usize),
    BeforeLast(String, usize),
}

impl Display for Issue {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}/{}#{}", self.owner, self.repo, self.number)
    }
}

impl Display for PullRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}/{}#{}", self.owner, self.repo, self.number)
    }
}

#[Object]
impl Query {
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
                load_issues(ctx, after, before, first, last)
            },
        )
        .await
    }

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
            |after, before, first, last| async move {
                load_pull_requests(ctx, after, before, first, last)
            },
        )
        .await
    }
}

fn load_issues(
    ctx: &Context<'_>,
    after: Option<String>,
    before: Option<String>,
    first: Option<usize>,
    last: Option<usize>,
) -> Result<Connection<String, Issue, EmptyFields, EmptyFields>> {
    let db = ctx.data::<Database>()?;
    let p_type = check_paging_type(after, before, first, last)?;
    let select_vec = db.select_issue_range(p_type)?;
    let (prev, next) = has_prev_next(select_vec.first(), select_vec.last(), db.issue_store())?;
    Ok(connect_cursor(select_vec, prev, next))
}

fn load_pull_requests(
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

fn connect_cursor<T>(
    select_vec: Vec<T>,
    prev: bool,
    next: bool,
) -> Connection<String, T, EmptyFields, EmptyFields>
where
    T: OutputType + Display,
{
    let mut connection: Connection<String, T, EmptyFields, EmptyFields> =
        Connection::new(prev, next);
    for output in select_vec {
        connection
            .edges
            .push(Edge::new(base64::encode(format!("{}", output)), output));
    }
    connection
}

fn has_prev_next<T>(prev: Option<&T>, next: Option<&T>, tree: &Tree) -> anyhow::Result<(bool, bool)>
where
    T: OutputType + Display,
{
    if let Some(prev_val) = prev {
        if let Some(next_val) = next {
            return Ok((
                Database::has_prev(format!("{}", prev_val), tree)?,
                Database::has_next(format!("{}", next_val), tree)?,
            ));
        }
    }
    Ok((false, false))
}

fn check_paging_type(
    after: Option<String>,
    before: Option<String>,
    first: Option<usize>,
    last: Option<usize>,
) -> Result<PagingType> {
    if let Some(f_val) = first {
        if let Some(cursor) = after {
            return Ok(PagingType::AfterFirst(
                String::from_utf8(base64::decode(cursor)?)?,
                f_val,
            ));
        }
        return Ok(PagingType::First(f_val));
    } else if let Some(l_val) = last {
        if let Some(cursor) = before {
            return Ok(PagingType::BeforeLast(
                String::from_utf8(base64::decode(cursor)?)?,
                l_val,
            ));
        }
        return Ok(PagingType::Last(l_val));
    }
    Ok(PagingType::All)
}

pub fn schema(database: Database) -> Schema {
    Schema::build(Query, EmptyMutation, EmptySubscription)
        .data(database)
        .finish()
}

#[cfg(test)]
struct TestSchema {
    _dir: tempfile::TempDir, // to prevent the data directory from being deleted while the test is running
    schema: Schema,
}

#[cfg(test)]
impl TestSchema {
    fn new() -> Self {
        let db_dir = tempfile::tempdir().unwrap();
        let db = Database::connect(db_dir.path()).unwrap();
        let schema = schema(db);
        Self {
            _dir: db_dir,
            schema,
        }
    }

    async fn execute(&self, query: &str) -> async_graphql::Response {
        let request: async_graphql::Request = query.into();
        self.schema.execute(request).await
    }
}

#[cfg(test)]
mod tests {
    use super::TestSchema;

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
        let res = schema.execute(&query).await;
        assert_eq!(res.data.to_string(), "{issues: {edges: []}}");
    }
}
