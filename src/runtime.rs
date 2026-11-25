use crate::types::*;
use crate::Planner;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

pub trait Tool: Send + Sync { fn descriptor(&self) -> ToolDescriptor; fn execute(&self, task: &AgentGoal, node: &TaskNode, prior: &BTreeMap<String, NodeResult>) -> RuntimeResult<NodeResult>; }

struct AwaitUserTool;
impl Tool for AwaitUserTool {
    fn descriptor(&self)->ToolDescriptor{ToolDescriptor{tool_id:"runtime.awaitUser".into(),description:"Pause the task and request user review/approval".into(),side_effect:SideEffect::LocalWrite,required_permissions:vec![],timeout_ms:0,idempotent:true}}
    fn execute(&self,_:&AgentGoal,_:&TaskNode,_:&BTreeMap<String,NodeResult>)->RuntimeResult<NodeResult>{Ok(NodeResult{output:serde_json::json!({"awaitingUser":true}),evidence_refs:vec![]})}
}

/// Deterministic reference plan for board-photo note taking. Production
/// applications can replace this proposal source with an AI-backed Planner.
pub struct BoardNotePlanner;
impl crate::Planner for BoardNotePlanner {
    fn propose(&self, goal: &AgentGoal, _tools: &[ToolDescriptor]) -> RuntimeResult<TaskPlan> {
        Ok(TaskPlan {
            task_id: goal.task_id.clone(),
            nodes: vec![
                TaskNode { node_id: "read_board".into(), tool_id: "m1.perceive".into(), input: serde_json::json!({"photoRef":"current-photo"}), depends_on: vec![], requires_approval: false },
                TaskNode { node_id: "search_notes".into(), tool_id: "m2.retrieve".into(), input: serde_json::json!({"query":"本节课主题"}), depends_on: vec![], requires_approval: false },
                TaskNode { node_id: "draft_note".into(), tool_id: "m5.generate".into(), input: serde_json::json!({"style":"user-profile"}), depends_on: vec!["read_board".into(),"search_notes".into()], requires_approval: false },
                TaskNode { node_id: "user_review".into(), tool_id: "runtime.awaitUser".into(), input: serde_json::json!({"reviewDraft":true}), depends_on: vec!["draft_note".into()], requires_approval: true },
                TaskNode { node_id: "save_note".into(), tool_id: "app.notes.create".into(), input: serde_json::json!({"source":"approved-draft"}), depends_on: vec!["user_review".into()], requires_approval: true },
            ],
        })
    }
}

#[derive(Default)]
pub struct Registry { tools: HashMap<String, Arc<dyn Tool>> }
impl Registry {
    pub fn register(&mut self, tool: Arc<dyn Tool>) -> RuntimeResult<()> {
        let id=tool.descriptor().tool_id;
        if id.trim().is_empty() || self.tools.contains_key(&id) { return Err(RuntimeError::InvalidPlan(format!("empty or duplicate tool ID: {id}"))); }
        self.tools.insert(id,tool); Ok(())
    }
    pub fn descriptors(&self)->Vec<ToolDescriptor>{let mut v=self.tools.values().map(|x|x.descriptor()).collect::<Vec<_>>();v.sort_by(|a,b|a.tool_id.cmp(&b.tool_id));v}
    fn tool(&self,id:&str)->RuntimeResult<Arc<dyn Tool>>{self.tools.get(id).cloned().ok_or_else(||RuntimeError::UnknownTool(id.into()))}
}

/// Decides whether a node should run on the local, nearby, or cloud model.
/// Hosts may implement this with a local lightweight model, an on-device rule
/// engine, or a cloud LLM; the runtime validates the proposal afterwards.
pub trait DecisionAdvisor: Send + Sync {
    fn name(&self) -> &str;
    fn advise(&self, context: &DecisionContext) -> RuntimeResult<DecisionProposal>;
}

/// Deterministic built-in "local AI": prefers the cheapest capable local backend
/// and never proposes cloud for data that is not allowed to leave the device.
pub struct RuleDecisionAdvisor;
impl DecisionAdvisor for RuleDecisionAdvisor {
    fn name(&self) -> &str { "local-rule" }

