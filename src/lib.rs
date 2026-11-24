//! AI proposes a task graph; a deterministic runtime validates policy and schedules tools.
mod runtime;
mod types;
pub use runtime::{AgentRuntime,BoardNotePlanner,Registry,RunState,Tool,input_fingerprint,validate_plan};
pub use types::*;

use std::ffi::{CStr,CString,c_char};
use std::panic::{AssertUnwindSafe,catch_unwind};
use std::sync::{Arc,OnceLock,RwLock};
const MAX_INPUT:usize=16*1024*1024;
static RUNTIME:OnceLock<RwLock<Option<AgentRuntime>>>=OnceLock::new();
fn slot()->&'static RwLock<Option<AgentRuntime>>{RUNTIME.get_or_init(||RwLock::new(None))}

pub trait Planner:Send+Sync { fn propose(&self,goal:&AgentGoal,tools:&[ToolDescriptor])->RuntimeResult<TaskPlan>; }

pub fn install_runtime(planner:Arc<dyn Planner>,registry:Registry,policy:ExecutionPolicy)->RuntimeResult<()>{*slot().write().map_err(|_|RuntimeError::PoisonedLock)?=Some(AgentRuntime::new(planner,registry,policy));Ok(())}
fn envelope<T:serde::Serialize>(r:RuntimeResult<T>)->String{match r{Ok(data)=>serde_json::json!({"ok":true,"data":data}).to_string(),Err(e)=>serde_json::json!({"ok":false,"error":{"code":e.code(),"message":e.to_string()}}).to_string()}}
unsafe fn input(p:*const c_char)->RuntimeResult<String>{if p.is_null(){return Err(RuntimeError::NullPointer)}let b=unsafe{CStr::from_ptr(p)}.to_bytes();if b.len()>MAX_INPUT{return Err(RuntimeError::InvalidPlan("JSON exceeds 16 MiB".into()))}Ok(std::str::from_utf8(b).map_err(|_|RuntimeError::InvalidPlan("JSON must be UTF-8".into()))?.to_owned())}
fn cstr(s:String)->*mut c_char{CString::new(s).map(CString::into_raw).unwrap_or(std::ptr::null_mut())}
fn ffi<T:serde::Serialize>(p:*const c_char,f:impl FnOnce(&str)->RuntimeResult<T>)->*mut c_char{match catch_unwind(AssertUnwindSafe(||{let r=unsafe{input(p)}.and_then(|s|f(&s));cstr(envelope(r))})){Ok(p)=>p,Err(_)=>cstr(envelope::<()>(Err(RuntimeError::InvalidPlan("panic caught at FFI boundary".into()))))}}
#[derive(serde::Deserialize)]#[serde(rename_all="camelCase")]struct StartRequest{goal:AgentGoal,plan:TaskPlan}
#[unsafe(no_mangle)]pub unsafe extern "C" fn m3_propose_plan_json(p:*const c_char)->*mut c_char{ffi(p,|s|{let r:StartRequest=serde_json::from_str(s)?;slot().read().map_err(|_|RuntimeError::PoisonedLock)?.as_ref().ok_or_else(||RuntimeError::InvalidPlan("runtime is not initialized".into()))?.plan(&r.goal)})}
#[unsafe(no_mangle)]pub unsafe extern "C" fn m3_validate_plan_json(p:*const c_char)->*mut c_char{ffi(p,|s|{let plan:TaskPlan=serde_json::from_str(s)?;let slot=slot().read().map_err(|_|RuntimeError::PoisonedLock)?;let rt=slot.as_ref().ok_or_else(||RuntimeError::InvalidPlan("runtime is not initialized".into()))?;validate_plan(&plan,rt.registry(),rt.policy())?;Ok(plan)})}
#[unsafe(no_mangle)]pub unsafe extern "C" fn m3_start_json(p:*const c_char)->*mut c_char{ffi(p,|s|{let r:StartRequest=serde_json::from_str(s)?;let slot=slot().read().map_err(|_|RuntimeError::PoisonedLock)?;let rt=slot.as_ref().ok_or_else(||RuntimeError::InvalidPlan("runtime is not initialized".into()))?;validate_plan(&r.plan,rt.registry(),rt.policy())?;Ok(rt.start(r.goal,r.plan)?.snapshot())})}
#[unsafe(no_mangle)]pub unsafe extern "C" fn m3_free_string(p:*mut c_char){if !p.is_null(){drop(unsafe{CString::from_raw(p)});}}

#[cfg(test)]mod tests{use super::*;#[test]fn envelope_and_json_contract_work(){let v:TaskPlan=serde_json::from_str(r#"{"taskId":"T","nodes":[{"nodeId":"N1","toolId":"read","input":{},"dependsOn":[],"requiresApproval":false}]}"#).unwrap();assert_eq!(v.task_id,"T");assert!(envelope(Ok(v)).contains("\"ok\":true"));}}
