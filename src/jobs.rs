//! # Jobs Module — KPI-driven maintenance and 2-year epoch rebalancing.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// 20-Year Plan divided into ten 2-Year Epochs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpochLifecycle {
    pub current_epoch_number: u8,
    pub years_per_epoch: u8,
    pub start_timestamp: u64,
}

/// Universal performance metric.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KpiMetric {
    pub metric_name: String,
    pub current_value: f32,
    pub target_value: f32,
    pub epoch_degradation_rate: f32,
}

impl KpiMetric {
    pub fn projected_end_of_epoch(&self) -> f32 { self.current_value - self.epoch_degradation_rate }
    pub fn will_breach_floor(&self) -> bool { self.projected_end_of_epoch() < self.target_value }
    pub fn breach_severity(&self) -> f32 {
        let p = self.projected_end_of_epoch();
        if p < self.target_value { self.target_value - p } else { 0.0 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterialAllocation {
    pub inventory_id: Uuid,
    pub item_name: String,
    pub quantity: u32,
    pub demiurge_material_cost: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum JobCategory {
    ITInfrastructure, MedicalAndRehab, Agricultural, MechanicalPlumbing, ElectricalSolar, Education, GeneralMaintenance,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaintenanceJob {
    pub job_id: Uuid,
    pub target_node_id: Uuid,
    pub category: JobCategory,
    pub required_capability: String,
    pub labor_hours_required: u8,
    pub allocated_materials: Vec<MaterialAllocation>,
    pub demiurge_labor_reward: u64,
    pub projected_kpi_restoration: HashMap<String, f32>,
    pub status: JobStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobStatus { Open, ClaimedBy(Uuid), InProgress(Uuid), VerificationPending(Uuid), Completed(Uuid), Cancelled(String) }

pub struct JobEngine;

impl JobEngine {
    pub fn evaluate_and_rebalance_node(node_id: Uuid, metrics: &HashMap<String, KpiMetric>) -> Vec<MaintenanceJob> {
        let mut jobs = Vec::new();
        for (name, kpi) in metrics {
            if !kpi.will_breach_floor() { continue; }
            let sev = kpi.breach_severity();
            let hrs = ((sev * 100.0) as u8).max(1);
            jobs.push(MaintenanceJob {
                job_id: Uuid::new_v4(), target_node_id: node_id, category: Self::map_metric_to_category(name),
                required_capability: Self::map_metric_to_capability(name), labor_hours_required: hrs,
                allocated_materials: vec![], demiurge_labor_reward: (hrs as u64) * 10,
                projected_kpi_restoration: HashMap::from([(name.clone(), sev)]), status: JobStatus::Open,
            });
        }
        jobs
    }

    pub fn map_metric_to_capability(m: &str) -> String {
        match m {
            "network_uptime" => "it_infrastructure_lvl2", "solar_efficiency" => "high_voltage_electrical",
            "mental_health_index" | "rehabilitation_status" => "psychiatric_care",
            "physical_health_index" => "medical_doctor", "water_pressure" => "plumbing_lvl1",
            "crop_yield" => "agricultural_specialist", _ => "general_maintenance",
        }.to_string()
    }

    pub fn map_metric_to_category(m: &str) -> JobCategory {
        match m {
            "network_uptime" => JobCategory::ITInfrastructure, "solar_efficiency" => JobCategory::ElectricalSolar,
            "mental_health_index" | "rehabilitation_status" | "physical_health_index" => JobCategory::MedicalAndRehab,
            "water_pressure" => JobCategory::MechanicalPlumbing, "crop_yield" => JobCategory::Agricultural,
            _ => JobCategory::GeneralMaintenance,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test] fn kpi_breach_detection() {
        let k = KpiMetric { metric_name: "solar_efficiency".into(), current_value: 0.85, target_value: 0.90, epoch_degradation_rate: 0.10 };
        assert!(k.will_breach_floor());
    }

    #[test] fn healthy_kpi_no_jobs() {
        let k = KpiMetric { metric_name: "network_uptime".into(), current_value: 0.99, target_value: 0.95, epoch_degradation_rate: 0.02 };
        let m = HashMap::from([("network_uptime".into(), k)]);
        assert!(JobEngine::evaluate_and_rebalance_node(Uuid::new_v4(), &m).is_empty());
    }

    #[test] fn degraded_generates_job() {
        let k = KpiMetric { metric_name: "mental_health_index".into(), current_value: 0.60, target_value: 0.80, epoch_degradation_rate: 0.15 };
        let m = HashMap::from([("mental_health_index".into(), k)]);
        let jobs = JobEngine::evaluate_and_rebalance_node(Uuid::new_v4(), &m);
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].required_capability, "psychiatric_care");
    }

    #[test] fn reward_scales_with_severity() {
        let k = KpiMetric { metric_name: "water_pressure".into(), current_value: 0.50, target_value: 0.90, epoch_degradation_rate: 0.20 };
        let m = HashMap::from([("water_pressure".into(), k)]);
        let jobs = JobEngine::evaluate_and_rebalance_node(Uuid::new_v4(), &m);
        assert!(jobs[0].demiurge_labor_reward > 0);
    }
}
