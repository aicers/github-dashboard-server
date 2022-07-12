mod web;

#[tokio::main]
async fn main() {
    println!("AICE GitHub Dashboard Server");

    web::serve().await;
}
