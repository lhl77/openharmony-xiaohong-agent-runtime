export interface PlanNode { nodeId:string; toolId:string; input:object; dependsOn:string[]; requiresApproval:boolean; }
export interface TaskPlan { taskId:string; nodes:PlanNode[]; }
export interface PlannerTool { toolId:string; description:string; sideEffect:'readOnly'|'localWrite'|'externalWrite'; requiredPermissions:string[]; timeoutMs:number; idempotent:boolean; }
export interface TaskRuntimeNative {
  /** Goal JSON; planner receives the runtime's registered tool descriptors. */
  proposePlan(goalJson:string):Promise<string>;
  validatePlan(planJson:string):Promise<string>;
  start(requestJson:string):Promise<string>;
}
declare const runtime:TaskRuntimeNative;
export default runtime;
