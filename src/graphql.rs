use crate::{database::Database, ISSUE_TREE_NAME, PR_TREE_NAME};
use anyhow::{anyhow, bail, Result};
use async_graphql::{
    types::connection::{query, Connection, Edge, EmptyFields},
    Context, EmptyMutation, EmptySubscription, Object, OutputType, Schema, SimpleObject,
};
use sled::Tree;
use std::fmt::Display;

pub struct Query;

#[derive(Debug, SimpleObject)]
pub struct Issue {
    pub owner: String,
    pub repo: String,
    pub number: i32,
    pub title: String,
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
    pub async fn issues<'ctx>(
        &self,
        ctx: &Context<'ctx>,
        after: Option<String>,
        before: Option<String>,
        first: Option<i32>,
        last: Option<i32>,
    ) -> Result<Connection<String, Issue, EmptyFields, EmptyFields>> {
        match query(
            after,
            before,
            first,
            last,
            |after, before, first, last| async move {
                let db = match ctx.data::<Database>() {
                    Ok(ret) => ret,
                    Err(e) => bail!("{:?}", e),
                };
                let tree = match db.tree(ISSUE_TREE_NAME) {
                    Ok(ret) => ret,
                    Err(e) => bail!("{:?}", e),
                };
                let p_type = check_paging_type(after, before, first, last)?;
                let select_vec = Database::select_issue_range(p_type, tree)?;
                let (prev, next) =
                    check_prev_next_exist(select_vec.first(), select_vec.last(), tree)?;
                Ok(connect_cursor(select_vec, prev, next))
            },
        )
        .await
        {
            Ok(conn) => Ok(conn),
            Err(e) => Err(anyhow!("{:?}", e)),
        }
    }

    #[allow(non_snake_case)]
    pub async fn pullRequests<'ctx>(
        &self,
        ctx: &Context<'ctx>,
        after: Option<String>,
        before: Option<String>,
        first: Option<i32>,
        last: Option<i32>,
    ) -> Result<Connection<String, PullRequest, EmptyFields, EmptyFields>> {
        match query(
            after,
            before,
            first,
            last,
            |after, before, first, last| async move {
                let db = match ctx.data::<Database>() {
                    Ok(ret) => ret,
                    Err(e) => bail!("{:?}", e),
                };

                let tree = match db.tree(PR_TREE_NAME) {
                    Ok(ret) => ret,
                    Err(e) => bail!("{:?}", e),
                };

                let p_type = check_paging_type(after, before, first, last)?;
                let select_vec = Database::select_pr_range(p_type, tree)?;
                let (prev, next) =
                    check_prev_next_exist(select_vec.first(), select_vec.last(), tree)?;
                Ok(connect_cursor(select_vec, prev, next))
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

fn check_prev_next_exist<T>(prev: Option<&T>, next: Option<&T>, tree: &Tree) -> Result<(bool, bool)>
where
    T: OutputType + Display,
{
    if let Some(prev_val) = prev {
        if let Some(next_val) = next {
            return Ok((
                Database::get_prev_exist(format!("{}", prev_val), tree)?,
                Database::get_next_exist(format!("{}", next_val), tree)?,
            ));
        }
    }
    Err(anyhow!("Wrong Issue value"))
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

pub fn schema(database: Database) -> Schema<Query, EmptyMutation, EmptySubscription> {
    Schema::build(Query, EmptyMutation, EmptySubscription)
        .data(database)
        .finish()
}
