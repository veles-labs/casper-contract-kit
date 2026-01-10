use std::path::PathBuf;

use clap::Parser;
use futures::StreamExt;
use tracing_subscriber::EnvFilter;

use veles_casper_rust_sdk::sse::config::ListenerConfig;
use veles_casper_rust_sdk::sse::event::SseEvent;

#[derive(Debug, Parser)]
#[command(name = "sse_listener")]
#[command(about = "Stream Casper SSE events as JSON", long_about = None)]
struct Cli {
    endpoint: String,
    #[arg(long = "timestamp-path")]
    timestamp_path: Option<PathBuf>,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    let mut builder = ListenerConfig::builder().with_endpoint(cli.endpoint);
    if let Some(path) = cli.timestamp_path {
        builder = builder.with_timestamp_path(path);
    }
    let config = builder.build()?;

    let mut stream = Box::pin(veles_casper_rust_sdk::sse::listener(config).await?);

    while let Some(event) = stream.next().await {
        match event {
            Ok(event) => {
                if matches!(event, SseEvent::FinalitySignature(_)) {
                    continue;
                }
                match serde_json::to_string(&event) {
                    Ok(json) => println!("{json}"),
                    Err(err) => eprintln!("failed to serialize event: {err}"),
                }
            }
            Err(err) => {
                eprintln!("listener error: {err}");
                break;
            }
        }
    }

    Ok(())
}
