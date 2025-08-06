use std::{convert::Infallible, net::SocketAddr, path::Path};

use async_graphql::http::{playground_source, GraphQLPlaygroundConfig};
use warp::{http::Response as HttpResponse, Filter};

use crate::api::Schema;

pub(super) async fn serve(schema: Schema, socketaddr: SocketAddr, key: &Path, cert: &Path) {
    let filter = async_graphql_warp::graphql(schema).and_then(
        |(schema, request): (Schema, async_graphql::Request)| async move {
            let resp = schema.execute(request).await;

            Ok::<_, Infallible>(async_graphql_warp::GraphQLResponse::from(resp))
        },
    );

    let graphql_playground = warp::path!("graphql" / "playground").map(|| {
        HttpResponse::builder()
            .header("content-type", "text/html")
            .body(playground_source(GraphQLPlaygroundConfig::new("/graphql")))
    });

    let route_graphql = warp::path("graphql").and(warp::any()).and(filter);
    let route_home = warp::path::end().map(|| "");

    let routes = graphql_playground.or(warp::post().and(route_graphql.or(route_home)));

    warp::serve(routes)
        .tls()
        .key_path(key)
        .cert_path(cert)
        .run(socketaddr)
        .await;
}
