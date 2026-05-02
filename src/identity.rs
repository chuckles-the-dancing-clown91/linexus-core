//! # Identity Module
//!
//! Defines the Universal Node — the cryptographic identity of every entity
//! in the Dignifundus.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use uuid::Uuid;

/// The strict timeline of the Generative Federation.
/// Each phase defines the obligations and capabilities of a Human Node.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LifecyclePhase {
    /// 0-18 years. Generates 20hr/week Demand for Supranjus (teaching/care).
    Childhood,
    /// 5 years. 20 hours/week of focused Supranjus generation.
    Learning,
    /// 20 years. 20 hours/week of Pranjurity (physical/system maintenance).
    Labor,
    /// Re-introduction track. 20 hours learning + 20 hours labor.
    Rehabilitation,
    /// 40+ years. 0 hours required. Full draw from Dignity Floor.
    Retirement,
}

impl LifecyclePhase {
    /// Returns (labor_hours, learning_hours) weekly obligation.
    pub fn weekly_obligations(&self) -> (u8, u8) {
        match self {
            Self::Childhood => (0, 0),
            Self::Learning => (0, 20),
            Self::Labor => (20, 0),
            Self::Rehabilitation => (20, 20),
            Self::Retirement => (0, 0),
        }
    }

    /// Whether this phase can claim labor tasks.
    pub fn can_claim_labor(&self) -> bool {
        matches!(self, Self::Labor | Self::Rehabilitation | Self::Retirement)
    }

    /// Whether this phase can claim education/supranjus tasks.
    pub fn can_claim_education(&self) -> bool {
        matches!(self, Self::Learning | Self::Rehabilitation | Self::Retirement)
    }

    /// Whether this phase grants voting rights.
    pub fn can_vote(&self) -> bool {
        !matches!(self, Self::Childhood)
    }
}

/// Type of energy an infrastructure generator produces.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GenerationType {
    Solar,
    Biodigester,
    Wind,
    WaterExtraction,
}

/// Type of resource an infrastructure consumer draws.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ConsumptionType {
    ClimateControl,
    WaterPump,
    NetworkEquipment,
    Lighting,
}

/// The physical assets of the Dignifundus.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum InfraProduct {
    SolarArray { peak_kw: u32, efficiency_pct: f32 },
    Biodigester { daily_capacity_m3: u32 },
    WaterPump { gallons_per_minute: u32 },
    Hvac { btu_capacity: u32 },
    AgriculturalUnit { crop_type: String, expected_yield_kg: u32 },
    NetworkNode { bandwidth_mbps: u32 },
    ResidentialUnit { max_occupancy: u8 },
}

/// The Universal Cryptographic Node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum NodeClass {
    /// The "I". The sovereign human.
    Human {
        phase: LifecyclePhase,
        capabilities: HashSet<String>,
        assigned_homestead: Option<Uuid>,
    },
    /// Infrastructure that produces (Solar, Biodigester).
    InfraGenerator {
        product: InfraProduct,
        generation_type: GenerationType,
        location_id: Uuid,
    },
    /// Infrastructure that consumes (HVAC, Pumps).
    InfraConsumer {
        product: InfraProduct,
        consumption_type: ConsumptionType,
        location_id: Uuid,
    },
    /// Multi-signature voting aggregate for a district.
    Council {
        district_id: Uuid,
        threshold_pct: f32,
    },
}

/// Cryptographic identity of a Node in the Linexus system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeIdentity {
    pub node_id: Uuid,
    pub public_key: String,
    pub class: NodeClass,
    pub label: String,
    pub commissioned_at: u64,
}

impl NodeIdentity {
    pub fn is_human(&self) -> bool { matches!(self.class, NodeClass::Human { .. }) }
    pub fn is_generator(&self) -> bool { matches!(self.class, NodeClass::InfraGenerator { .. }) }
    pub fn is_consumer(&self) -> bool { matches!(self.class, NodeClass::InfraConsumer { .. }) }
    pub fn is_council(&self) -> bool { matches!(self.class, NodeClass::Council { .. }) }

    pub fn lifecycle_phase(&self) -> Option<&LifecyclePhase> {
        if let NodeClass::Human { ref phase, .. } = self.class { Some(phase) } else { None }
    }

    pub fn capabilities(&self) -> Option<&HashSet<String>> {
        if let NodeClass::Human { ref capabilities, .. } = self.class { Some(capabilities) } else { None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn childhood_has_zero_obligations() {
        let (l, e) = LifecyclePhase::Childhood.weekly_obligations();
        assert_eq!((l, e), (0, 0));
    }

    #[test]
    fn learning_phase_requires_20hrs_education() {
        let (l, e) = LifecyclePhase::Learning.weekly_obligations();
        assert_eq!((l, e), (0, 20));
    }

    #[test]
    fn labor_phase_requires_20hrs_pranjurity() {
        let (l, e) = LifecyclePhase::Labor.weekly_obligations();
        assert_eq!((l, e), (20, 0));
    }

    #[test]
    fn rehabilitation_requires_both_tracks() {
        let (l, e) = LifecyclePhase::Rehabilitation.weekly_obligations();
        assert_eq!((l, e), (20, 20));
    }

    #[test]
    fn retirement_has_zero_obligations() {
        let (l, e) = LifecyclePhase::Retirement.weekly_obligations();
        assert_eq!((l, e), (0, 0));
    }

    #[test]
    fn children_cannot_vote() { assert!(!LifecyclePhase::Childhood.can_vote()); }

    #[test]
    fn all_non_children_can_vote() {
        assert!(LifecyclePhase::Learning.can_vote());
        assert!(LifecyclePhase::Labor.can_vote());
        assert!(LifecyclePhase::Rehabilitation.can_vote());
        assert!(LifecyclePhase::Retirement.can_vote());
    }

    #[test]
    fn children_cannot_claim_labor() {
        assert!(!LifecyclePhase::Childhood.can_claim_labor());
        assert!(!LifecyclePhase::Learning.can_claim_labor());
    }

    #[test]
    fn node_identity_classification() {
        let human = NodeIdentity {
            node_id: Uuid::new_v4(),
            public_key: "ed25519_test_key".to_string(),
            class: NodeClass::Human {
                phase: LifecyclePhase::Labor,
                capabilities: HashSet::from(["plumbing_lvl1".to_string()]),
                assigned_homestead: None,
            },
            label: "Test Worker".to_string(),
            commissioned_at: 0,
        };
        assert!(human.is_human());
        assert!(!human.is_generator());
        assert_eq!(human.lifecycle_phase(), Some(&LifecyclePhase::Labor));
    }
}
