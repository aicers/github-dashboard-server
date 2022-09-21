mod issue;
mod pull_request;

pub use self::issue::Issue;
pub use self::pull_request::PullRequest;
use crate::database::Database;
use async_graphql::{
    types::connection::{Connection, Edge, EmptyFields},
    EmptyMutation, EmptySubscription, MergedObject, OutputType, Result,
};
use sled::Tree;
use std::fmt::Display;

/// The default page size for connections when neither `first` nor `last` is
/// provided.
const DEFAULT_PAGE_SIZE: usize = 100;

/// A set of queries defined in the schema.
///
/// This is exposed only for [`Schema`], and not used directly.
#[derive(Default, MergedObject)]
pub struct Query(issue::IssueQuery, pull_request::PullRequestQuery);

pub type Schema = async_graphql::Schema<Query, EmptyMutation, EmptySubscription>;

#[derive(Debug)]
pub enum PagingType {
    First(usize),
    Last(usize),
    AfterFirst(String, usize),
    BeforeLast(String, usize),
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
    if let Some(cursor) = after {
        return Ok(PagingType::AfterFirst(
            String::from_utf8(base64::decode(cursor)?)?,
            DEFAULT_PAGE_SIZE,
        ));
    } else if let Some(cursor) = before {
        return Ok(PagingType::BeforeLast(
            String::from_utf8(base64::decode(cursor)?)?,
            DEFAULT_PAGE_SIZE,
        ));
    }
    Ok(PagingType::First(DEFAULT_PAGE_SIZE))
}

pub fn schema(database: Database) -> Schema {
    Schema::build(Query::default(), EmptyMutation, EmptySubscription)
        .data(database)
        .finish()
}

#[cfg(test)]
struct TestSchema {
    _dir: tempfile::TempDir, // to prevent the data directory from being deleted while the test is running
    db: Database,
    schema: Schema,
}

#[cfg(test)]
impl TestSchema {
    fn new() -> Self {
        let db_dir = tempfile::tempdir().unwrap();
        let db = Database::connect(db_dir.path()).unwrap();
        let schema = schema(db.clone());
        Self {
            _dir: db_dir,
            db,
            schema,
        }
    }

    async fn execute(&self, query: &str) -> async_graphql::Response {
        let request: async_graphql::Request = query.into();
        self.schema.execute(request).await
    }
}
