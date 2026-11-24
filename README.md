# xiaohong-agent-runtime · M3 任务编排运行时

小鸿助手 M3 是跨能力的本地统筹层：接收用户目标，调用 Planner 提出任务图，校验依赖与策略，逐波调度已就绪工具，汇总事件/检查点。**AI 可以建议如何拆任务；只有运行时能批准计划进入执行，且写入节点不能由 AI 自行批准。**

## 调度设计

1. **AI Planner（可替换）**：输入用户目标和已注册工具描述，输出 `TaskPlan` 候选 DAG。模型是建议来源，不是执行器。
2. **确定性校验器**：检查节点/工具是否注册、依赖是否存在及成环、节点数上限、工具权限与写入审批声明。未知工具或非法 DAG 在执行前拒绝。
3. **分层 ready 调度**：一轮仅取所有依赖已成功的节点。无依赖的 M1 看图和 M2 检索同一波并行；M5 草稿必须等两者完成。每波输出结构化 `TaskEvent`。
4. **工具执行边界**：工具按注册 ID 调用；输入是结构化 JSON。M1/M2/M5 通过工具 trait 接入，Runtime 不直接耦合这些 crate。
5. **审批与副作用**：写工具必须在 plan 中声明 `requiresApproval`，由 Runtime 进入 `awaitingApproval`。审批另行提交并绑定 task、node 和输入 fingerprint；AI 输出里不含“已批准”标志。外部写入还需应用明确允许。
6. **快照恢复**：`TaskSnapshot` 保存节点状态、结果与事件，用于进程/设备恢复。本参考内核提供序列化结构，不实现 ArkData 落盘或分布式同步。

示意 DAG：

```text
 M1 读照片 ──┐
              ├─> M5 起草 -> 用户校对/批准 -> 应用保存笔记
 M2 查旧笔记 ┘
```

## 项目布局

```
src/types.rs        TaskPlan、Tool、权限、事件、快照数据契约
src/runtime.rs      Registry、AI planner 边界、DAG 校验、波次调度、审批门
src/lib.rs          C ABI JSON envelope
include/m3_ffi.h    C 接口
napi/napi_init.cpp  OpenHarmony 异步 N-API 样例
arkts/              ArkTS 类型和调用示例
examples/            可实际运行的五节点板书整理调度演示
tests/               编排与失败路径单测
```

## 运行演示

```sh
cargo test --locked
cargo run --locked --example scheduler_demo
```

演示输出计划、并行只读第一波、生成草稿第二波及人工审批第三波。替身工具只回传 demo 状态，不会真的调用 M1/M2/M5 或写入笔记库；`BoardNotePlanner` 是确定性示例，可替换成 ArkTS/端侧模型实现的 AI Planner。

## OpenHarmony 集成状态

API 面向 ArkTS + N-API；C ABI 每个入口接收一个 JSON 字符串并返回 `{ok,data}` 或 `{ok:false,error}`。通过 `scripts/build-ohos.sh` 和 `OHOS_NDK_HOME` 交叉构建。当前环境未安装 OpenHarmony SDK/NDK，未验证设备构建/运行。接入实际 app 时，由宿主注册真实工具、权限、Planner 与事件订阅，再调用 Runtime。

## 范围与限制

- 此 crate 提供任务规划边界、策略校验、波次调度和检查点 schema；不内置 LLM、M1/M2/M5 实际工具或 ArkData 适配器。
- 示例 `inputFingerprint` 是非密码学 FNV-1a 绑定提示，不是认证或防篡改安全凭据。生产审批应由可信宿主用密码学摘要签发，并安全验证用户身份。
- `run_ready` 当前为同步演示执行器；超时、取消、并行线程池和真实持久化由宿主 adapter 实现。`timeoutMs` 是工具契约元数据，当前 demo runtime 不强行杀线程。
- ExternalWrite 默认关闭；同机的 `grantedPermissions` 也只是传入策略快照，不替代系统权限检查。
- 不承诺恰好一次外部副作用。恢复/重试策略由工具是否可查状态、是否支持幂等操作决定。

License: MIT OR Apache-2.0。
