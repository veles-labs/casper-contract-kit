use casper_types::{
    Block, BlockHash, EraId, FinalitySignature, InitiatorAddr, ProtocolVersion, PublicKey,
    TimeDiff, Timestamp, Transaction, TransactionHash, contract_messages::Messages,
    execution::ExecutionResult,
};
use serde::{Deserialize, Serialize};

/// Represents an event received from the Casper SSE (Server-Sent Events) stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SseEvent {
    ApiVersion(ProtocolVersion),
    DeployAccepted(serde_json::Value),
    BlockAdded {
        block_hash: BlockHash,
        block: Block,
    },
    DeployProcessed(serde_json::Value),
    DeployExpired(serde_json::Value),
    TransactionAccepted(Transaction),
    TransactionProcessed {
        transaction_hash: TransactionHash,
        initiator_addr: InitiatorAddr,
        timestamp: Timestamp,
        ttl: TimeDiff,
        block_hash: BlockHash,
        execution_result: ExecutionResult,
        messages: Messages,
    },
    TransactionExpired {
        transaction_hash: TransactionHash,
    },
    Fault {
        era_id: EraId,
        public_key: Box<PublicKey>,
        timestamp: Timestamp,
    },
    Step {
        era_id: EraId,
        // This technically is not amorphic, but this field is potentially > 30MB of size. By not
        // parsing it we make the process of intaking these messages much quicker and less memory
        // consuming.
        execution_effects: Box<serde_json::value::RawValue>,
    },
    Shutdown,
    FinalitySignature(FinalitySignature),
}
