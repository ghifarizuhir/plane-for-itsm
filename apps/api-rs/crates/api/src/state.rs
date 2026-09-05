#[derive(Clone)]
pub struct AppState {
    pub pool: sqlx::PgPool,
    pub redis: redis::Client,
    pub config: common::config::AppConfig,
}

impl AppState {
    pub async fn redis_client(&self) -> redis::RedisResult<redis::aio::ConnectionManager> {
        self.redis.get_connection_manager().await
    }
}
