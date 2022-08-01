use crate::database::Database;
use anyhow::{anyhow, Result};
use async_graphql::{Context, EmptyMutation, EmptySubscription, Object, Schema, SimpleObject};

pub struct Query;

#[derive(SimpleObject)]
pub struct Issue {
    pub owner: String,
    pub repo: String,
    pub number: i32,
    pub title: String,
}

#[Object]
impl Query {
    pub async fn issues<'ctx>(&self, ctx: &Context<'ctx>) -> Result<Vec<Issue>> {
        match ctx.data::<Database>() {
            Ok(db_conn_pool) => Ok(db_conn_pool.select_all()?),
            Err(error) => Err(anyhow!("{:?}", error)),
        }
    }
}

pub fn schema(database: Database) -> Schema<Query, EmptyMutation, EmptySubscription> {
    Schema::build(Query, EmptyMutation, EmptySubscription)
        .data(database)
        .finish()
}
