use std::net::SocketAddr;

pub async fn serve(socketaddr: SocketAddr) {
    warp::serve(warp::fs::dir("./")).run(socketaddr).await;
}
