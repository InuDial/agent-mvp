# Design Purpose Index

Use this file when you see a type or pattern and want to know why it exists.

| Design | Purpose | Main code |
| --- | --- | --- |
| `ToolImpl` | Lets user/builtin code express behavior without owning runtime authority. | `crates/kernel/src/tool/adapter.rs` |
| `ToolContext` | Gives tools controlled service facades and nested invocation without exposing backend handles. | `crates/kernel/src/tool/context.rs` |
| `Kernel::ToolPath` | Lets each concrete kernel choose the registered tool identifier type. | `crates/kernel/src/kernel.rs`, `crates/app/src/lib.rs` |
| Service facade | Converts ergonomic tool calls into auditable actions. | `crates/service-fs/src/service.rs`, `crates/service-network/src/service.rs`, `crates/service-monty/src/service.rs` |
| Backend / store | Performs direct domain operations after a service has obtained a grant. | `crates/service-fs/src/backend.rs`, `crates/service-network/src/backend.rs`, `crates/service-monty/src/store.rs` |
| `Action` | Makes side-effect intent explicit for policy and audit. | `crates/kernel/src/action.rs` |
| `ActionExecutor` | Keeps execution on backend/store objects while preserving a shared grant flow. | `crates/kernel/src/action.rs`, `crates/service-fs/src/backend.rs`, `crates/service-network/src/backend.rs`, `crates/service-monty/src/store.rs` |
| `PolicyPlane` | Centralizes authorization order and default deny behavior. | `crates/kernel/src/policy/plane.rs` |
| Inbound policies | Apply global gates before resource-specific policy can allow anything. | `crates/kernel/src/policy/plane.rs` |
| `CapabilityEnvelopePolicy` | Prevents an action from exceeding the invocation's effective capabilities. | `crates/kernel/src/policy/plane.rs` |
| Typed policies | Keep resource authorization scoped to concrete action types. | `crates/kernel/src/policy/traits.rs`, `crates/service-fs/src/policy.rs`, `crates/service-network/src/policy.rs`, `crates/service-monty/src/policy.rs` |
| `PolicyGrant.reason` | Stores the policy's human-readable explanation. Deny reasons can become user-facing authorization errors. | `crates/kernel/src/policy/decision.rs` |
| `PolicyGrant.predicate` | Stores the exact predicate used for DEBUG policy diagnostics. | `crates/kernel/src/policy/decision.rs`, `crates/kernel/src/audit.rs` |
| `Granted<Action>` | Represents authorization as a value required before execution. | `crates/kernel/src/policy/grant.rs` |
| `GrantId` | Correlates final grant records with execution records. | `crates/kernel/src/tool/mod.rs`, `crates/kernel/src/audit.rs` |
| `AuditResource` | Gives grant and execution records a stable resource field. | `crates/kernel/src/action.rs` |
| `policy.evaluate` audit | Shows each policy's decision at DEBUG level for diagnostics. | `crates/kernel/src/audit.rs`, `crates/kernel/src/policy/plane.rs` |
| `grant.allow` / `grant.deny` audit | Records the final authorization fact at INFO level. | `crates/kernel/src/audit.rs` |
| `execute.start` / `execute.finish` / `execute.error` audit | Records execution lifecycle after a grant. | `crates/kernel/src/audit.rs`, `crates/kernel/src/policy/grant.rs` |
| `CanonicalPath` | Ensures fs actions compare canonical filesystem paths, not raw user strings. | `crates/service-fs/src/action.rs` |
| `CanonicalRoot` | Keeps workspace containment checks on canonical roots. | `crates/service-fs/src/action.rs`, `crates/kernel/src/policy/context.rs` |
| `CanonicalPrefix` | Keeps prefix policies in the same canonical path space as actions. | `crates/service-fs/src/action.rs`, `crates/service-fs/src/policy.rs` |
| `FsAction` parent action | Authorizes and audits shared fs path resolution before read/write-specific actions. | `crates/service-fs/src/action.rs`, `crates/service-fs/src/service.rs` |
| Monty session actions | Persist Monty REPL state through the same policy, grant, and audit flow as other services. | `crates/service-monty/src/action.rs`, `crates/service-monty/src/service.rs`, `crates/service-monty/src/store.rs` |
| `MontyTool` | Runs Monty code while routing exposed host tool calls back through nested tool invocation. | `crates/tool-monty/src/lib.rs` |
| `MontyOsTool` | Handles supported Monty OS calls by delegating to service facades such as `ctx.fs()`. | `crates/tool-monty/src/lib.rs` |
| Nested capability inheritance | Prevents delegated tool calls from expanding authority. | `crates/app/src/lib.rs`, `crates/test-support/src/lib.rs` |
| `ToolOutcome.classification` | Carries output sensitivity metadata in the contract. It is not yet enforced as an output policy. | `crates/contract/src/lib.rs` |

## Design Reading Map

If you want to understand one workflow end to end, start with filesystem read:

1. `mvp-tool-builtin/src/read_file.rs`: tool asks for `ctx.fs().read_file(...)`
2. `mvp-service-fs/src/service.rs`: service creates fs actions
3. `mvp-service-fs/src/action.rs`: canonical paths and action metadata
4. `mvp-kernel/src/policy/plane.rs`: policy evaluation order
5. `mvp-service-fs/src/policy.rs`: resource-specific policies
6. `mvp-kernel/src/policy/grant.rs`: grant value and execution audit wrapper
7. `mvp-kernel/src/audit.rs`: audit event shape
