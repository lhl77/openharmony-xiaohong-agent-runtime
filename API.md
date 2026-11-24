# M3 Runtime API 参考

> 当前 N-API 工作样例暴露 `proposePlan`、`validatePlan`、`start` 三个 Promise 方法。应用装配层须先注册实际 Planner、ToolRegistry 与 ExecutionPolicy；仓库未内置 LLM 或业务工具。

## 数据契约

### AgentGoal

```json
{"taskId":"T-BOARD-001","instruction":"把这张板书整理成我的课程笔记","contextRefs":["photo:P1"]}
```

### TaskPlan

```json
{"taskId":"T-BOARD-001","nodes":[
 {"nodeId":"read_board","toolId":"m1.perceive","input":{"photoRef":"P1"},"dependsOn":[],"requiresApproval":false},
 {"nodeId":"search_notes","toolId":"m2.retrieve","input":{"query":"本节课主题"},"dependsOn":[],"requiresApproval":false},
 {"nodeId":"draft_note","toolId":"m5.generate","input":{"style":"user-profile"},"dependsOn":["read_board","search_notes"],"requiresApproval":false},
 {"nodeId":"user_review","toolId":"runtime.awaitUser","input":{"reviewDraft":true},"dependsOn":["draft_note"],"requiresApproval":true},
 {"nodeId":"save_note","toolId":"app.notes.create","input":{"source":"approved-draft"},"dependsOn":["user_review"],"requiresApproval":true}
]}
```

Planner 只可引用 ToolRegistry 内的 toolId，不能给节点附带任意命令、函数指针或可执行脚本。

### ToolDescriptor / policy

`ToolDescriptor` 包含 `toolId`、`sideEffect`（`readOnly|localWrite|externalWrite`）、`requiredPermissions`、`timeoutMs` 和 `idempotent`。`ExecutionPolicy` 包含宿主给出的权限快照、`allowExternalWrite` 和最大节点数。未知工具、权限不足、外部写入未开放、环路和过量节点均在计划进入调度前拒绝。所有非 `ReadOnly` 的工具节点都必须声明 `requiresApproval:true`，缺失时拒绝计划，防止 AI 把写操作标成可自动执行。

## Rust API

- `Registry::register(Arc<dyn Tool>)`：应用启动时注册工具实现与描述。
- `AgentRuntime::new(planner, registry, policy)`：组合 Planner、工具表与策略。
- `plan(goal)`：调用 Planner 取得候选图，再由 Runtime 验证。
- `start(goal, plan)`：再次验证后建立运行状态。适合 app 获取 AI 候选后再独立校验/呈现。
- `run_ready(&mut state)`：跑一波依赖已完成的 pending 节点。结果为 `Vec<TaskEvent>`；失败节点记为 failed，依赖它的节点不会放行。此参考版本顺序调用工具，ready 集合表示可并行集合，实际并行由执行器 adapter 实现。
- `approve_and_run(state, approval)`：校验审批 task/node/input fingerprint 与当前待审批节点，再调用工具。
- `RunState::snapshot/restore`：将状态转换成版本化序列化检查点；实际存取由宿主实现。

## 事件与审批

状态：`pending → ready → running → completed|failed`；ready 的审批节点转 `awaitingApproval`，不会执行工具；人工批准后才 `running`。拒绝/取消策略由宿主扩展。每个事件包含 `taskId`、`nodeId`、`status`、`message`。检查点包含节点状态、`NodeResult` 和待消费事件。

`UserApproval.inputSha256` 字段当前示例命名历史保留，**MVP 实现实际使用 `fnv1a64:<hex>` 非密码学 fingerprint**。它只用于绑定演示中的精确工具输入，不是 SHA-256、用户身份凭证或抗恶意篡改证明。生产实现必须由可信宿主改为 SHA-256/HMAC，并在平台身份/权限体系验证后创建审批对象；不可让 Planner 生成该值代替用户。示例 `BoardNotePlanner` 只生成候选图，写节点设置 `requiresApproval`；它本身不执行、也不授予批准。

## N-API/C ABI

C ABI (`include/m3_ffi.h`) JSON 入口：`m3_propose_plan_json`、`m3_validate_plan_json`、`m3_start_json`、`m3_free_string`。N-API 模块提供异步 Promise 包装。Runtime 初始化通过宿主 Rust 侧 `install_runtime(planner, registry, policy)` 完成；工作样例导出的 N-API 尚未包含生产期动态 tool registration/init API，因此如果单独加载未安装 runtime，业务调用会返回 runtime 未初始化错误。计划请求/响应 JSON envelope 为：

```json
{"ok":true,"data":{"taskId":"T1","nodes":[]}}
{"ok":false,"error":{"code":"DEPENDENCY_CYCLE","message":"..."}}
```

C ABI 的 propose/start 都校验计划；`start` 请求为 `{ "goal": AgentGoal, "plan": TaskPlan }`。`validatePlan` 的输入直接为 `TaskPlan`。

Envelope:

```json
{"ok":true,"data":{"taskId":"T-BOARD-001","nodes":[...]}}
{"ok":false,"error":{"code":"DEPENDENCY_CYCLE","message":"..."}}
```

## 板书笔记任务执行演算

1. Planner 提议 `read_board` 和 `search_notes` 两个无依赖节点；Runtime 第一波都 ready。
2. 两个结果都完成后 `draft_note` 才 ready。若任一失败，草稿不会拿缺失结果冒充完成。
3. `user_review` 和 `save_note` 是写操作声明，Runtime 把它们置为 `awaitingApproval`，不会与只读节点一起自动执行。
4. 人工批准必须匹配精确输入 fingerprint；修改草稿后，旧批准失效。
5. 保存后的操作幂等、崩溃恢复及跨设备快照冲突仍需工具/宿主实现。
