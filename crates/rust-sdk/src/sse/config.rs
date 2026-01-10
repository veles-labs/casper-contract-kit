use std::path::{Path, PathBuf};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ListenerConfigError {
    #[error("missing SSE endpoint URL")]
    MissingEndpoint,
}

#[derive(Clone, Debug)]
pub struct ListenerConfig {
    endpoint: String,
    timestamp_path: Option<PathBuf>,
}

impl ListenerConfig {
    pub fn builder() -> ListenerConfigBuilder {
        ListenerConfigBuilder::new()
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub fn timestamp_path(&self) -> Option<&Path> {
        self.timestamp_path.as_deref()
    }
}

#[derive(Debug, Default)]
pub struct ListenerConfigBuilder {
    endpoint: Option<String>,
    timestamp_path: Option<PathBuf>,
}

impl ListenerConfigBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = Some(endpoint.into());
        self
    }

    pub fn with_timestamp_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.timestamp_path = Some(path.into());
        self
    }

    pub fn build(self) -> Result<ListenerConfig, ListenerConfigError> {
        let endpoint = self
            .endpoint
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .ok_or(ListenerConfigError::MissingEndpoint)?;

        Ok(ListenerConfig {
            endpoint,
            timestamp_path: self.timestamp_path,
        })
    }
}
