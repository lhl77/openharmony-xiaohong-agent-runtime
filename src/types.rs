use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

pub type RuntimeResult<T> = Result<T, RuntimeError>;

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid plan: {0}")]
    InvalidPlan(String),
    #[error("unknown tool: {0}")]
    UnknownTool(String),
    #[error("tool failed: {0}")]
    ToolFailed(String),
    #[error("permission denied for tool {tool}: {reason}")]
    PermissionDenied { tool: String, reason: String },
    #[error("dependency failed for node {0}")]
    DependencyFailed(String),
    #[error("review is required before side-effect node {0}")]
    ApprovalRequired(String),
    #[error("plan has a dependency cycle")]
    DependencyCycle,
    #[error("node limit exceeded (max {0})")]
    NodeLimit(usize),
    #[error("runtime lock poisoned")]
    PoisonedLock,
    #[error("null pointer passed across C ABI")]
    NullPointer,
}

impl RuntimeError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Json(_) => "INVALID_JSON",
            Self::InvalidPlan(_) => "INVALID_PLAN",
            Self::UnknownTool(_) => "UNKNOWN_TOOL",
            Self::ToolFailed(_) => "TOOL_FAILED",
            Self::PermissionDenied { .. } => "PERMISSION_DENIED",
            Self::DependencyFailed(_) => "DEPENDENCY_FAILED",
            Self::ApprovalRequired(_) => "APPROVAL_REQUIRED",
            Self::DependencyCycle => "DEPENDENCY_CYCLE",
            Self::NodeLimit(_) => "NODE_LIMIT",
            Self::PoisonedLock => "RUNTIME_LOCKED",
            Self::NullPointer => "NULL_INPUT",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentGoal {
    pub task_id: String,
    pub instruction: String,
    #[serde(default)]
    pub context_refs: Vec<String>,
}

/// AI planner output is a proposal. The runtime validates it before execution.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TaskPlan {
    pub task_id: String,
    pub nodes: Vec<TaskNode>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TaskNode {
    pub node_id: String,
    /// Registered Tool ID, not free-form executable text.
    pub tool_id: String,
    pub input: serde_json::Value,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub requires_approval: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ToolDescriptor {
    pub tool_id: String,
    pub description: String,
    pub side_effect: SideEffect,
    #[serde(default)]
    pub required_permissions: Vec<String>,
    #[serde(default)]
    pub timeout_ms: u64,
    #[serde(default)]
    pub idempotent: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SideEffect { ReadOnly, LocalWrite, ExternalWrite }

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionPolicy {
    /// Permissions granted by the application/user for this task.
    #[serde(default)]
    pub granted_permissions: Vec<String>,
    /// Only a user action may add IDs here; an AI plan cannot approve itself.
    #[serde(default)]
    pub approved_nodes: Vec<String>,
    /// External writes remain disabled unless explicitly enabled by the app.
    #[serde(default)]
    pub allow_external_write: bool,
    #[serde(default = "default_node_limit")]
    pub max_nodes: usize,
}
fn default_node_limit() -> usize { 64 }
impl Default for ExecutionPolicy {
    fn default() -> Self {
        Self { granted_permissions: Vec::new(), approved_nodes: Vec::new(),
            allow_external_write: false, max_nodes: default_node_limit() }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum NodeStatus { Pending, Ready, Running, Completed, Failed, Blocked, AwaitingApproval }

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NodeResult {
    pub output: serde_json::Value,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TaskEvent {
    pub task_id: String,
    pub node_id: String,
    pub status: NodeStatus,
    #[serde(default)]
    pub message: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TaskSnapshot {
    pub schema_version: u16,
    pub task_id: String,
    pub statuses: BTreeMap<String, NodeStatus>,
    pub results: BTreeMap<String, NodeResult>,
    #[serde(default)]
    pub pending_events: Vec<TaskEvent>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UserApproval {
    pub task_id: String,
    pub node_id: String,
    /// Bind approval to the exact serialized tool input shown to the user.
    pub input_sha256: String,
}
