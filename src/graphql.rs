use async_graphql::{EmptyMutation, EmptySubscription, Object, Schema};

pub struct Query;

#[Object]
impl Query {
    pub async fn issues(&self) -> Vec<i32> {
        Vec::new()
    }
}

pub fn schema() -> Schema<Query, EmptyMutation, EmptySubscription> {
    Schema::new(Query, EmptyMutation, EmptySubscription)
}
