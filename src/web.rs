use crate::graphql;
use async_graphql::{EmptyMutation, EmptySubscription, Schema};
use std::{convert::Infallible, net::SocketAddr};
use warp::Filter;

pub async fn serve(
    schema: Schema<graphql::Query, EmptyMutation, EmptySubscription>,
    socketaddr: SocketAddr,
) {
    type MySchema = Schema<graphql::Query, EmptyMutation, EmptySubscription>;
    let filter = async_graphql_warp::graphql(schema).and_then(
        |(schema, request): (MySchema, async_graphql::Request)| async move {
            let resp = schema.execute(request).await;

            Ok::<_, Infallible>(async_graphql_warp::GraphQLResponse::from(resp))
        },
    );

    let route_graphql = warp::path("graphql").and(warp::any()).and(filter);
    let route_home = warp::path::end().map(|| "");
    let routes = warp::post().and(route_graphql.or(route_home));

    warp::serve(routes).run(socketaddr).await;
}
