mod cli;
mod core;
#[cfg(test)]
mod gates;
mod infra;
mod runtime;
mod tui;

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
  runtime::run().await
}
