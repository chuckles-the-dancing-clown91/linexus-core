//! # Permission Funnel — enforces Dignifundus physics.

use uuid::Uuid;
use crate::errors::LinexusError;
use crate::identity::{LifecyclePhase, NodeClass, NodeIdentity};

/// Every possible action a Node can attempt within Linexus.
#[derive(Debug, Clone)]
pub enum SystemIntent {
    ConsumeDignityFloor,
    GeneratePassive(u64),
    ClaimTask(Uuid),
    ProposePolicy(String),
    VoteOnPolicy(Uuid),
    CommissionNewNode(NodeClass),
    ApproveFiatExpenditure(Uuid),
}

/// The Permission Funnel — the absolute gateway.
pub struct PermissionFunnel;

impl PermissionFunnel {
    pub fn evaluate_intent(actor: &NodeIdentity, intent: &SystemIntent) -> Result<(), LinexusError> {
        match (&actor.class, intent) {
            // COMMISSIONING OF LIFE
            (NodeClass::Human { capabilities, .. }, SystemIntent::CommissionNewNode(NodeClass::Human { .. })) => {
                if capabilities.contains("medical_doctor") { Ok(()) }
                else { Err(LinexusError::AuthViolation("Only medical nodes can verify the Breath Premise".into())) }
            }
            (NodeClass::Council { .. }, SystemIntent::CommissionNewNode(_)) => Ok(()),

            // FIAT EXPENDITURE
            (NodeClass::Council { .. }, SystemIntent::ApproveFiatExpenditure(_)) => Ok(()),
            (_, SystemIntent::ApproveFiatExpenditure(_)) => Err(LinexusError::AuthViolation("Only Councils can approve fiat".into())),

            // HARDWARE PHYSICS
            (NodeClass::InfraGenerator { .. }, SystemIntent::GeneratePassive(_)) => Ok(()),
            (NodeClass::InfraGenerator { .. }, _) => Err(LinexusError::AuthViolation("Generators can only generate".into())),
            (NodeClass::InfraConsumer { .. }, SystemIntent::ConsumeDignityFloor) => Ok(()),
            (NodeClass::InfraConsumer { .. }, _) => Err(LinexusError::AuthViolation("Consumers can only consume".into())),

            // HUMAN LIFECYCLE
            (NodeClass::Human { phase, .. }, SystemIntent::ClaimTask(_)) => {
                if *phase == LifecyclePhase::Childhood { Err(LinexusError::AuthViolation("Children exempt from labor".into())) } else { Ok(()) }
            }
            (NodeClass::Human { phase, .. }, SystemIntent::VoteOnPolicy(_)) | (NodeClass::Human { phase, .. }, SystemIntent::ProposePolicy(_)) => {
                if *phase == LifecyclePhase::Childhood { Err(LinexusError::AuthViolation("Voting unlocks at Learning phase".into())) } else { Ok(()) }
            }
            (NodeClass::Human { .. }, SystemIntent::ConsumeDignityFloor) => Ok(()),
            (NodeClass::Human { .. }, SystemIntent::GeneratePassive(_)) => Err(LinexusError::AuthViolation("Humans generate via Task Fulfillment".into())),

            // COUNCIL
            (NodeClass::Council { .. }, SystemIntent::ProposePolicy(_)) => Ok(()),
            (NodeClass::Council { .. }, SystemIntent::VoteOnPolicy(_)) => Err(LinexusError::AuthViolation("Individual votes go through Human nodes".into())),
            (NodeClass::Council { .. }, _) => Err(LinexusError::AuthViolation("Councils cannot consume or labor".into())),

            _ => Err(LinexusError::AuthViolation("Intent violates Dignifundus constraints".into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use crate::identity::{GenerationType, InfraProduct};

    fn make_human(phase: LifecyclePhase, caps: &[&str]) -> NodeIdentity {
        NodeIdentity { node_id: Uuid::new_v4(), public_key: "k".into(), class: NodeClass::Human { phase, capabilities: caps.iter().map(|s| s.to_string()).collect::<HashSet<_>>(), assigned_homestead: None }, label: "T".into(), commissioned_at: 0 }
    }

    #[test] fn solar_can_generate() {
        let s = NodeIdentity { node_id: Uuid::new_v4(), public_key: "k".into(), class: NodeClass::InfraGenerator { product: InfraProduct::SolarArray { peak_kw: 32, efficiency_pct: 0.95 }, generation_type: GenerationType::Solar, location_id: Uuid::new_v4() }, label: "S".into(), commissioned_at: 0 };
        assert!(PermissionFunnel::evaluate_intent(&s, &SystemIntent::GeneratePassive(100)).is_ok());
    }
    #[test] fn solar_cannot_vote() {
        let s = NodeIdentity { node_id: Uuid::new_v4(), public_key: "k".into(), class: NodeClass::InfraGenerator { product: InfraProduct::SolarArray { peak_kw: 32, efficiency_pct: 0.95 }, generation_type: GenerationType::Solar, location_id: Uuid::new_v4() }, label: "S".into(), commissioned_at: 0 };
        assert!(PermissionFunnel::evaluate_intent(&s, &SystemIntent::VoteOnPolicy(Uuid::new_v4())).is_err());
    }
    #[test] fn child_cannot_claim() { assert!(PermissionFunnel::evaluate_intent(&make_human(LifecyclePhase::Childhood, &[]), &SystemIntent::ClaimTask(Uuid::new_v4())).is_err()); }
    #[test] fn child_can_consume() { assert!(PermissionFunnel::evaluate_intent(&make_human(LifecyclePhase::Childhood, &[]), &SystemIntent::ConsumeDignityFloor).is_ok()); }
    #[test] fn worker_can_claim() { assert!(PermissionFunnel::evaluate_intent(&make_human(LifecyclePhase::Labor, &[]), &SystemIntent::ClaimTask(Uuid::new_v4())).is_ok()); }
    #[test] fn doctor_can_commission() {
        let nc = NodeClass::Human { phase: LifecyclePhase::Childhood, capabilities: HashSet::new(), assigned_homestead: None };
        assert!(PermissionFunnel::evaluate_intent(&make_human(LifecyclePhase::Labor, &["medical_doctor"]), &SystemIntent::CommissionNewNode(nc)).is_ok());
    }
    #[test] fn non_doctor_cannot_commission() {
        let nc = NodeClass::Human { phase: LifecyclePhase::Childhood, capabilities: HashSet::new(), assigned_homestead: None };
        assert!(PermissionFunnel::evaluate_intent(&make_human(LifecyclePhase::Labor, &["plumbing"]), &SystemIntent::CommissionNewNode(nc)).is_err());
    }
    #[test] fn council_approves_fiat() {
        let c = NodeIdentity { node_id: Uuid::new_v4(), public_key: "k".into(), class: NodeClass::Council { district_id: Uuid::new_v4(), threshold_pct: 0.66 }, label: "C".into(), commissioned_at: 0 };
        assert!(PermissionFunnel::evaluate_intent(&c, &SystemIntent::ApproveFiatExpenditure(Uuid::new_v4())).is_ok());
    }
    #[test] fn human_cannot_approve_fiat() { assert!(PermissionFunnel::evaluate_intent(&make_human(LifecyclePhase::Labor, &[]), &SystemIntent::ApproveFiatExpenditure(Uuid::new_v4())).is_err()); }
}
