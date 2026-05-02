//! # Linexus Core
//!
//! The foundational domain types for the Linexus ecosystem — the digital nervous system
//! of the Generative Federation.
//!
//! This crate encodes the *Architecture of Worth* into Rust's type system. The compiler
//! itself enforces the Breath Premise: human worth is unconditional and begins at the
//! first breath. Invalid social states are unrepresentable. A solar panel cannot vote.
//! A child cannot be assigned high-voltage labor. Demiurge cannot be hoarded beyond
//! a 5-year epoch.
//!
//! ## Modules
//!
//! - [`identity`] — The "I", the 5/20/40 Lifecycle, NodeClass, InfraProduct
//! - [`demiurge`] — The immutable ledger, Pranjurity/Supranjus minting, decay mechanics
//! - [`engine`] — The Orchestrator backbone: existence creates Demand, the engine routes Supply
//! - [`funnel`] — The Permission Funnel: the impenetrable middleware enforcing Dignifundus physics
//! - [`jobs`] — KPI-driven maintenance jobs, 2-year epoch rebalancing
//! - [`errors`] — Unified error types

pub mod identity;
pub mod demiurge;
pub mod engine;
pub mod funnel;
pub mod jobs;
pub mod errors;
