//! CLI entry point; the shared acceptance runner is also used by private consumers.

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    e2e::run().await
}
