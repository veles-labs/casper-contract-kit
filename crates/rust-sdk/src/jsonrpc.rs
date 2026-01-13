//! JSONRPC client for interacting with a Casper network.
use casper_client::{self, JsonRpcId, Verbosity};
pub use casper_client::{
    Error as CasperClientRpcError,
    cli::TransactionV1BuilderError,
    rpcs::{
        AccountIdentifier,
        common::BlockIdentifier,
        results::{
            GetAccountResult, GetBlockResult, GetChainspecResult, GetStateRootHashResult,
            GetTransactionResult, PutTransactionResult, SpeculativeExecTxnResult,
        },
    },
};

use casper_types::{Digest, Transaction, TransactionHash, U512, crypto::ErrorExt};
use rand::Rng;
use thiserror::Error;
use toml::Value as TomlValue;

/// JSONRPC client for interacting with a Casper network sidecar instance.
#[derive(Clone, Debug)]
pub struct CasperClient {
    network_name: String,
    rpc_endpoints: Vec<String>,
    verbosity: Verbosity,
}

impl CasperClient {
    /// Creates a new client using the provided network name and RPC endpoints.
    ///
    /// At least one endpoint must be provided.
    pub fn new<N, I, S>(network_name: N, rpc_endpoints: I) -> Result<Self, CasperClientError>
    where
        N: Into<String>,
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let endpoints: Vec<String> = rpc_endpoints
            .into_iter()
            .filter_map(|endpoint| {
                let endpoint: String = endpoint.into();
                normalize_node_address(&endpoint)
            })
            .collect();

        if endpoints.is_empty() {
            return Err(CasperClientError::MissingRpcEndpoints);
        }

        Ok(Self {
            network_name: network_name.into(),
            rpc_endpoints: endpoints,
            // Verbosity is set to low by default to avoid cluttering of stdout.
            verbosity: Verbosity::Low,
        })
    }

    /// Returns the primary RPC endpoint configured for this client.
    pub fn rpc_endpoint(&self) -> &str {
        // safe: enforced in `new`.
        self.rpc_endpoints.first().expect("endpoint must exist")
    }

    /// Network name associated with this client.
    pub fn network_name(&self) -> &str {
        &self.network_name
    }

    /// Fetches the account information for the provided public key hex.
    pub async fn get_account(
        &self,
        account_identifier: AccountIdentifier,
    ) -> Result<Option<GetAccountResult>, CasperClientError> {
        match casper_client::get_account(
            next_rpc_id(),
            self.rpc_endpoint(),
            self.verbosity,
            None,
            account_identifier,
        )
        .await
        {
            Ok(response) => Ok(Some(response.result)),
            Err(CasperClientRpcError::ResponseIsRpcError { error, .. })
                if is_missing_account_error(error.code, &error.message) =>
            {
                Ok(None)
            }
            Err(error) => Err(CasperClientError::Client(Box::new(error))),
        }
    }

    /// Returns the latest state root hash as a `Digest`.
    pub async fn get_state_root_hash(&self) -> Result<Digest, CasperClientError> {
        let response = casper_client::get_state_root_hash(
            next_rpc_id(),
            self.rpc_endpoint(),
            self.verbosity,
            None,
        )
        .await?;

        {
            let result: &GetStateRootHashResult = &response.result;
            result
                .state_root_hash
                .ok_or(CasperClientError::MissingStateRootHash)
        }
    }

    /// Returns the balance (in motes) for the provided public key, if the account exists.
    pub async fn get_balance(
        &self,
        account_identifier: AccountIdentifier,
    ) -> Result<Option<U512>, CasperClientError> {
        let account = match self.get_account(account_identifier).await? {
            Some(result) => result,
            None => return Ok(None),
        };

        let main_purse = account.account.main_purse();
        let state_root = self.get_state_root_hash().await?;

        let response = casper_client::get_balance(
            next_rpc_id(),
            self.rpc_endpoint(),
            self.verbosity,
            state_root,
            main_purse,
        )
        .await?;

        let balance = response.result.balance_value;
        Ok(Some(balance))
    }

    /// Submits a pre-built transaction and returns the transaction hash.
    pub async fn put_transaction(
        &self,
        transaction: Transaction,
    ) -> Result<TransactionHash, CasperClientError> {
        let response = casper_client::put_transaction(
            next_rpc_id(),
            self.rpc_endpoint(),
            self.verbosity,
            transaction,
        )
        .await?;
        Ok(response.result.transaction_hash)
    }

    /// Downloads and parses the chainspec TOML as `toml::Value`.
    ///
    /// NOTE: This API may change in future and provide a deserialized `Chainspec` struct instead.
    pub async fn get_chainspec(&self) -> Result<TomlValue, CasperClientError> {
        let response =
            casper_client::get_chainspec(next_rpc_id(), self.rpc_endpoint(), self.verbosity)
                .await?;
        parse_chainspec(&response.result)
    }

    /// Reads the network name from the chainspec.
    pub async fn get_network_name(&self) -> Result<String, CasperClientError> {
        let chainspec = self.get_chainspec().await?;
        chainspec
            .get("network")
            .and_then(|value| value.get("name"))
            .and_then(TomlValue::as_str)
            .map(|value| value.to_string())
            .ok_or(CasperClientError::MissingNetworkName)
    }

    /// Fetches the transaction details for the provided transaction hash.
    pub async fn get_transaction(
        &self,
        transaction_hash: TransactionHash,
    ) -> Result<GetTransactionResult, CasperClientError> {
        let response = casper_client::get_transaction(
            next_rpc_id(),
            self.rpc_endpoint(),
            self.verbosity,
            transaction_hash,
            true,
        )
        .await?;
        Ok(response.result)
    }

    /// Performs a speculative execution of the provided transaction.
    pub async fn speculative_exec_txn(
        &self,
        transaction: Transaction,
    ) -> Result<SpeculativeExecTxnResult, CasperClientError> {
        let response = casper_client::speculative_exec_txn(
            next_rpc_id(),
            self.rpc_endpoint(),
            self.verbosity,
            transaction,
        )
        .await?;
        Ok(response.result)
    }

    pub async fn get_block(
        &self,
        block_identifier: Option<BlockIdentifier>,
    ) -> Result<GetBlockResult, CasperClientError> {
        let response = casper_client::get_block(
            next_rpc_id(),
            self.rpc_endpoint(),
            self.verbosity,
            block_identifier,
        )
        .await?;
        Ok(response.result)
    }
}

