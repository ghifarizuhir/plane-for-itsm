use redis::{aio::ConnectionManager, AsyncCommands};
use serde_json::Value;

pub const STREAM: &str = "plane:jobs";
pub const GROUP: &str = "workers";

pub async fn ensure_group(mgr: &mut ConnectionManager) -> anyhow::Result<()> {
    let res: Result<String, redis::RedisError> = redis::cmd("XGROUP")
        .arg("CREATE")
        .arg(STREAM)
        .arg(GROUP)
        .arg("0")
        .arg("MKSTREAM")
        .query_async(mgr)
        .await;
    match res {
        Ok(_) => Ok(()),
        Err(e) if e.to_string().contains("BUSYGROUP") => Ok(()),
        Err(e) => anyhow::bail!(e),
    }
}

pub async fn push_job(
    mgr: &mut ConnectionManager,
    job: &str,
    payload: Value,
) -> anyhow::Result<String> {
    let id: String = mgr
        .xadd(STREAM, "*", &[("job", job), ("payload", &payload.to_string())])
        .await?;
    Ok(id)
}

pub async fn read_jobs(
    mgr: &mut ConnectionManager,
    group: &str,
    consumer: &str,
    count: usize,
) -> anyhow::Result<Vec<String>> {
    let opts = redis::streams::StreamReadOptions::default()
        .group(group, consumer)
        .count(count)
        .block(500);
    let reply: redis::streams::StreamReadReply =
        mgr.xread_options(&[STREAM], &[">"], &opts).await?;
    Ok(reply
        .keys
        .into_iter()
        .flat_map(|k| k.ids.into_iter().map(|id| id.id))
        .collect())
}

pub async fn ack_job(mgr: &mut ConnectionManager, id: &str) -> anyhow::Result<()> {
    let _: i32 = mgr.xack(STREAM, GROUP, &[id]).await?;
    Ok(())
}
