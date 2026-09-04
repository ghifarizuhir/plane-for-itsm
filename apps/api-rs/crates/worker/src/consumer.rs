use redis::aio::ConnectionManager;

pub async fn fetch_one(
    mgr: &mut ConnectionManager,
    consumer: &str,
) -> anyhow::Result<Vec<String>> {
    common::stream::read_jobs(mgr, common::stream::GROUP, consumer, 1).await
}

pub async fn ack(mgr: &mut ConnectionManager, id: &str) -> anyhow::Result<()> {
    common::stream::ack_job(mgr, id).await
}
