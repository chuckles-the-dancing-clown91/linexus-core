//! # Error Module
//!
//! Unified error types for the Linexus ecosystem.
//!
//! Every error is explicitly handled — no `.unwrap()` or `.expect()` in
//! production logic. The error hierarchy maps directly to the architectural
//! constraints of the Dignifundus.

use thiserror::Error;
use uuid::Uuid;

/// The root error type for all Linexus operations.
///
/// Each variant corresponds to a distinct failure domain in the architecture.
/// The system is designed so that errors in one domain (e.g., the Demiurge ledger)
/// cannot cascade into another domain (e.g., physical infrastructure control).
#[derive(Debug, Error)]
pub enum LinexusError {
    // -----------------------------------------------------------------
    // Authorization & Permission Funnel
    // -----------------------------------------------------------------
    /// A Node attempted an action that violates the structural constraints
    /// of the Dignifundus. This is the Permission Funnel's rejection.
    #[error("Authorization violation: {0}")]
    AuthViolation(String),

    /// A Node's cryptographic signature could not be verified.
    #[error("Signature verification failed for node {node_id}: {reason}")]
    SignatureError {
        node_id: Uuid,
        reason: String,
    },

    // -----------------------------------------------------------------
    // Demiurge Ledger
    // -----------------------------------------------------------------
    /// An error in the immutable Demiurge ledger operations.
    #[error("Ledger error: {0}")]
    LedgerError(String),

    /// Attempted to mint Demiurge without a valid task completion proof.
    #[error("Minting rejected: no valid task completion for node {node_id}")]
    MintingRejected {
        node_id: Uuid,
    },

    // -----------------------------------------------------------------
    // Orchestrator & Routing
    // -----------------------------------------------------------------
    /// The Orchestrator could not find a qualified Node to fulfill a Demand.
    #[error("Routing failed for demand {demand_id}: {reason}")]
    RoutingError {
        demand_id: Uuid,
        reason: String,
    },

    /// A task could not be claimed due to phase or capability mismatch.
    #[error("Task claim rejected: {0}")]
    TaskClaimRejected(String),

    // -----------------------------------------------------------------
    // Inventory & Material Supply
    // -----------------------------------------------------------------
    /// The local spoke does not have sufficient materials to complete a job.
    #[error("Inventory shortage: item {item_id} requires {needed} but only {available} in stock")]
    InventoryShortage {
        item_id: Uuid,
        needed: u32,
        available: u32,
    },

    // -----------------------------------------------------------------
    // Governance
    // -----------------------------------------------------------------
    /// A governance proposal violated structural constraints.
    #[error("Governance error: {0}")]
    GovernanceError(String),

    /// A vote was submitted by an ineligible Node.
    #[error("Vote rejected: node {voter_id} is not eligible to vote on proposal {proposal_id}")]
    VoteRejected {
        voter_id: Uuid,
        proposal_id: Uuid,
    },

    // -----------------------------------------------------------------
    // Edge / Agent
    // -----------------------------------------------------------------
    /// An edge agent reported a communication failure.
    #[error("Agent communication error: {0}")]
    AgentCommError(String),

    /// A node's telemetry indicates critical drift from the desired state.
    #[error("Critical drift detected on node {node_id}: {metric} at {current_value:.2} (target: {target_value:.2})")]
    CriticalDrift {
        node_id: Uuid,
        metric: String,
        current_value: f32,
        target_value: f32,
    },

    // -----------------------------------------------------------------
    // General
    // -----------------------------------------------------------------
    /// A requested entity was not found in the system.
    #[error("Entity not found: {entity_type} with ID {entity_id}")]
    NotFound {
        entity_type: String,
        entity_id: Uuid,
    },

    /// An internal system error that should be logged and investigated.
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Alias for Results using LinexusError.
pub type LinexusResult<T> = Result<T, LinexusError>;
