use redis::{aio::ConnectionManager, streams::StreamMaxlen, AsyncCommands, FromRedisValue};
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
    // `MAXLEN ~ 10000` keeps the stream bounded; acked entries are never read
    // again and in-flight backlog is far below this bound.
    let id: String = mgr
        .xadd_maxlen(
            STREAM,
            StreamMaxlen::Approx(10000),
            "*",
            &[("job", job), ("payload", &payload.to_string())],
        )
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

/// Read one stream entry by id and parse its `job` + JSON `payload` fields.
/// `Ok(None)` means no entry with that exact id exists; a found-but-malformed
/// entry is an error. Callers must pass the full id (e.g. from `XREADGROUP`);
/// partial millisecond ids would make `XRANGE` match the first entry of that ms.
pub async fn job_by_id(
    mgr: &mut ConnectionManager,
    id: &str,
) -> anyhow::Result<Option<(String, Value)>> {
    let reply: redis::streams::StreamRangeReply = mgr.xrange(STREAM, id, id).await?;
    let Some(entry) = reply.ids.into_iter().next() else {
        return Ok(None);
    };
    if entry.id != id {
        return Ok(None);
    }
    let fields: Vec<(String, String)> = entry
        .map
        .into_iter()
        .map(|(key, value)| (key, String::from_redis_value(&value).unwrap_or_default()))
        .collect();
    match parse_entry(&fields) {
        Some(parsed) => Ok(Some(parsed)),
        None => anyhow::bail!("malformed stream entry {id}"),
    }
}

/// Pure parser for a stream entry's fields.
pub fn parse_entry(fields: &[(String, String)]) -> Option<(String, Value)> {
    let get = |name: &str| {
        fields
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    };
    let job = get("job")?.to_string();
    let payload = serde_json::from_str(get("payload")?).ok()?;
    Some((job, payload))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_entry_reads_job_and_payload() {
        let fields = vec![
            ("job".to_string(), "ai.schedule.run".to_string()),
            ("payload".to_string(), r#"{"run_id":"abc"}"#.to_string()),
        ];
        let (job, payload) = parse_entry(&fields).expect("parsed");
        assert_eq!(job, "ai.schedule.run");
        assert_eq!(payload, json!({"run_id": "abc"}));
    }

    #[test]
    fn parse_entry_rejects_missing_or_bad_fields() {
        assert!(parse_entry(&[]).is_none());
        assert!(parse_entry(&[("job".to_string(), "x".to_string())]).is_none());
        assert!(parse_entry(&[
            ("job".to_string(), "x".to_string()),
            ("payload".to_string(), "not-json".to_string())
        ])
        .is_none());
    }
}
