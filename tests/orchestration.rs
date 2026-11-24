use std::collections::BTreeMap;
use std::sync::Arc;
use xiaohong_agent_runtime::{
    validate_plan, AgentGoal, AgentRuntime, BoardNotePlanner, ExecutionPolicy, NodeResult,
    NodeStatus, Registry, RuntimeError, RuntimeResult, SideEffect, TaskNode, TaskPlan,
    Tool, ToolDescriptor, UserApproval,
};

struct Stub(ToolDescriptor);
impl Tool for Stub {
    fn descriptor(&self) -> ToolDescriptor { self.0.clone() }
    fn execute(&self, _: &AgentGoal, node: &TaskNode, _: &BTreeMap<String, NodeResult>) -> RuntimeResult<NodeResult> {
        Ok(NodeResult { output: serde_json::json!({"node": node.node_id}), evidence_refs: vec![] })
    }
}

fn registry() -> Registry {
    let mut registry=Registry::default();
    for (id,side_effect,perms) in [
        ("m1.perceive",SideEffect::ReadOnly,vec![]),
        ("m2.retrieve",SideEffect::ReadOnly,vec![]),
        ("m5.generate",SideEffect::ReadOnly,vec![]),
        ("app.notes.create",SideEffect::LocalWrite,vec!["notes.write".to_owned()]),
    ] {
        registry.register(Arc::new(Stub(ToolDescriptor {
            tool_id:id.into(),description:id.into(),side_effect:side_effect.clone(),
            required_permissions:perms,timeout_ms:1000,idempotent:true,
        }))).unwrap();
    }
    registry
}

fn goal()->AgentGoal { AgentGoal { task_id:"T1".into(),instruction:"整理板书".into(),context_refs:vec![] } }
fn policy()->ExecutionPolicy { ExecutionPolicy { granted_permissions:vec!["notes.write".into()],approved_nodes:vec![],allow_external_write:false,max_nodes:16 } }

#[test]
fn board_plan_runs_reads_before_draft_and_holds_save_for_approval() {
    let rt=AgentRuntime::new(Arc::new(BoardNotePlanner),registry(),policy());
    let g=goal();let plan=rt.plan(&g).unwrap();let mut state=rt.start(g,plan).unwrap();
    rt.run_ready(&mut state).unwrap();
    assert_eq!(state.statuses["read_board"],NodeStatus::Completed);
    assert_eq!(state.statuses["search_notes"],NodeStatus::Completed);
    assert!(!state.statuses.contains_key("draft_note"));
    rt.run_ready(&mut state).unwrap();assert_eq!(state.statuses["draft_note"],NodeStatus::Completed);
    rt.run_ready(&mut state).unwrap();assert_eq!(state.statuses["user_review"],NodeStatus::AwaitingApproval);
    assert!(!state.statuses.contains_key("save_note"));
}

#[test]
fn write_node_cannot_be_smuggled_as_auto_executable() {
    let plan=TaskPlan{task_id:"T".into(),nodes:vec![TaskNode{node_id:"save".into(),tool_id:"app.notes.create".into(),input:serde_json::json!({}),depends_on:vec![],requires_approval:false}]};
    assert!(matches!(validate_plan(&plan,&registry(),&policy()),Err(RuntimeError::InvalidPlan(_))));
}

#[test]
fn approval_is_bound_to_exact_tool_input_and_task() {
    let rt=AgentRuntime::new(Arc::new(BoardNotePlanner),registry(),policy());let g=goal();let plan=rt.plan(&g).unwrap();let mut state=rt.start(g,plan).unwrap();
    for _ in 0..3 { rt.run_ready(&mut state).unwrap(); }
    assert!(matches!(rt.approve_and_run(&mut state,&UserApproval{task_id:"T1".into(),node_id:"user_review".into(),input_sha256:"forged".into()}),Err(RuntimeError::InvalidPlan(_))));
}

#[test]
fn checkpoint_round_trip_preserves_completed_nodes_and_events() {
    let rt=AgentRuntime::new(Arc::new(BoardNotePlanner),registry(),policy());let g=goal();let plan=rt.plan(&g).unwrap();let mut state=rt.start(g.clone(),plan.clone()).unwrap();rt.run_ready(&mut state).unwrap();
    let restored=xiaohong_agent_runtime::RunState::restore(g,plan,policy(),state.snapshot()).unwrap();
    assert_eq!(restored.statuses["read_board"],NodeStatus::Completed);
    assert_eq!(restored.statuses["search_notes"],NodeStatus::Completed);
    assert_eq!(restored.events.len(),state.events.len());
}