    fn advise(&self, context: &DecisionContext) -> RuntimeResult<DecisionProposal> {
        for backend in [ModelBackend::Local, ModelBackend::Nearby, ModelBackend::Cloud] {
            if let Some(candidate) = context.candidates.iter()
                .find(|candidate| candidate.backend == backend && backend_eligible(candidate, context))
            {
                return Ok(DecisionProposal {
                    backend: candidate.backend,
                    rationale: format!("{} satisfies capability and data constraints", candidate_name(candidate.backend)),
                    confidence: 1.0,
                });
            }
        }
        Ok(DecisionProposal {
            backend: ModelBackend::Local,
            rationale: "no eligible backend found".into(),
            confidence: 0.0,
        })
    }
}

fn candidate_name(backend: ModelBackend) -> &'static str {
    match backend { ModelBackend::Local => "local", ModelBackend::Nearby => "nearby", ModelBackend::Cloud => "cloud" }
}

/// Hard constraints: available, capability superset, data allowed, and cloud switch.
fn backend_eligible(candidate: &BackendOption, context: &DecisionContext) -> bool {
    if !candidate.available || !candidate.data_allowed {
        return false;
    }
    if candidate.backend == ModelBackend::Cloud && !context.allow_cloud {
        return false;
    }
    context.required_capabilities.iter().all(|required| candidate.capabilities.contains(required))
}

