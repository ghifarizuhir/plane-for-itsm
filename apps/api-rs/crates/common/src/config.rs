use std::env;

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub database_url: String,
    pub redis_url: String,
    pub port: u16,
    pub jwt_secret: String,
    pub cookie_secure: bool,
    pub frontend_url: String,
    pub github_client_id: String,
    pub github_client_secret: String,
    pub google_client_id: String,
    pub google_client_secret: String,
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
            jwt_secret: env::var("JWT_SECRET").unwrap_or_else(|_| {
                eprintln!("JWT_SECRET unset — using insecure dev default");
                "dev-only-insecure".into()
            }),
            cookie_secure: env::var("COOKIE_SECURE").map(|v| v == "1").unwrap_or(false),
            frontend_url: env::var("FRONTEND_URL")
                .unwrap_or_else(|_| "http://localhost:3000".into()),
            github_client_id: env::var("GITHUB_CLIENT_ID").unwrap_or_default(),
            github_client_secret: env::var("GITHUB_CLIENT_SECRET").unwrap_or_default(),
            google_client_id: env::var("GOOGLE_CLIENT_ID").unwrap_or_default(),
            google_client_secret: env::var("GOOGLE_CLIENT_SECRET").unwrap_or_default(),
        }
    }
}
