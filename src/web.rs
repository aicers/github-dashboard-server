const IP_ADDR: [u8; 4] = [127, 0, 0, 1];
const PORT: u16 = 8000;

pub async fn serve() {
    warp::serve(warp::fs::dir("./")).run((IP_ADDR, PORT)).await;
}
