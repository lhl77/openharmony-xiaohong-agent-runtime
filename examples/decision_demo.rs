use std::sync::Arc;
use xiaohong_agent_runtime::*;

/// A cloud LLM decision advisor: proposes cloud, but the runtime gate wins
/// when the workload is private or cloud is disabled.
struct CloudAdvisor;
impl DecisionAdvisor for CloudAdvisor {
    fn name(&self) -> &str { "cloud-llm" }

    fn advise(&self, _context: &DecisionContext) -> RuntimeResult<DecisionProposal> {
        Ok(DecisionProposal {
            backend: ModelBackend::Cloud,
            rationale: "cloud model has the best quality".into(),
            confidence: 0.95,
        })
    }
}

struct NeverPlanner;
impl Planner for NeverPlanner {
    fn propose(&self, _goal: &AgentGoal, _tools: &[ToolDescriptor]) -> RuntimeResult<TaskPlan> {
        unreachable!("decision demo does not propose a plan")
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let planner: Arc<dyn Planner> = Arc::new(NeverPlanner);
    let advisor: Arc<dyn DecisionAdvisor> = Arc::new(CloudAdvisor);
    let runtime = AgentRuntime::with_advisor(planner, advisor, Registry::default(), ExecutionPolicy::default());
    let candidates = vec![
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
    ];
    let private = runtime.decide_backend(DecisionContext {
        task_id: "T1".into(),
        node_id: "gen".into(),
        required_capabilities: vec!["text-generation".into()],
        data_class: DataClass::Private,
        allow_cloud: false,
        candidates: candidates.clone(),
    })?;
    println!("PRIVATE_DECISION={}", serde_json::to_string_pretty(&private)?);

    let public_candidates = vec![
        candidates[0].clone(),
        BackendOption { data_allowed: true, ..candidates[1].clone() },
    ];
    let public = runtime.decide_backend(DecisionContext {
        task_id: "T2".into(),
        node_id: "gen".into(),
        required_capabilities: vec!["text-generation".into()],
        data_class: DataClass::Public,
        allow_cloud: true,
        candidates: public_candidates,
    })?;
    println!("PUBLIC_DECISION={}", serde_json::to_string_pretty(&public)?);
    Ok(())
}
