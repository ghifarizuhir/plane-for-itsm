use std::env;

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub database_url: String,
    pub redis_url: String,
    pub port: u16,
}

impl AppConfig {
    pub fn from_env() -> Self {
        Self {
            database_url: env::var("DATABASE_URL")
                .unwrap_or_else(|_| "postgres://plane:plane@plane-db:5432/plane".into()),
            redis_url: env::var("REDIS_URL")
                .unwrap_or_else(|_| "redis://plane-redis:6379".into()),
            port: env::var("PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(8001),
        }
    }
}
