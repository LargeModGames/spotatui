use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
  spotatui::runtime::run().await
}
