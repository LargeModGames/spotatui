use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
  spotatui::run_cli().await
}
