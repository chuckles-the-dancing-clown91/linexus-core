//! # Engine Module — Existence creates Demand; the Engine routes Supply.

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::errors::LinexusError;
use crate::identity::{LifecyclePhase, NodeClass, NodeIdentity};

/// A Demand Event represents a need in the Dignifundus.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DemandEvent {
    Education { child_id: Uuid, weekly_hours: u8, subject: String, demiurge_reward: u64 },
    PhysicalMaintenance { infra_id: Uuid, weekly_hours: u8, capability_required: String, demiurge_reward: u64 },
    RehabilitationSupport { rehab_node_id: Uuid, weekly_hours: u8, capability_required: String, demiurge_reward: u64 },
    MaterialShortage { item_id: Uuid, fiat_cost_estimate: f64, justification: String },
}

impl DemandEvent {
    pub fn source_node_id(&self) -> Option<Uuid> {
        match self {
            Self::Education { child_id, .. } => Some(*child_id),
            Self::PhysicalMaintenance { infra_id, .. } => Some(*infra_id),
            Self::RehabilitationSupport { rehab_node_id, .. } => Some(*rehab_node_id),
            Self::MaterialShortage { .. } => None,
        }
    }
    pub fn required_capability(&self) -> Option<&str> {
        match self {
            Self::Education { subject, .. } => Some(subject),
            Self::PhysicalMaintenance { capability_required, .. } => Some(capability_required),
            Self::RehabilitationSupport { capability_required, .. } => Some(capability_required),
            Self::MaterialShortage { .. } => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskAssignmentStatus {
    Unassigned,
    Offered { candidate_id: Uuid },
    Claimed { worker_id: Uuid },
    VerificationPending { worker_id: Uuid },
    Completed { worker_id: Uuid },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskAssignment {
    pub task_id: Uuid,
    pub demand_event: DemandEvent,
    pub status: TaskAssignmentStatus,
    pub created_at: u64,
}

/// The Orchestrator Engine — matches demands to available Human Nodes.
pub struct OrchestratorEngine;

impl OrchestratorEngine {
    pub fn get_node_obligations(node: &NodeIdentity) -> (u8, u8) {
        if let NodeClass::Human { ref phase, .. } = node.class { phase.weekly_obligations() } else { (0, 0) }
    }

    pub fn route_demand(demand: &DemandEvent, available_nodes: &[NodeIdentity]) -> Result<Option<Uuid>, LinexusError> {
        match demand {
            DemandEvent::MaterialShortage { .. } => Ok(None),
            DemandEvent::Education { subject, .. } => Ok(Self::find_capable_node(available_nodes, subject, true)),
            DemandEvent::PhysicalMaintenance { capability_required, .. } => Ok(Self::find_capable_node(available_nodes, capability_required, false)),
            DemandEvent::RehabilitationSupport { capability_required, .. } => Ok(Self::find_capable_node(available_nodes, capability_required, true)),
        }
    }

    fn find_capable_node(nodes: &[NodeIdentity], required_capability: &str, prefer_supranjus: bool) -> Option<Uuid> {
        for node in nodes {
            if let NodeClass::Human { ref capabilities, ref phase, .. } = node.class
                && capabilities.contains(required_capability)
            {
                let (labor_req, supra_req) = phase.weekly_obligations();
                if prefer_supranjus && supra_req > 0 { return Some(node.node_id); }
                if !prefer_supranjus && labor_req > 0 { return Some(node.node_id); }
                if *phase == LifecyclePhase::Retirement { return Some(node.node_id); }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn make_human(phase: LifecyclePhase, caps: &[&str]) -> NodeIdentity {
        NodeIdentity { node_id: Uuid::new_v4(), public_key: "k".into(), class: NodeClass::Human { phase, capabilities: caps.iter().map(|s| s.to_string()).collect::<HashSet<_>>(), assigned_homestead: None }, label: "T".into(), commissioned_at: 0 }
    }

    #[test]
    fn material_shortage_never_routes() {
        let d = DemandEvent::MaterialShortage { item_id: Uuid::new_v4(), fiat_cost_estimate: 500.0, justification: "x".into() };
        assert!(OrchestratorEngine::route_demand(&d, &[make_human(LifecyclePhase::Labor, &["x"])]).unwrap().is_none());
    }

    #[test]
    fn education_routes_to_teacher() {
        let d = DemandEvent::Education { child_id: Uuid::new_v4(), weekly_hours: 20, subject: "math".into(), demiurge_reward: 200 };
        let t = make_human(LifecyclePhase::Learning, &["math"]);
        assert_eq!(OrchestratorEngine::route_demand(&d, &[t.clone()]).unwrap(), Some(t.node_id));
    }

    #[test]
    fn maintenance_routes_to_labor() {
        let d = DemandEvent::PhysicalMaintenance { infra_id: Uuid::new_v4(), weekly_hours: 4, capability_required: "solar".into(), demiurge_reward: 40 };
        let w = make_human(LifecyclePhase::Labor, &["solar"]);
        assert_eq!(OrchestratorEngine::route_demand(&d, &[w.clone()]).unwrap(), Some(w.node_id));
    }

    #[test]
    fn children_never_routed() {
        let d = DemandEvent::PhysicalMaintenance { infra_id: Uuid::new_v4(), weekly_hours: 4, capability_required: "x".into(), demiurge_reward: 20 };
        assert!(OrchestratorEngine::route_demand(&d, &[make_human(LifecyclePhase::Childhood, &["x"])]).unwrap().is_none());
    }
}
