//! Standalone evaluation runner CLI for NaviFS MCP tools.

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let report = navifs_eval::run_evaluation().await?;
    println!("\n📊 Final Evaluation Report:\n{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
