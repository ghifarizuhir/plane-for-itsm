use std::time::Duration;

use api::middleware::rate_limit::bucket_allows;

#[test]
fn bucket_allows_burst_within_quota() {
    assert_eq!(bucket_allows(2, Duration::from_secs(60), 2, Duration::from_secs(0)), vec![true, true]);
}

#[test]
fn bucket_rejects_burst_over_quota() {
    assert_eq!(
        bucket_allows(2, Duration::from_secs(60), 3, Duration::from_secs(0)),
        vec![true, true, false]
    );
}

#[test]
fn bucket_refills_over_time() {
    // quota 1/min: immediate retry rejected, retry after a minute allowed.
    assert_eq!(
        bucket_allows(1, Duration::from_secs(60), 3, Duration::from_secs(60)),
        vec![true, true, true]
    );
    assert_eq!(
        bucket_allows(1, Duration::from_secs(60), 2, Duration::from_secs(0)),
        vec![true, false]
    );
}
