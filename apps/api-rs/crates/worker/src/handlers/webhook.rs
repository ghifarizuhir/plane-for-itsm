use serde_json::Value;

pub async fn handle(payload: Value) -> anyhow::Result<()> {
    let url = payload.get("url").and_then(|v| v.as_str()).unwrap_or("");
    tracing::info!(url=%url, "webhook dispatched");
    if url.is_empty() {
        anyhow::bail!("missing url");
    }
    Ok(())
}