#[derive(Error, Debug)]
pub enum CasperClientError {
    #[error("no RPC endpoints configured")]
    MissingRpcEndpoints,
    #[error("casper client error: {0}")]
    Client(Box<CasperClientRpcError>),
    #[error("failed to parse chainspec response: {0}")]
    Chainspec(#[from] toml::de::Error),
    #[error("balance value exceeds u64 range")]
    BalanceOverflow,
    #[error("missing state root hash in response")]
    MissingStateRootHash,
    #[error("missing network name in chainspec")]
    MissingNetworkName,
    #[error("failed to load or parse secret key: {0}")]
    SecretKey(#[from] ErrorExt),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("transaction builder error: {0}")]
    TransactionBuild(#[from] TransactionV1BuilderError),
    #[error("blocking task error: {0}")]
    TaskJoin(#[from] tokio::task::JoinError),
}

impl From<CasperClientRpcError> for CasperClientError {
    fn from(value: CasperClientRpcError) -> Self {
        Self::Client(Box::new(value))
    }
}

/// Generates the next JSONRPC ID.
fn next_rpc_id() -> JsonRpcId {
    let value: i64 = rand::rng().random();
    JsonRpcId::from(value)
}

/// Parses the chainspec TOML from the RPC result.
fn parse_chainspec(result: &GetChainspecResult) -> Result<TomlValue, CasperClientError> {
    toml::de::from_slice(result.chainspec_bytes.chainspec_bytes()).map_err(Into::into)
}

/// Normalizes a node address by removing trailing slashes and `/rpc` suffixes.
fn normalize_node_address(endpoint: &str) -> Option<String> {
    let trimmed = endpoint.trim();
    if trimmed.is_empty() {
        return None;
    }

    let without_trailing_slash = trimmed.trim_end_matches('/');
    let cleaned = if let Some(stripped) = without_trailing_slash.strip_suffix("/rpc") {
        stripped.trim_end_matches('/').to_owned()
    } else {
        without_trailing_slash.to_owned()
    };

    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

/// Determines if the provided error code and message indicate a missing account.
///
/// Kind of hacky, but may be improved in the future with better error codes from the node.
fn is_missing_account_error(code: i64, message: &str) -> bool {
    const ACCOUNT_NOT_FOUND_CODE: i64 = -32075;

    if code == ACCOUNT_NOT_FOUND_CODE {
        return true;
    }

    let message = message.to_ascii_lowercase();
    message.contains("failed to get account")
        || message.contains("account not found")
        || message.contains("no such account")
        || message.contains("does not exist")
        || message.contains("missing")
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_next_rpc_id_increments() {
        let id1 = next_rpc_id();
        let id2 = next_rpc_id();
        let id3 = next_rpc_id();

        // IDs should be unique
        assert_ne!(id1, id2);
        assert_ne!(id2, id3);
    }

    #[test]
    fn test_normalize_node_address_empty() {
        assert_eq!(normalize_node_address(""), None);
        assert_eq!(normalize_node_address("   "), None);
    }

    #[test]
    fn test_normalize_node_address_trailing_slash() {
        assert_eq!(
            normalize_node_address("http://localhost:11101/"),
            Some("http://localhost:11101".to_string())
        );
    }

    #[test]
    fn test_normalize_node_address_rpc_suffix() {
        assert_eq!(
            normalize_node_address("http://localhost:11101/rpc"),
            Some("http://localhost:11101".to_string())
        );
        assert_eq!(
            normalize_node_address("http://localhost:11101/rpc/"),
            Some("http://localhost:11101".to_string())
        );
    }

    #[test]
    fn test_normalize_node_address_clean() {
        assert_eq!(
            normalize_node_address("http://localhost:11101"),
            Some("http://localhost:11101".to_string())
        );
    }

    #[test]
    fn test_normalize_node_address_only_rpc() {
        assert_eq!(normalize_node_address("/rpc"), None);
        assert_eq!(normalize_node_address("rpc"), Some("rpc".into()));
    }

    #[test]
    fn test_is_missing_account_error_code() {
        assert!(is_missing_account_error(-32075, ""));
        assert!(!is_missing_account_error(-32076, ""));
    }

    #[test]
    fn test_is_missing_account_error_message() {
        assert!(is_missing_account_error(0, "Failed to get account"));
        assert!(is_missing_account_error(0, "Account not found"));
        assert!(is_missing_account_error(0, "No such account"));
        assert!(is_missing_account_error(0, "does not exist"));
        assert!(is_missing_account_error(0, "missing account"));
        assert!(is_missing_account_error(0, "ACCOUNT NOT FOUND"));
        assert!(!is_missing_account_error(0, "other error"));
    }

    #[test]
    fn test_casper_client_new_empty_endpoints() {
        let result = CasperClient::new("testnet", Vec::<String>::new());
        assert!(matches!(
            result,
            Err(CasperClientError::MissingRpcEndpoints)
        ));
    }

    #[test]
    fn test_casper_client_new_filters_empty() {
        let result = CasperClient::new("testnet", vec!["", "  ", "/rpc"]);
        assert!(matches!(
            result,
            Err(CasperClientError::MissingRpcEndpoints)
        ));
    }

    #[test]
    fn test_casper_client_new_success() {
        let client = CasperClient::new("testnet", vec!["http://localhost:11101"])
            .expect("should create client");
        assert_eq!(client.network_name(), "testnet");
        assert_eq!(client.rpc_endpoint(), "http://localhost:11101");
    }

    #[test]
    fn test_casper_client_new_normalizes_endpoints() {
        let client = CasperClient::new(
            "testnet",
            vec!["http://localhost:11101/rpc/", "http://other:8888/", ""],
        )
        .expect("should create client");
        assert_eq!(client.rpc_endpoint(), "http://localhost:11101");
    }

    #[test]
    fn test_casper_client_error_display() {
        let error = CasperClientError::MissingRpcEndpoints;
        assert_eq!(error.to_string(), "no RPC endpoints configured");

        let error = CasperClientError::BalanceOverflow;
        assert_eq!(error.to_string(), "balance value exceeds u64 range");

        let error = CasperClientError::MissingStateRootHash;
        assert_eq!(error.to_string(), "missing state root hash in response");

        let error = CasperClientError::MissingNetworkName;
        assert_eq!(error.to_string(), "missing network name in chainspec");
    }
}
