//! SSE (Server-Sent Events) listener for Casper blockchain
pub mod config;
pub mod event;

use std::path::PathBuf;

use async_stream::stream;
use futures::StreamExt;
use reqwest_eventsource::{Event, EventSource};
use thiserror::Error;
use tokio::sync::mpsc;
use tracing::{debug, error, info, trace, warn};
use url::Url;

use crate::sse::{config::ListenerConfig, event::SseEvent};

#[derive(Debug, Error)]
pub enum ListenerError {
    #[error("unexpected SSE event type: {0}")]
    UnexpectedEventType(String),
    #[error("failed to decode SSE event payload: {head}")]
    Decode {
        head: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("invalid SSE endpoint URL: {0}")]
    Url(#[from] url::ParseError),
    #[error("event source error: {0}")]
    EventSource(#[from] reqwest_eventsource::Error),
    #[error("blocking task error: {0}")]
    TaskJoin(#[from] tokio::task::JoinError),
}

pub async fn listener(
    config: ListenerConfig,
) -> Result<impl futures::Stream<Item = Result<SseEvent, ListenerError>>, ListenerError> {
    info!("Starting listener for {}", config.endpoint());

    let endpoint = config.endpoint().to_string();
    let timestamp_path = config.timestamp_path().map(PathBuf::from);

    let mut url = Url::parse(&endpoint)?;
    if let Some(timestamp_path) = timestamp_path.as_deref() {
        match tokio::fs::read_to_string(timestamp_path).await {
            Ok(content) => {
                let last_id = content.trim();
                if last_id.is_empty() {
                    debug!("Timestamp file is empty, starting without start_from");
                } else {
                    url.query_pairs_mut().append_pair("start_from", last_id);
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                debug!(
                    "Timestamp file not found at {}, starting without start_from",
                    timestamp_path.display()
                );
            }
            Err(err) => {
                warn!(
                    "Failed to read timestamp file at {}: {:?}",
                    timestamp_path.display(),
                    err
                );
            }
        }
    }

    let endpoint_url = url.to_string();
    let (tx, mut rx) = mpsc::channel::<Result<SseEvent, ListenerError>>(256);
    let (raw_tx, mut raw_rx) = mpsc::channel::<String>(256);

    let parse_sender = tx.clone();

    // Task to parse raw event data into SseEvent
    tokio::spawn(async move {
        while let Some(data) = raw_rx.recv().await {
            let parse_result = match tokio::task::spawn_blocking(move || {
                let head = data.chars().take(100).collect::<String>();
                serde_json::from_str::<SseEvent>(&data)
                    .map_err(|source| ListenerError::Decode { head, source })
            })
            .await
            {
                Ok(result) => result,
                Err(err) => Err(ListenerError::TaskJoin(err)),
            };
            let _ = parse_sender.send(parse_result).await;
        }
    });

    // Task to receive events from the SSE endpoint
    tokio::spawn(async move {
        let mut es = EventSource::get(endpoint_url);
        trace!("Starting to receive events");

        while let Some(event) = es.next().await {
            match event {
                Ok(Event::Open) => {
                    info!("Connection opened");
                }
                Ok(Event::Message(message)) => {
                    if message.event != "message" {
                        let _ = tx
                            .send(Err(ListenerError::UnexpectedEventType(message.event)))
                            .await;
                        break;
                    }

                    if let Some(timestamp_path) = timestamp_path.as_ref() {
                        if message.id.is_empty() {
                            debug!("Skipping timestamp write; message id is empty");
                        } else if let Err(err) =
                            tokio::fs::write(timestamp_path, message.id.clone()).await
                        {
                            error!("Failed to write event id to file: {:?}", err);
                        }
                    }

                    // Push raw message data to the parser task, if it fails, we stop processing
                    // A bit overkill; but we don't want to stall the SSE stream, we want to keep
                    // ordering and we want everything nicely asynchronous as some of the JSONs
                    // may be huge.
                    if raw_tx.send(message.data).await.is_err() {
                        break;
                    }
                }
                Err(err) => {
                    error!("Error receiving event: {:?}", err);
                    let _ = tx.send(Err(ListenerError::EventSource(err))).await;
                    break;
                }
            }
        }

        trace!("Event stream ended");
    });

    Ok(stream! {
        while let Some(item) = rx.recv().await {
            yield item;
        }
    })
}
