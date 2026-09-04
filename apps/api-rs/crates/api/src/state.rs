#[derive(Clone)]
pub struct AppState {
    pub pool: sqlx::PgPool,
    pub redis: redis::aio::ConnectionManager,
}
