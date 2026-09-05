use api::middleware::rate_limit::IpRateLimiter;
use std::net::IpAddr;
use std::time::Duration;

#[test]
fn quota_then_reject_per_ip() {
    let lim = IpRateLimiter::new(2, Duration::from_secs(60));
    let ip: IpAddr = "10.0.0.1".parse().unwrap();
    assert!(lim.allow_ip(ip));
    assert!(lim.allow_ip(ip));
    assert!(!lim.allow_ip(ip));
    let other: IpAddr = "10.0.0.2".parse().unwrap();
    assert!(lim.allow_ip(other));
}
