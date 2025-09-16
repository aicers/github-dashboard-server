mod discussion;
pub(crate) mod discussion_stat;
pub(crate) mod issue;
pub(crate) mod issue_stat;
pub(crate) mod pull_request;
pub(crate) mod pull_request_stat;

use std::fmt::Display;

use async_graphql::{
    types::connection::{Connection, Edge, EmptyFields},
    Context, EmptyMutation, EmptySubscription, InputValueError, InputValueResult, MergedObject,
    OutputType, Result, Scalar, ScalarType, Value,
};
use base64::{engine::general_purpose, Engine as _};
use jiff::Timestamp;

pub(crate) use self::discussion::Discussion;
use crate::database::Database;

// The title of the github project.
pub(crate) const TODO_LIST_PROJECT_TITLE: &str = "to-do list";
// The status name of the "Done" column in the github project.
pub(crate) const TODO_LIST_STATUS_DONE: &str = "Done";

/// The default page size for connections when neither `first` nor `last` is provided.
const DEFAULT_PAGE_SIZE: usize = 100;

/// A set of queries defined in the schema.
///
/// This is exposed only for [`Schema`], and not used directly.
#[derive(Default, MergedObject)]
pub(crate) struct Query(
    discussion::DiscussionQuery,
    discussion_stat::DiscussionStatQuery,
    issue::IssueQuery,
    issue_stat::IssueStatQuery,
    pull_request::PullRequestQuery,
    pull_request_stat::PullRequestStatQuery,
);

pub(crate) type Schema = async_graphql::Schema<Query, EmptyMutation, EmptySubscription>;

#[derive(Debug, PartialEq, PartialOrd, Clone, Copy)]
pub(crate) struct DateTimeUtc(Timestamp);

#[Scalar]
impl ScalarType for DateTimeUtc {
    fn parse(value: Value) -> InputValueResult<Self> {
        match &value {
            Value::String(s) => Ok(DateTimeUtc(s.parse()?)),
            _ => Err(InputValueError::expected_type(value)),
        }
    }

    fn to_value(&self) -> Value {
        Value::String(self.0.to_string())
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
        connection.edges.push(Edge::new(
            general_purpose::STANDARD.encode(format!("{output}")),
            output,
        ));
    }
    connection
}

pub(crate) fn schema(database: Database) -> Schema {
    Schema::build(Query::default(), EmptyMutation, EmptySubscription)
        .data(database)
        .finish()
}

fn load_connection<N, I>(
    ctx: &Context<'_>,
    iter_builder: impl Fn(&Database, Option<&[u8]>, Option<&[u8]>) -> I,
    after: Option<String>,
    before: Option<String>,
    first: Option<usize>,
    last: Option<usize>,
) -> Result<Connection<String, N, EmptyFields, EmptyFields>>
where
    N: Display + OutputType,
    I: DoubleEndedIterator<Item = anyhow::Result<N>>,
{
    let db = ctx.data::<Database>()?;
    let (nodes, has_previous, has_next) = if let Some(before) = before {
        if after.is_some() {
            return Err("cannot use both `after` and `before`".into());
        }
        if first.is_some() {
            return Err("'before' and 'first' cannot be specified simultaneously".into());
        }
        let last = last.unwrap_or(DEFAULT_PAGE_SIZE);
        let cursor = general_purpose::STANDARD.decode(before)?;
        let iter = iter_builder(db, None, Some(cursor.as_slice())).rev();
        let (mut nodes, has_previous) = collect_nodes(iter, last)?;
        nodes.reverse();
        (nodes, has_previous, false)
    } else if let Some(after) = after {
        if before.is_some() {
            return Err("cannot use both `after` and `before`".into());
        }
        if last.is_some() {
            return Err("'after' and 'last' cannot be specified simultaneously".into());
        }
        let first = first.unwrap_or(DEFAULT_PAGE_SIZE);
        let cursor = general_purpose::STANDARD.decode(after)?;
        let iter = iter_builder(db, Some(cursor.as_slice()), None);
        let (nodes, has_next) = collect_nodes(iter, first)?;
        (nodes, false, has_next)
    } else if let Some(last) = last {
        if first.is_some() {
            return Err("first and last cannot be used together".into());
        }
        let iter = iter_builder(db, None, None).rev();
        let (mut nodes, has_previous) = collect_nodes(iter, last)?;
        nodes.reverse();
        (nodes, has_previous, false)
    } else {
        let first = first.unwrap_or(DEFAULT_PAGE_SIZE);
        let iter = iter_builder(db, None, None);
        let (nodes, has_next) = collect_nodes(iter, first)?;
        (nodes, false, has_next)
    };
    Ok(connect_cursor(nodes, has_previous, has_next))
}

fn collect_nodes<I, T>(mut iter: I, size: usize) -> Result<(Vec<T>, bool)>
where
    I: Iterator<Item = anyhow::Result<T>>,
{
    let mut nodes = Vec::with_capacity(size);
    let mut has_more = false;
    while let Some(node) = iter.next() {
        let node = node.map_err(|e| format!("failed to read database: {e}"))?;
        nodes.push(node);
        if nodes.len() == size {
            has_more = iter.next().is_some();
            break;
        }
    }
    Ok((nodes, has_more))
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