pub struct AgentRuntime { planner: Arc<dyn Planner>, advisor: Arc<dyn DecisionAdvisor>, registry: Registry, policy: ExecutionPolicy }
impl AgentRuntime {
    pub fn new(planner:Arc<dyn Planner>,registry:Registry,policy:ExecutionPolicy)->Self{
        Self::with_advisor(planner,Arc::new(RuleDecisionAdvisor),registry,policy)
    }
    pub fn with_advisor(planner:Arc<dyn Planner>,advisor:Arc<dyn DecisionAdvisor>,mut registry:Registry,policy:ExecutionPolicy)->Self{
        if !registry.tools.contains_key("runtime.awaitUser") { registry.tools.insert("runtime.awaitUser".into(),Arc::new(AwaitUserTool)); }
        Self{planner,advisor,registry,policy}
    }
    pub fn registry(&self)->&Registry{&self.registry}
    pub fn policy(&self)->&ExecutionPolicy{&self.policy}
    pub fn advisor_name(&self)->&str{self.advisor.name()}
    /// AI advises a backend; the runtime validates the proposal against hard constraints.
    pub fn decide_backend(&self,context:DecisionContext)->RuntimeResult<DecisionResult>{
        if context.task_id.trim().is_empty()||context.node_id.trim().is_empty(){return Err(RuntimeError::InvalidPlan("taskId and nodeId must be non-empty".into()));}
        if context.candidates.is_empty(){return Err(RuntimeError::InvalidPlan("candidates must not be empty".into()));}
        let proposal=self.advisor.advise(&context)?;
        let mut reasons=vec![format!("advisor {} proposed {:?}",self.advisor.name(),proposal.backend)];
        let candidate=context.candidates.iter().find(|candidate|candidate.backend==proposal.backend);
        let accepted=candidate.is_some_and(|candidate|backend_eligible(candidate,&context));
        if accepted{reasons.push("proposal passed hard-constraint validation".into());return Ok(DecisionResult{task_id:context.task_id,node_id:context.node_id,backend:Some(proposal.backend),proposed_by:self.advisor.name().into(),accepted:true,reasons});}
        if let Some(candidate)=candidate{reasons.push(format!("proposal rejected: available={}, dataAllowed={}, cloudAllowed={}",candidate.available,candidate.data_allowed,context.allow_cloud));}
        // Deterministic fallback keeps execution going without violating constraints.
        for backend in [ModelBackend::Local,ModelBackend::Nearby,ModelBackend::Cloud]{
            if let Some(candidate)=context.candidates.iter().find(|candidate|candidate.backend==backend&&backend_eligible(candidate,&context)){
                reasons.push(format!("fallback to {:?}",candidate.backend));
                return Ok(DecisionResult{task_id:context.task_id,node_id:context.node_id,backend:Some(candidate.backend),proposed_by:self.advisor.name().into(),accepted:false,reasons});
            }
        }
        reasons.push("no eligible backend; host should wait, degrade, or ask".into());
        Ok(DecisionResult{task_id:context.task_id,node_id:context.node_id,backend:None,proposed_by:self.advisor.name().into(),accepted:false,reasons})
    }
    /// The planner proposes; the deterministic runtime validates and schedules.
    pub fn plan(&self,goal:&AgentGoal)->RuntimeResult<TaskPlan>{let plan=self.planner.propose(goal,&self.registry.descriptors())?;validate_plan(&plan,&self.registry,&self.policy)?;Ok(plan)}
    pub fn start(&self,goal:AgentGoal,plan:TaskPlan)->RuntimeResult<RunState>{validate_plan(&plan,&self.registry,&self.policy)?;Ok(RunState{goal,plan,policy:self.policy.clone(),statuses:BTreeMap::new(),results:BTreeMap::new(),events:Vec::new()})}
    /// Execute all currently ready nodes that do not require user approval.
    pub fn run_ready(&self,state:&mut RunState)->RuntimeResult<Vec<TaskEvent>>{
        let ready=ready_nodes(&state.plan,&state.statuses);
        let before=state.events.len();
        for node in ready {
            let tool=self.registry.tool(&node.tool_id)?;
            let descriptor=tool.descriptor();
            if node.requires_approval {
                state.statuses.insert(node.node_id.clone(),NodeStatus::AwaitingApproval);
                state.events.push(event(state,&node.node_id,NodeStatus::AwaitingApproval,"waiting for user approval"));continue;
            }
            check_permissions(&descriptor,&state.policy)?;
            state.statuses.insert(node.node_id.clone(),NodeStatus::Running);
            state.events.push(event(state,&node.node_id,NodeStatus::Running,"tool started"));
            match tool.execute(&state.goal,&node,&state.results) {
                Ok(result)=>{state.results.insert(node.node_id.clone(),result);state.statuses.insert(node.node_id.clone(),NodeStatus::Completed);state.events.push(event(state,&node.node_id,NodeStatus::Completed,"tool completed"));}
                Err(error)=>{state.statuses.insert(node.node_id.clone(),NodeStatus::Failed);state.events.push(event(state,&node.node_id,NodeStatus::Failed,&error.to_string()));}
            }
        }
        Ok(state.events[before..].to_vec())
    }
    /// Approval is a separate user-originated call, bound to the exact node input.
    pub fn approve_and_run(&self,state:&mut RunState,approval:&UserApproval)->RuntimeResult<TaskEvent>{
        if approval.task_id!=state.goal.task_id{return Err(RuntimeError::InvalidPlan("approval taskId mismatch".into()));}
        let node=state.plan.nodes.iter().find(|n|n.node_id==approval.node_id).ok_or_else(||RuntimeError::InvalidPlan("approval node not found".into()))?;
        if state.statuses.get(&node.node_id)!=Some(&NodeStatus::AwaitingApproval){return Err(RuntimeError::InvalidPlan("node is not awaiting approval".into()));}
        if approval.input_sha256!=input_fingerprint(&node.input){return Err(RuntimeError::InvalidPlan("approval is bound to different input".into()));}
        let tool=self.registry.tool(&node.tool_id)?;let descriptor=tool.descriptor();check_permissions(&descriptor,&state.policy)?;
        if descriptor.side_effect==SideEffect::ExternalWrite&&!state.policy.allow_external_write{return Err(RuntimeError::PermissionDenied{tool:descriptor.tool_id,reason:"external writes disabled by app policy".into()});}
        state.statuses.insert(node.node_id.clone(),NodeStatus::Running);state.events.push(event(state,&node.node_id,NodeStatus::Running,"approved tool started"));
        match tool.execute(&state.goal,node,&state.results){Ok(result)=>{state.results.insert(node.node_id.clone(),result);state.statuses.insert(node.node_id.clone(),NodeStatus::Completed);let ev=event(state,&node.node_id,NodeStatus::Completed,"approved tool completed");state.events.push(ev.clone());Ok(ev)},Err(e)=>{state.statuses.insert(node.node_id.clone(),NodeStatus::Failed);let ev=event(state,&node.node_id,NodeStatus::Failed,&e.to_string());state.events.push(ev.clone());Ok(ev)}}
    }
}

