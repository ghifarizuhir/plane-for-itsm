use redis::aio::ConnectionManager;

pub async fn create_redis(url: &str) -> ConnectionManager {
    let client = redis::Client::open(url).expect("redis client open failed");
    ConnectionManager::new(client)
        .await
        .expect("redis connect failed")
}
