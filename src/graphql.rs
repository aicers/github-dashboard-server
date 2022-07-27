use async_graphql::{EmptyMutation, EmptySubscription, Object, Schema, SimpleObject};

pub struct Query;

#[derive(SimpleObject)]
pub struct Issue {
    owner: String,
    repo: String,
    number: i32,
    title: String,
}

#[Object]
impl Query {
    pub async fn issues(&self) -> Vec<Issue> {
        Vec::new()
    }
}

pub fn schema() -> Schema<Query, EmptyMutation, EmptySubscription> {
    Schema::new(Query, EmptyMutation, EmptySubscription)
}