#[derive(Clone,Debug)]
pub struct RunState { pub goal:AgentGoal,pub plan:TaskPlan,pub policy:ExecutionPolicy,pub statuses:BTreeMap<String,NodeStatus>,pub results:BTreeMap<String,NodeResult>,pub events:Vec<TaskEvent> }
impl RunState { pub fn snapshot(&self)->TaskSnapshot{TaskSnapshot{schema_version:1,task_id:self.goal.task_id.clone(),statuses:self.statuses.clone(),results:self.results.clone(),pending_events:self.events.clone()}} pub fn restore(goal:AgentGoal,plan:TaskPlan,policy:ExecutionPolicy,snapshot:TaskSnapshot)->RuntimeResult<Self>{if snapshot.task_id!=goal.task_id{return Err(RuntimeError::InvalidPlan("snapshot taskId mismatch".into()));}Ok(Self{goal,plan,policy,statuses:snapshot.statuses,results:snapshot.results,events:snapshot.pending_events})} }

pub fn validate_plan(plan:&TaskPlan,registry:&Registry,policy:&ExecutionPolicy)->RuntimeResult<()> {
 if plan.task_id.trim().is_empty()||plan.nodes.is_empty(){return Err(RuntimeError::InvalidPlan("taskId and nodes must be non-empty".into()));}
 if plan.nodes.len()>policy.max_nodes{return Err(RuntimeError::NodeLimit(policy.max_nodes));}
 let mut ids=HashSet::new();for n in &plan.nodes{if n.node_id.trim().is_empty()||!ids.insert(n.node_id.as_str()){return Err(RuntimeError::InvalidPlan("node IDs must be non-empty and unique".into()));}let tool=registry.tool(&n.tool_id)?;check_permissions(&tool.descriptor(),policy)?;if n.depends_on.iter().any(|d|d==&n.node_id){return Err(RuntimeError::DependencyCycle);}}
 for node in &plan.nodes {
  let descriptor=registry.tool(&node.tool_id)?.descriptor();
  if descriptor.side_effect!=SideEffect::ReadOnly&&!node.requires_approval {
   return Err(RuntimeError::InvalidPlan(format!("side-effect node {} must require approval",node.node_id)));
  }
 }
 for n in &plan.nodes{for dep in &n.depends_on{if !ids.contains(dep.as_str()){return Err(RuntimeError::InvalidPlan(format!("unknown dependency {dep}")));}}}
 // Kahn validation prevents an AI-proposed dependency cycle from deadlocking execution.
 let mut completed:HashSet<String>=HashSet::new();
 let mut remaining:HashMap<&str,usize>=plan.nodes.iter().map(|n|(n.node_id.as_str(),n.depends_on.len())).collect();
 while let Some(id)=remaining.iter().find(|(id,count)|**count==0&&!completed.contains(**id)).map(|(id,_)|*id){
  completed.insert(id.to_owned());
  for node in &plan.nodes{if node.depends_on.iter().any(|dep|dep==id){if let Some(count)=remaining.get_mut(node.node_id.as_str()){*count=count.saturating_sub(1);}}}
 }
 if completed.len()!=plan.nodes.len(){return Err(RuntimeError::DependencyCycle);}
 Ok(())
}
fn check_permissions(tool:&ToolDescriptor,policy:&ExecutionPolicy)->RuntimeResult<()>{if tool.side_effect==SideEffect::ExternalWrite&&!policy.allow_external_write{return Err(RuntimeError::PermissionDenied{tool:tool.tool_id.clone(),reason:"external writes disabled by app policy".into()});}for perm in &tool.required_permissions{if !policy.granted_permissions.contains(perm){return Err(RuntimeError::PermissionDenied{tool:tool.tool_id.clone(),reason:format!("missing permission {perm}")});}}Ok(())}
fn ready_nodes<'a>(plan:&'a TaskPlan,status:&BTreeMap<String,NodeStatus>)->Vec<&'a TaskNode>{let mut out=plan.nodes.iter().filter(|n|!status.contains_key(&n.node_id)&&n.depends_on.iter().all(|d|status.get(d)==Some(&NodeStatus::Completed))).collect::<Vec<_>>();out.sort_by(|a,b|a.node_id.cmp(&b.node_id));out}
pub fn input_fingerprint(input:&serde_json::Value)->String { // stable non-cryptographic binding token; production app may replace with SHA-256
 let bytes=serde_json::to_vec(input).unwrap_or_default();let mut h=0xcbf29ce484222325u64;for b in bytes{h^=u64::from(b);h=h.wrapping_mul(0x100000001b3);}format!("fnv1a64:{h:016x}")
}
fn event(s:&RunState,node:&str,status:NodeStatus,message:&str)->TaskEvent{TaskEvent{task_id:s.goal.task_id.clone(),node_id:node.into(),status,message:message.into()}}

