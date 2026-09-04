#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

mod consumer;
mod handlers;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .json()
        .init();

    let cfg = common::config::AppConfig::from_env();
    let pool = common::db::create_pool(&cfg).await;
    if let Err(e) = common::db::migrate(&pool).await {
        tracing::warn!(error=%e, "migrate failed");
    }
    let mut redis = common::redis::create_redis(&cfg.redis_url).await;
    if let Err(e) = common::stream::ensure_group(&mut redis).await {
        tracing::warn!(error=%e, "ensure_group failed");
    }
    tracing::info!("worker listening on stream {}", common::stream::STREAM);
    loop {
        let ids = consumer::fetch_one(&mut redis, "worker-1")
            .await
            .unwrap_or_default();
        for id in ids {
            if let Err(e) = handlers::handle_by_id(&pool, &mut redis, &id).await {
                tracing::error!(id=%id, error=%e, "handle failed");
            }
            if let Err(e) = consumer::ack(&mut redis, &id).await {
                tracing::warn!(id=%id, error=%e, "ack failed");
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
}
