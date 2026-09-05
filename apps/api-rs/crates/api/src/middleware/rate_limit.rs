use std::{
    collections::HashMap,
    net::IpAddr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde_json::json;

/// Token-bucket backstop mirroring DRF throttle intent
/// (`plane/settings/common.py`: anon 30/min, API-key 60/min).
/// Process-wide bucket (not per-key): per-key Redis accounting stays a
/// follow-up. Unlike `tower::limit::RateLimit` (which *delays* excess
/// requests), this rejects with 429 immediately.
#[derive(Debug)]
struct Bucket {
    capacity: f64,
    tokens: f64,
    last: Instant,
    refill_per_sec: f64,
}

impl Bucket {
    fn new(quota: u64, per: Duration) -> Self {
        Self {
            capacity: quota as f64,
            tokens: quota as f64,
            last: Instant::now(),
            refill_per_sec: quota as f64 / per.as_secs_f64(),
        }
    }

    /// Pure decision step (unit-testable): refill by elapsed, take one.
    fn allow(&mut self, now: Instant) -> bool {
        let elapsed = now.duration_since(self.last).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.refill_per_sec).min(self.capacity);
        self.last = now;
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

#[derive(Debug, Clone)]
pub struct RateLimiter {
    bucket: Arc<Mutex<Bucket>>,
}

impl RateLimiter {
    pub fn new(quota: u64, per: Duration) -> Self {
        Self {
            bucket: Arc::new(Mutex::new(Bucket::new(quota, per))),
        }
    }

    fn allow(&self) -> bool {
        self.bucket.lock().map(|mut b| b.allow(Instant::now())).unwrap_or(true)
    }
}

/// Test helper (exercised by `tests/rate_limit_test.rs` through the lib
/// target): quota burst passes, excess rejected, refill over time.
#[allow(dead_code)]
pub fn bucket_allows(quota: u64, per: Duration, attempts: usize, spacing: Duration) -> Vec<bool> {
    let mut bucket = Bucket::new(quota, per);
    let start = Instant::now();
    (0..attempts)
        .map(|i| bucket.allow(start + spacing * i as u32))
        .collect()
}

pub async fn rate_limit_middleware(
    State(limiter): State<RateLimiter>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    if limiter.allow() {
        Ok(next.run(req).await)
    } else {
        Err(StatusCode::TOO_MANY_REQUESTS)
    }
}

#[derive(Debug, Clone)]
pub struct IpRateLimiter {
    buckets: Arc<Mutex<HashMap<IpAddr, Bucket>>>,
    quota: u64,
    per: Duration,
}

impl IpRateLimiter {
    pub fn new(quota: u64, per: Duration) -> Self {
        Self { buckets: Arc::new(Mutex::new(HashMap::new())), quota, per }
    }
    pub fn allow_ip(&self, ip: IpAddr) -> bool {
        let mut map = self.buckets.lock().unwrap();
        if map.len() > 10_000 {
            map.clear();
        }
        map.entry(ip).or_insert_with(|| Bucket::new(self.quota, self.per)).allow(Instant::now())
    }
}

/// Ekstrak IP: X-Forwarded-For pertama → ConnectInfo → loopback.
pub fn client_ip(req: &Request, fallback: IpAddr) -> IpAddr {
    if let Some(xff) = req.headers().get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
        if let Some(first) = xff.split(',').next() {
            if let Ok(ip) = first.trim().parse() {
                return ip;
            }
        }
    }
    req.extensions()
        .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
        .map(|c| c.0.ip())
        .unwrap_or(fallback)
}

pub async fn ip_rate_limit_middleware(
    State(lim): State<IpRateLimiter>,
    req: Request,
    next: Next,
) -> Response {
    let ip = client_ip(&req, IpAddr::from([127, 0, 0, 1]));
    if lim.allow_ip(ip) {
        next.run(req).await
    } else {
        (StatusCode::TOO_MANY_REQUESTS, axum::Json(json!({"error": "rate limit exceeded"}))).into_response()
    }
}
