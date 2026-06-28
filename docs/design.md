# Design Purpose Index

Use this file when you see a type or pattern and want to know why it exists.

| Design | Purpose | Main code |
| --- | --- | --- |
| `ToolImpl` | Lets user/builtin code express behavior without owning runtime authority. | `crates/core/src/tool`, `crates/tool-builtin/src` |
| `ToolContext` | Gives tools controlled access facades and nested invocation without exposing backend handles. | `crates/core/src/tool`, `crates/app/src/lib.rs` |
| `ToolHost::ToolPath` | Lets each concrete tool host choose the registered tool identifier type. | `crates/core/src/tool`, `crates/app/src/lib.rs` |
| Access facade | Converts ergonomic tool calls into auditable actions. | `crates/access-fs/src/access.rs`, `crates/access-network/src/access.rs`, `crates/access-monty/src/access.rs` |
| Backend / store | Performs direct domain operations after an access facade has obtained a grant. | `crates/access-fs/src/backend.rs`, `crates/access-network/src/backend.rs`, `crates/access-monty/src/store.rs` |
| `Action` | Makes side-effect intent explicit for policy and audit. | `crates/core/src/action.rs` |
| `ActionExecutor` | Keeps execution on backend/store objects while preserving a shared grant flow. | `crates/core/src/action.rs`, `crates/access-fs/src/backend.rs`, `crates/access-network/src/backend.rs`, `crates/access-monty/src/store.rs` |
| `PolicyEngine` | Trait for deciding actions; its default `grant` implementation creates `Granted<Action>`. | `crates/core/src/policy/traits.rs` |
| `PolicyPipeline` | Concrete kernel policy engine with inbound, typed, outbound, and default-deny evaluation. | `crates/kernel/src/pipeline/mod.rs` |
| Inbound policies | Apply global gates before resource-specific policy can allow anything. | `crates/kernel/src/pipeline/mod.rs` |
| `CapabilityEnvelopePolicy` | Prevents an action from exceeding the invocation's effective capabilities. | `crates/kernel/src/pipeline/mod.rs` |
| Typed policies | Keep resource authorization scoped to concrete action types. | `crates/core/src/policy/traits.rs`, `crates/access-fs/src/policy.rs`, `crates/access-network/src/policy.rs`, `crates/access-monty/src/policy.rs` |
| `PolicyGrant.reason` | Stores the policy's human-readable explanation. Deny reasons can become user-facing authorization errors. | `crates/contract/src/lib.rs` |
| `PolicyGrant.predicate` | Stores the exact predicate used for DEBUG policy diagnostics. | `crates/contract/src/lib.rs`, `crates/kernel/src/audit.rs` |
| `Granted<Action>` | Represents authorization as a value required before execution. | `crates/core/src/policy/grant.rs` |
| `GrantId` | Correlates final grant records with execution records. | `crates/contract/src/lib.rs`, `crates/kernel/src/audit.rs` |
| `AuditResource` | Gives grant and execution records a stable resource field. | `crates/contract/src/lib.rs` |
| `policy.evaluate` audit | Shows each policy's decision at DEBUG level for diagnostics. | `crates/kernel/src/audit.rs`, `crates/kernel/src/pipeline/mod.rs` |
| `grant.allow` / `grant.deny` audit | Records the final authorization fact at INFO level. | `crates/kernel/src/audit.rs` |
| `execute.start` / `execute.finish` / `execute.error` audit | Records execution lifecycle after a grant. | `crates/kernel/src/audit.rs`, `crates/kernel/src/runtime/mod.rs` |
| `CanonicalPath` | Ensures fs actions compare canonical filesystem paths, not raw user strings. | `crates/access-fs/src/action.rs` |
| `CanonicalRoot` | Keeps workspace containment checks on canonical roots. | `crates/access-fs/src/action.rs`, `crates/kernel/src/policy_context.rs` |
| `CanonicalPrefix` | Keeps prefix policies in the same canonical path space as actions. | `crates/access-fs/src/action.rs`, `crates/access-fs/src/policy.rs` |
| `FsAction` parent action | Authorizes and audits shared fs path resolution before read/write-specific actions. | `crates/access-fs/src/action.rs`, `crates/access-fs/src/access.rs` |
| Monty session actions | Persist Monty REPL state through the same policy, grant, and audit flow as other services. | `crates/access-monty/src/action.rs`, `crates/access-monty/src/access.rs`, `crates/access-monty/src/store.rs` |
| `MontyTool` | Runs Monty code while routing exposed host tool calls back through nested tool invocation. | `crates/tool-monty/src/lib.rs` |
| `MontyOsTool` | Handles supported Monty OS calls by delegating to access facades such as `ctx.fs()`. | `crates/tool-monty/src/lib.rs` |
| Nested capability inheritance | Prevents delegated tool calls from expanding authority. | `crates/app/src/lib.rs`, `crates/test-support/src/lib.rs` |
| `ToolOutcome.classification` | Carries output sensitivity metadata in the contract. It is not yet enforced as an output policy. | `crates/contract/src/lib.rs` |

## Design Reading Map

If you want to understand one workflow end to end, start with filesystem read:

1. `mvp-tool-builtin/src/read_file.rs`: tool asks for `ctx.fs().read_file(...)`
2. `mvp-access-fs/src/access.rs`: `FsAccess` creates fs actions
3. `mvp-access-fs/src/action.rs`: canonical paths and action metadata
4. `mvp-kernel/src/pipeline/mod.rs`: policy evaluation order
5. `mvp-access-fs/src/policy.rs`: resource-specific policies
6. `mvp-core/src/policy/grant.rs`: grant value
7. `mvp-kernel/src/audit.rs`: audit event shape
