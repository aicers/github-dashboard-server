use crate::database::Database;
use anyhow::{anyhow, bail, Result};
use async_graphql::{
    types::connection::{query, Connection, ConnectionNameType, Edge, EdgeNameType, EmptyFields},
    Context, EmptyMutation, EmptySubscription, Object, OutputType, Schema, SimpleObject,
};
use std::fmt::Display;

pub struct Query;
pub struct GithubConnectionName;
pub struct GithubEdgeName;

#[derive(Debug, SimpleObject)]
pub struct Issue {
    pub owner: String,
    pub repo: String,
    pub number: i32,
    pub title: String,
}

#[derive(Clone, Copy, Debug)]
struct Pagination {
    after_idx: Option<usize>,
    before_idx: Option<usize>,
    first: Option<usize>,
    last: Option<usize>,
}

impl Display for Issue {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}/{}#{}", self.owner, self.repo, self.number)
    }
}

impl ConnectionNameType for GithubConnectionName {
    fn type_name<T: OutputType>() -> String {
        "GithubConnection".to_string()
    }
}

impl EdgeNameType for GithubEdgeName {
    fn type_name<T: OutputType>() -> String {
        "GithubEdge".to_string()
    }
}

#[Object]
impl Query {
    pub async fn issues<'ctx>(
        &self,
        ctx: &Context<'ctx>,
        after: Option<String>,
        before: Option<String>,
        first: Option<i32>,
        last: Option<i32>,
    ) -> Result<
        Connection<String, Issue, EmptyFields, EmptyFields, GithubConnectionName, GithubEdgeName>,
    > {
        match query(
            after,
            before,
            first,
            last,
            |after, before, first, last| async move {
                let db = match ctx.data::<Database>() {
                    Ok(ret) => ret,
                    Err(e) => {
                        bail!("{:?}", e)
                    }
                };

                let (after_idx, before_idx) = get_cursor_index(db, after, before)?;
                let issue_len = db.issue_tree_len();
                let (start, end) = get_pagination(
                    Pagination {
                        after_idx,
                        before_idx,
                        first,
                        last,
                    },
                    issue_len,
                );
                let issue_vec = db.select_range(start, end)?;
                Ok(connect_cursor(issue_vec, start, end, issue_len))
            },
        )
        .await
        {
            Ok(conn) => Ok(conn),
            Err(e) => Err(anyhow!("{:?}", e)),
        }
    }
}

fn connect_cursor<T>(
    mut db_vec: Vec<T>,
    start: usize,
    end: usize,
    len: usize,
) -> Connection<String, T, EmptyFields, EmptyFields, GithubConnectionName, GithubEdgeName>
where
    T: OutputType + Display,
{
    let mut connection: Connection<
        String,
        T,
        EmptyFields,
        EmptyFields,
        GithubConnectionName,
        GithubEdgeName,
    > = Connection::new(start > 0, end < len);
    for _ in start..end {
        let issue = db_vec.remove(0);
        connection
            .edges
            .push(Edge::new(base64::encode(format!("{}", issue)), issue));
    }
    connection
}

fn get_pagination(pg: Pagination, len: usize) -> (usize, usize) {
    let mut start = pg.after_idx.map_or(0, |after| after + 1);
    let mut end = pg.before_idx.unwrap_or(len);
    if let Some(first) = pg.first {
        end = (start + first).min(end);
    }
    if let Some(last) = pg.last {
        start = if last > end - start { end } else { end - last };
    }
    (start, end)
}

fn get_cursor_index(
    db: &Database,
    after: Option<String>,
    before: Option<String>,
) -> Result<(Option<usize>, Option<usize>)> {
    let mut after_idx: Option<usize> = None;
    let mut before_idx: Option<usize> = None;
    if let Some(cursor) = after {
        if let Ok(ret) = db.get_tree_loc(&String::from_utf8(base64::decode(cursor)?)?) {
            after_idx = Some(ret);
        }
    } else if let Some(cursor) = before {
        if let Ok(ret) = db.get_tree_loc(&String::from_utf8(base64::decode(cursor)?)?) {
            before_idx = Some(ret);
        }
    }
    Ok((after_idx, before_idx))
}

pub fn schema(database: Database) -> Schema<Query, EmptyMutation, EmptySubscription> {
    Schema::build(Query, EmptyMutation, EmptySubscription)
        .data(database)
        .finish()
}
