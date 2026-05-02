//! # Demiurge Module — The accounting of human energy.
//! Demiurge decays after a 5-year Epoch. The ledger is append-only.

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::errors::LinexusError;

/// 5-year decay period in seconds.
pub const EPOCH_DECAY_SECS: u64 = 5 * 365 * 24 * 60 * 60;

/// Classes of value driving the Linexus engine.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ValueClass {
    Pranjurity(u64),
    Supranjus(u64),
    PassiveEcological(u64),
}

impl ValueClass {
    pub fn amount(&self) -> u64 {
        match self { Self::Pranjurity(a) | Self::Supranjus(a) | Self::PassiveEcological(a) => *a }
    }
}

/// Immutable entry in the Demiurge Segmented Log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerEntry {
    pub transaction_id: Uuid,
    pub timestamp: u64,
    pub node_id: Uuid,
    pub value: ValueClass,
    pub source_demand_id: Option<Uuid>,
    pub task_id: Option<Uuid>,
    pub expires_at: u64,
    pub authority_signature: String,
}

impl LedgerEntry {
    pub fn mint(node_id: Uuid, demand_id: Option<Uuid>, task_id: Option<Uuid>, value: ValueClass, current_time: u64, authority_sig: String) -> Self {
        Self { transaction_id: Uuid::new_v4(), timestamp: current_time, node_id, value, source_demand_id: demand_id, task_id, expires_at: current_time + EPOCH_DECAY_SECS, authority_signature: authority_sig }
    }

    pub fn is_decayed(&self, current_time: u64) -> bool { current_time > self.expires_at }
    pub fn remaining_ttl(&self, current_time: u64) -> u64 { self.expires_at.saturating_sub(current_time) }
}

/// Aggregated view of a Node's active Demiurge balance.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DemiurgeBalance {
    pub pranjurity: u64,
    pub supranjus: u64,
    pub passive_ecological: u64,
}

impl DemiurgeBalance {
    pub fn total(&self) -> u64 { self.pranjurity + self.supranjus + self.passive_ecological }

    pub fn apply_entry(&mut self, entry: &LedgerEntry, current_time: u64) -> Result<(), LinexusError> {
        if entry.is_decayed(current_time) {
            return Err(LinexusError::LedgerError("Cannot apply decayed entry".to_string()));
        }
        match &entry.value {
            ValueClass::Pranjurity(a) => self.pranjurity += a,
            ValueClass::Supranjus(a) => self.supranjus += a,
            ValueClass::PassiveEcological(a) => self.passive_ecological += a,
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const T: u64 = 1_700_000_000;

    #[test]
    fn mint_sets_correct_expiry() {
        let e = LedgerEntry::mint(Uuid::new_v4(), None, None, ValueClass::Pranjurity(100), T, "s".into());
        assert_eq!(e.expires_at, T + EPOCH_DECAY_SECS);
        assert!(!e.is_decayed(T));
        assert!(e.is_decayed(T + EPOCH_DECAY_SECS + 1));
    }

    #[test]
    fn remaining_ttl_counts_down() {
        let e = LedgerEntry::mint(Uuid::new_v4(), None, None, ValueClass::Supranjus(50), T, "s".into());
        assert_eq!(e.remaining_ttl(T), EPOCH_DECAY_SECS);
        assert_eq!(e.remaining_ttl(T + EPOCH_DECAY_SECS + 9999), 0);
    }

    #[test]
    fn balance_applies_active() {
        let mut b = DemiurgeBalance::default();
        let e = LedgerEntry::mint(Uuid::new_v4(), None, None, ValueClass::Pranjurity(100), T, "s".into());
        b.apply_entry(&e, T).unwrap();
        assert_eq!(b.total(), 100);
    }

    #[test]
    fn balance_rejects_decayed() {
        let mut b = DemiurgeBalance::default();
        let e = LedgerEntry::mint(Uuid::new_v4(), None, None, ValueClass::Pranjurity(100), T, "s".into());
        assert!(b.apply_entry(&e, T + EPOCH_DECAY_SECS + 1).is_err());
    }

    #[test]
    fn value_class_amount() {
        assert_eq!(ValueClass::Pranjurity(42).amount(), 42);
        assert_eq!(ValueClass::PassiveEcological(7).amount(), 7);
    }
}
