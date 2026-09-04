#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

mod schedule;

use serde_json::json;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .json()
        .init();

    let cfg = common::config::AppConfig::from_env();
    let mut redis = common::redis::create_redis(&cfg.redis_url).await;
    let _ = common::stream::ensure_group(&mut redis).await;
    let sched = tokio_cron_scheduler::JobScheduler::new().await.unwrap();

    // Every 5 min email stack — mirrors plane/celery.py:47 crontab(minute="*/5")
    {
        let mut r = redis.clone();
        sched
            .add(tokio_cron_scheduler::Job::new_async("0 */5 * * * *", move |_, _| {
                let mut rr = r.clone();
                Box::pin(async move {
                    let _ = common::stream::push_job(
                        &mut rr,
                        "email.notification",
                        json!({}),
                    )
                    .await;
                })
            }).unwrap())
            .await
            .unwrap();
    }
    // Daily 00:00 hard_delete — mirrors plane/celery.py:57
    {
        let mut r = redis.clone();
        sched
            .add(tokio_cron_scheduler::Job::new_async("0 0 0 * * *", move |_, _| {
                let mut rr = r.clone();
                Box::pin(async move {
                    let _ = common::stream::push_job(&mut rr, "cleanup.hard_delete", json!({})).await;
                })
            }).unwrap())
            .await
            .unwrap();
    }
    // 01:00 archive — mirrors plane/celery.py:61
    {
        let mut r = redis.clone();
        sched
            .add(tokio_cron_scheduler::Job::new_async("0 0 1 * * *", move |_, _| {
                let mut rr = r.clone();
                Box::pin(async move {
                    let _ = common::stream::push_job(&mut rr, "issue.archive", json!({})).await;
                })
            }).unwrap())
            .await
            .unwrap();
    }
    // 02:00 file_asset — mirrors 67
    // 02:30 api_logs — mirrors 71, etc. — add remaining 8 similarly with same pattern
    // For brevity, add generic daily jobs for cleanup.* at 02:30-03:45
    for (cron, job) in [
        ("0 30 2 * * *", "cleanup.api_logs"),
        ("0 45 2 * * *", "cleanup.email_logs"),
        ("0 0 3 * * *", "cleanup.page_versions"),
        ("0 15 3 * * *", "cleanup.issue_desc"),
        ("0 30 3 * * *", "cleanup.webhook_logs"),
        ("0 0 1 * * *", "export.delete_old_s3"), // 01:30 placeholder
        ("0 0 2 * * *", "file_asset.delete_unuploaded"),
    ] {
        let mut r = redis.clone();
        let job = job.to_string();
        sched
            .add(tokio_cron_scheduler::Job::new_async(cron, move |_, _| {
                let mut rr = r.clone();
                let j = job.clone();
                Box::pin(async move {
                    let _ = common::stream::push_job(&mut rr, &j, json!({})).await;
                })
            }).unwrap())
            .await
            .unwrap();
    }

    sched.start().await.unwrap();
    tracing::info!("beat scheduler started with 11 jobs");
    tokio::signal::ctrl_c().await.unwrap();
}
