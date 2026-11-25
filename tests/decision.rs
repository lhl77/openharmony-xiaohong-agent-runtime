use std::sync::Arc;
use xiaohong_agent_runtime::*;

struct LocalOnlyPlanner;
impl Planner for LocalOnlyPlanner {
    fn propose(&self, _goal: &AgentGoal, _tools: &[ToolDescriptor]) -> RuntimeResult<TaskPlan> {
        unreachable!("not used in decision tests")
    }
}

/// A "cloud AI" advisor that always wants cloud, to prove the runtime gate wins.
struct CloudBiasedAdvisor;
impl DecisionAdvisor for CloudBiasedAdvisor {
    fn name(&self) -> &str { "cloud-llm" }

    fn advise(&self, _context: &DecisionContext) -> RuntimeResult<DecisionProposal> {
        Ok(DecisionProposal {
            backend: ModelBackend::Cloud,
            rationale: "cloud model has the best quality".into(),
            confidence: 0.95,
        })
    }
}

fn runtime(advisor: Option<Arc<dyn DecisionAdvisor>>) -> AgentRuntime {
    let planner: Arc<dyn Planner> = Arc::new(LocalOnlyPlanner);
    let registry = Registry::default();
    match advisor {
        Some(advisor) => AgentRuntime::with_advisor(planner, advisor, registry, ExecutionPolicy::default()),
        None => AgentRuntime::new(planner, registry, ExecutionPolicy::default()),
    }
}

fn candidates() -> Vec<BackendOption> {
    vec![
        BackendOption {
            backend: ModelBackend::Local,
            available: true,
            capabilities: vec!["text-generation".into()],
            estimated_latency_ms: 900,
            estimated_cost_micros: 0,
            data_allowed: true,
        },
        BackendOption {
            backend: ModelBackend::Cloud,
            available: true,
            capabilities: vec!["text-generation".into()],
            estimated_latency_ms: 120,
            estimated_cost_micros: 3000,
            data_allowed: false,
        },
    ]
}

#[test]
fn local_rule_prefers_local_for_private_data() {
    let runtime = runtime(None);
    let result = runtime.decide_backend(DecisionContext {
        task_id: "T1".into(),
        node_id: "gen".into(),
        required_capabilities: vec!["text-generation".into()],
        data_class: DataClass::Private,
        allow_cloud: true,
        candidates: candidates(),
    }).unwrap();
    assert_eq!(result.backend, Some(ModelBackend::Local));
    assert!(result.accepted);
    assert_eq!(result.proposed_by, "local-rule");
}

#[test]
fn cloud_advisor_cannot_override_private_data_gate() {
    let runtime = runtime(Some(Arc::new(CloudBiasedAdvisor)));
    let result = runtime.decide_backend(DecisionContext {
        task_id: "T1".into(),
        node_id: "gen".into(),
        required_capabilities: vec!["text-generation".into()],
        data_class: DataClass::Private,
        allow_cloud: false,
        candidates: candidates(),
    }).unwrap();
    assert_eq!(result.backend, Some(ModelBackend::Local));
    assert!(!result.accepted);
    assert!(result.reasons.iter().any(|reason| reason.contains("rejected")));
}

#[test]
fn cloud_is_allowed_only_when_data_and_switch_permit_it() {
    let runtime = runtime(Some(Arc::new(CloudBiasedAdvisor)));
    let cloud_allowed = vec![
        BackendOption {
            backend: ModelBackend::Local,
            available: true,
            capabilities: vec!["text-generation".into()],
            estimated_latency_ms: 900,
            estimated_cost_micros: 0,
            data_allowed: true,
        },
        BackendOption {
            backend: ModelBackend::Cloud,
            available: true,
            capabilities: vec!["text-generation".into()],
            estimated_latency_ms: 120,
            estimated_cost_micros: 3000,
            data_allowed: true,
        },
    ];
    let result = runtime.decide_backend(DecisionContext {
        task_id: "T1".into(),
        node_id: "gen".into(),
        required_capabilities: vec!["text-generation".into()],
        data_class: DataClass::Public,
        allow_cloud: true,
        candidates: cloud_allowed,
    }).unwrap();
    assert_eq!(result.backend, Some(ModelBackend::Cloud));
    assert!(result.accepted);
}

#[test]
fn no_eligible_backend_returns_none() {
    let runtime = runtime(None);
    let result = runtime.decide_backend(DecisionContext {
        task_id: "T1".into(),
        node_id: "gen".into(),
        required_capabilities: vec!["text-generation".into()],
        data_class: DataClass::Private,
        allow_cloud: false,
        candidates: vec![BackendOption {
            backend: ModelBackend::Local,
            available: false,
            capabilities: vec![],
            estimated_latency_ms: 0,
            estimated_cost_micros: 0,
            data_allowed: true,
        }],
    }).unwrap();
    assert_eq!(result.backend, None);
    assert!(!result.accepted);
}

#[test]
fn decision_rejects_invalid_context() {
    let runtime = runtime(None);
    assert!(runtime.decide_backend(DecisionContext {
        task_id: "".into(),
        node_id: "gen".into(),
        required_capabilities: vec![],
        data_class: DataClass::Public,
        allow_cloud: false,
        candidates: candidates(),
    }).is_err());
    assert!(runtime.decide_backend(DecisionContext {
        task_id: "T1".into(),
        node_id: "gen".into(),
        required_capabilities: vec![],
        data_class: DataClass::Public,
        allow_cloud: false,
        candidates: vec![],
    }).is_err());
}
