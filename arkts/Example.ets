import runtime from 'libxiaohong_agent_runtime.so';

// Init/register the runtime and planner adapter during app startup. The planner
// proposes; the runtime validates the returned graph against its tool registry.
export async function createAndStartPlan(taskId: string, instruction: string): Promise<string> {
  const goal = {taskId, instruction, contextRefs: []};
  const proposed = JSON.parse(await runtime.proposePlan(JSON.stringify({goal})));
  if (!proposed.ok) return JSON.stringify(proposed);
  const plan = proposed.data;
  const validation = JSON.parse(await runtime.validatePlan(JSON.stringify(plan)));
  if (!validation.ok) return JSON.stringify(validation);
  return await runtime.start(JSON.stringify({goal, plan}));
}
