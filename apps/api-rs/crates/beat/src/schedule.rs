use redis::aio::ConnectionManager;
use serde_json::json;

pub async fn push_once(mgr: &mut ConnectionManager, job: &str) -> anyhow::Result<()> {
    common::stream::push_job(mgr, job, json!({})).await?;
    Ok(())
}