#[cfg(test)]mod tests{use super::*;struct Echo{d:ToolDescriptor}impl Tool for Echo{fn descriptor(&self)->ToolDescriptor{self.d.clone()}fn execute(&self,_:&AgentGoal,n:&TaskNode,_:&BTreeMap<String,NodeResult>)->RuntimeResult<NodeResult>{Ok(NodeResult{output:n.input.clone(),evidence_refs:vec![]})}}struct NeverPlanner;impl Planner for NeverPlanner{fn propose(&self,_:&AgentGoal,_:&[ToolDescriptor])->RuntimeResult<TaskPlan>{unreachable!()}}
#[test]fn ready_nodes_wait_for_dependencies_and_side_effects_wait_for_approval(){let mut r=Registry::default();for(id,effect)in[("read",SideEffect::ReadOnly),("write",SideEffect::LocalWrite)]{r.register(Arc::new(Echo{d:ToolDescriptor{tool_id:id.into(),description:id.into(),side_effect:effect,required_permissions:vec![],timeout_ms:1000,idempotent:true}})).unwrap();}let policy=ExecutionPolicy::default();let rt=AgentRuntime::new(Arc::new(NeverPlanner),r,policy.clone());let plan=TaskPlan{task_id:"T1".into(),nodes:vec![TaskNode{node_id:"A".into(),tool_id:"read".into(),input:serde_json::json!({"q":"λ"}),depends_on:vec![],requires_approval:false},TaskNode{node_id:"B".into(),tool_id:"write".into(),input:serde_json::json!({"note":"draft"}),depends_on:vec!["A".into()],requires_approval:true}]};let mut s=rt.start(AgentGoal{task_id:"T1".into(),instruction:"demo".into(),context_refs:vec![]},plan).unwrap();rt.run_ready(&mut s).unwrap();assert_eq!(s.statuses["A"],NodeStatus::Completed);assert!(!s.statuses.contains_key("B"));rt.run_ready(&mut s).unwrap();assert_eq!(s.statuses["B"],NodeStatus::AwaitingApproval);}
#[test]fn cycle_is_rejected(){let mut r=Registry::default();r.register(Arc::new(Echo{d:ToolDescriptor{tool_id:"x".into(),description:"x".into(),side_effect:SideEffect::ReadOnly,required_permissions:vec![],timeout_ms:1,idempotent:true}})).unwrap();let p=TaskPlan{task_id:"T".into(),nodes:vec![TaskNode{node_id:"A".into(),tool_id:"x".into(),input:serde_json::json!({}),depends_on:vec!["B".into()],requires_approval:false},TaskNode{node_id:"B".into(),tool_id:"x".into(),input:serde_json::json!({}),depends_on:vec!["A".into()],requires_approval:false}]};assert!(matches!(validate_plan(&p,&r,&ExecutionPolicy::default()),Err(RuntimeError::DependencyCycle)));}}
