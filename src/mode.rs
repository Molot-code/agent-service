// src/mode.rs
use std::sync::Arc;

use crate::db::AppDb;

#[derive(Debug, Clone, PartialEq)]
pub enum AgentMode {
    Local,
    Cloud,
    Hybrid,
}

impl AgentMode {
    pub fn from_str(s: &str) -> Self {
        match s {
            "cloud" => AgentMode::Cloud,
            "hybrid" => AgentMode::Hybrid,
            _ => AgentMode::Local,
        }
    }

    pub fn needs_local_api(&self) -> bool {
        matches!(self, AgentMode::Local | AgentMode::Hybrid)
    }

    pub fn needs_cloud_client(&self) -> bool {
        matches!(self, AgentMode::Cloud | AgentMode::Hybrid)
    }
}

pub fn get_current_mode(db: &Arc<AppDb>) -> AgentMode {
    let mode_str = db
        .get_state("agent_mode")
        .ok()
        .flatten()
        .unwrap_or_else(|| "hybrid".to_string());
    AgentMode::from_str(&mode_str)
}
