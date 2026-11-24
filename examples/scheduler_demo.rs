use std::sync::Arc;
use xiaohong_agent_runtime::{
    AgentGoal, AgentRuntime, BoardNotePlanner, ExecutionPolicy, NodeResult, Registry,
    RuntimeError, RuntimeResult, SideEffect, TaskNode, Tool, ToolDescriptor,
};

struct DemoTool(ToolDescriptor);
impl Tool for DemoTool {
    fn descriptor(&self) -> ToolDescriptor { self.0.clone() }
    fn execute(&self, _task: &AgentGoal, node: &TaskNode,
        _prior: &std::collections::BTreeMap<String,NodeResult>) -> RuntimeResult<NodeResult> {
        Ok(NodeResult {
            output: serde_json::json!({"demo":"completed", "nodeId":node.node_id}),
            evidence_refs: vec![],
        })
    }
}

fn main() -> Result<(),Box<dyn std::error::Error>> {
    let mut registry=Registry::default();
    for (id,effect,permission) in [
        ("m1.perceive",SideEffect::ReadOnly,None),
        ("m2.retrieve",SideEffect::ReadOnly,None),
        ("m5.generate",SideEffect::ReadOnly,None),
        ("runtime.awaitUser",SideEffect::LocalWrite,Some("user.review")),
        ("app.notes.create",SideEffect::LocalWrite,Some("notes.write")),
    ] {
        registry.register(Arc::new(DemoTool(ToolDescriptor {
            tool_id:id.into(),description:id.into(),side_effect:effect,
            required_permissions:permission.into_iter().map(str::to_owned).collect(),
            timeout_ms:5000,idempotent:true,
        })))?;
    }
    let planner=Arc::new(BoardNotePlanner);
    let runtime=AgentRuntime::new(planner,registry,ExecutionPolicy {
        granted_permissions:vec!["user.review".into(),"notes.write".into()],
        approved_nodes:vec![],allow_external_write:false,max_nodes:32,
    });
    let goal=AgentGoal{task_id:"T-BOARD-001".into(),instruction:"整理这张黑板照片为个人风格笔记".into(),context_refs:vec!["photo:M1-demo-board".into()]};
    let plan=runtime.plan(&goal)?;
    println!("PLAN={}",serde_json::to_string_pretty(&plan)?);
    let mut state=runtime.start(goal,plan)?;
    let events=runtime.run_ready(&mut state)?;
    println!("FIRST_WAVE={}",serde_json::to_string_pretty(&events)?);
    println!("SNAPSHOT={}",serde_json::to_string_pretty(&state.snapshot())?);
    assert_eq!(events.iter().filter(|e|e.status==xiaohong_agent_runtime::NodeStatus::Completed).count(),2);
    assert_eq!(state.statuses.get("draft_note"),None);
    // The two read-only branches complete in wave 1; the dependent draft runs
    // in wave 2; wave 3 exposes the explicit human-approval gate.
    let next=runtime.run_ready(&mut state)?;
    println!("SECOND_WAVE={}",serde_json::to_string_pretty(&next)?);
    let review=runtime.run_ready(&mut state)?;
    println!("APPROVAL_WAVE={}",serde_json::to_string_pretty(&review)?);
    assert!(state.statuses.get("user_review")==Some(&xiaohong_agent_runtime::NodeStatus::AwaitingApproval));
    let forged=runtime.approve_and_run(&mut state,&xiaohong_agent_runtime::UserApproval{task_id:"T-BOARD-001".into(),node_id:"user_review".into(),input_sha256:"wrong".into()});
    assert!(matches!(forged,Err(RuntimeError::InvalidPlan(_))));
    println!("APPROVAL_GATE=active; forged input fingerprint rejected");
    Ok(())
}
