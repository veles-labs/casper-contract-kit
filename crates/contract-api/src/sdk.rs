use std::{
    convert::TryInto,
    fs,
    sync::atomic::{AtomicI64, Ordering},
};

use casper_client::{
    self, JsonRpcId, Verbosity,
    cli::TransactionV1BuilderError,
    keygen,
    rpcs::{
        AccountIdentifier,
        results::{
            GetAccountResult, GetChainspecResult, GetStateRootHashResult, GetTransactionResult,
            PutTransactionResult, SpeculativeExecTxnResult,
        },
    },
};
use casper_types::{AsymmetricType, TransactionHash};
use casper_types::{
    Digest, PricingMode, PublicKey, RuntimeArgs, Transaction, TransactionRuntimeParams,
    crypto::ErrorExt,
};
use secrecy::SecretBox;
use thiserror::Error;
use toml::Value as TomlValue;

static RPC_COUNTER: AtomicI64 = AtomicI64::new(1);

/// Shared RPC client wrapper for host-side tools (agent, xtask, etc).
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
        public_key_hex: &str,
    ) -> Result<Option<GetAccountResult>, CasperClientError> {
        let public_key = PublicKey::from_hex(public_key_hex).map_err(|source| {
            CasperClientError::InvalidPublicKey {
                input: public_key_hex.to_string(),
                source,
            }
        })?;

        match casper_client::get_account(
            next_rpc_id(),
            self.rpc_endpoint(),
            self.verbosity,
            None,
            AccountIdentifier::PublicKey(public_key),
        )
        .await
        {
            Ok(response) => Ok(Some(response.result)),
            Err(casper_client::Error::ResponseIsRpcError { error, .. })
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

        extract_state_root(&response.result)
    }

    /// Returns the latest state root hash as a lowercase hex string.
    pub async fn get_state_root_hash_hex(&self) -> Result<String, CasperClientError> {
        Ok(self.get_state_root_hash().await?.to_string())
    }

    /// Returns the balance (in motes) for the provided public key, if the account exists.
    pub async fn get_balance(
        &self,
        public_key_hex: &str,
    ) -> Result<Option<u64>, CasperClientError> {
        let account = match self.get_account(public_key_hex).await? {
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
        let value: u64 = balance
            .try_into()
            .map_err(|_| CasperClientError::BalanceOverflow)?;
        Ok(Some(value))
    }

    /// Submits a session WASM transaction and returns the transaction hash.
    pub async fn put_session_transaction(
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

    /// Submits a pre-built transaction and returns the transaction hash.
    pub async fn put_transaction(
        &self,
        transaction: Transaction,
    ) -> Result<PutTransactionResult, CasperClientError> {
        let response = casper_client::put_transaction(
            next_rpc_id(),
            self.rpc_endpoint(),
            self.verbosity,
            transaction,
        )
        .await?;
        Ok(response.result)
    }

    /// Downloads and parses the chainspec TOML as `toml::Value`.
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

    /// Generates a new Secp256k1 secret key and returns its PEM contents.
    pub fn keygen() -> Result<SecretBox<String>, CasperClientError> {
        let tempdir = tempfile::tempdir()?;
        let dir = tempdir.path().to_string_lossy();

        keygen::generate_files(&dir, keygen::SECP256K1, true)?;

        let secret_key_path = tempdir.path().join(keygen::SECRET_KEY_PEM);
        let secret = fs::read_to_string(&secret_key_path)?;

        Ok(SecretBox::new(Box::new(secret)))
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
    ) -> Result<casper_client::rpcs::results::GetBlockResult, CasperClientError> {
        let response =
            casper_client::get_block(next_rpc_id(), self.rpc_endpoint(), self.verbosity, None)
                .await?;
        Ok(response.result)
    }
}

/// Options for constructing session transactions.
#[derive(Debug, Clone)]
pub struct SessionOptions {
    pub pricing: SessionPricing,
    pub install_upgrade: bool,
    pub runtime: TransactionRuntimeParams,
    pub runtime_args: Option<RuntimeArgs>,
}

impl Default for SessionOptions {
    fn default() -> Self {
        Self {
            pricing: SessionPricing::default(),
            install_upgrade: true,
            runtime: TransactionRuntimeParams::VmCasperV1,
            runtime_args: None,
        }
    }
}

/// Pricing configuration for session transactions.
#[derive(Debug, Clone, Copy)]
pub struct SessionPricing {
    pub payment_amount: u64,
    pub gas_price_tolerance: u8,
    pub standard_payment: bool,
}

impl Default for SessionPricing {
    fn default() -> Self {
        Self {
            payment_amount: 750_000_000_000,
            gas_price_tolerance: 1,
            standard_payment: true,
        }
    }
}

impl From<SessionPricing> for PricingMode {
    fn from(value: SessionPricing) -> Self {
        PricingMode::PaymentLimited {
            payment_amount: value.payment_amount,
            gas_price_tolerance: value.gas_price_tolerance,
            standard_payment: value.standard_payment,
        }
    }
}

#[derive(Error, Debug)]
pub enum CasperClientError {
    #[error("no RPC endpoints configured")]
    MissingRpcEndpoints,
    #[error("casper client error: {0}")]
    Client(Box<casper_client::Error>),
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
    TransactionBuild(TransactionV1BuilderError),
    #[error("failed to parse public key `{input}`: {source}")]
    InvalidPublicKey {
        input: String,
        source: casper_types::crypto::Error,
    },
}

impl From<casper_client::Error> for CasperClientError {
    fn from(value: casper_client::Error) -> Self {
        Self::Client(Box::new(value))
    }
}
impl From<TransactionV1BuilderError> for CasperClientError {
    fn from(value: TransactionV1BuilderError) -> Self {
        Self::TransactionBuild(value)
    }
}

fn next_rpc_id() -> JsonRpcId {
    JsonRpcId::from(RPC_COUNTER.fetch_add(1, Ordering::Relaxed))
}

fn extract_state_root(result: &GetStateRootHashResult) -> Result<Digest, CasperClientError> {
    result
        .state_root_hash
        .ok_or(CasperClientError::MissingStateRootHash)
}

fn parse_chainspec(result: &GetChainspecResult) -> Result<TomlValue, CasperClientError> {
    toml::de::from_slice(result.chainspec_bytes.chainspec_bytes()).map_err(Into::into)
}

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
