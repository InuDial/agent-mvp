# Architecture

This project is a small runtime architecture for executing tools with explicit
authority boundaries.

The central rule is:

```text
tool code never performs side effects directly
```

Instead, tools ask kernel-owned services to do work. Services translate requests
into typed actions, the policy plane grants or denies those actions, and only a
granted action can execute against the domain executor. Audit records describe
the invocation, policy decision, and execution result.

## Runtime Flow

```text
Host
  -> Kernel::invoke
  -> ToolContext
  -> ToolImpl
  -> Service facade
  -> Action
  -> PolicyPlane
  -> Granted<Action>
  -> Executor
  -> Audit
```

The important part is the direction of control:

- `ToolImpl` expresses intent.
- A service facade owns side-effect translation.
- `Action` is the unit policy understands.
- `PolicyPlane` decides whether the action is granted.
- `Granted<Action>` is the only value that can execute.
- The executor performs the domain operation.

## Main Concepts

### Tool

Purpose:
Represent user or builtin behavior without giving it direct authority to perform
side effects.

Code:
- `crates/kernel/src/tool/adapter.rs`
- `crates/kernel/src/tool/context.rs`
- `crates/tool-builtin/src`

### Service

Purpose:
Keep tool logic separate from side-effect domains.

A service facade exposes ergonomic methods such as `ctx.fs().read_file(...)`.
Internally it resolves the request into one or more actions, asks policy for a
grant, and executes only granted actions.

Code:
- `crates/service-fs/src/service.rs`
- `crates/service-network/src/service.rs`
- `crates/service-monty/src/service.rs`

### Action

Purpose:
Make side-effect intent explicit and auditable.

Actions declare their required capabilities, audit kind, and audit resource.
Executable actions also know how to run against a domain executor after policy
grants them.

Code:
- `crates/kernel/src/action.rs`
- `crates/service-fs/src/action.rs`
- `crates/service-network/src/action.rs`
- `crates/service-monty/src/action.rs`

### Policy

Purpose:
Separate authorization rules from tool and service code.

The policy plane evaluates actions in this order:

1. inbound global policies
2. typed action-specific policies
3. outbound global policies
4. default deny

The capability envelope is an inbound policy. Resource policies are typed
policies.

Code:
- `crates/kernel/src/policy/plane.rs`
- `crates/kernel/src/policy/traits.rs`
- `crates/service-fs/src/policy.rs`
- `crates/service-network/src/policy.rs`
- `crates/service-monty/src/policy.rs`

### Grant

Purpose:
Represent authorization as a value.

Services cannot execute an action directly. They need `Granted<Action>`, which
is created only by the policy plane. `GrantId` links grant audit records to
execution audit records.

Code:
- `crates/kernel/src/policy/grant.rs`

### Audit

Purpose:
Make decisions inspectable.

Final grant decisions and execution records are `INFO`. Per-policy evaluation
records are `DEBUG` because they are diagnostic detail rather than the final
authorization fact.

Code:
- `crates/kernel/src/audit.rs`

## Filesystem Path Model

Filesystem policy compares canonical paths, not raw user strings.

Purpose:
Avoid authorizing based on path aliases such as `/var` vs `/private/var`,
relative path segments, or symlinks.

Types:
- `CanonicalPath`: a canonical filesystem path used by fs actions.
- `CanonicalRoot`: a canonical workspace root with containment checks.
- `CanonicalPrefix`: a canonical policy prefix with containment checks.

Write targets that do not exist are represented by canonicalizing their parent
directory and then re-attaching the requested file name.

Code:
- `crates/service-fs/src/action.rs`
- `crates/service-fs/src/policy.rs`

Tradeoff:
This is still path-based authorization. It improves consistency and audit
clarity, but it is not a file-descriptor capability model.

## Capability Model

Tool capabilities describe the maximum default authority a tool needs.
Invocation parameters produce the effective capability envelope for a specific
call.

Rules:

- top-level call without override uses the tool's declared capabilities
- top-level call with override uses the requested envelope
- nested call without override inherits the parent envelope
- nested override must stay within the parent envelope

The `CapabilityEnvelopePolicy` denies actions whose required capabilities exceed
the current effective envelope.

Code:
- `crates/contract/src/lib.rs`
- `crates/kernel/src/policy/plane.rs`
- `crates/app/src/lib.rs`

## Where To Start Reading

Read in this order:

1. `crates/kernel/src/action.rs`
2. `crates/kernel/src/policy/decision.rs`
3. `crates/kernel/src/policy/plane.rs`
4. `crates/kernel/src/policy/grant.rs`
5. `crates/service-fs/src/service.rs`
6. `crates/service-fs/src/action.rs`
7. `crates/service-fs/src/policy.rs`
8. `crates/service-network/src/service.rs`
9. `crates/service-monty/src/service.rs`
10. `crates/kernel/src/audit.rs`
11. `crates/app/src/lib.rs`
12. `crates/tool-builtin/src`
13. `crates/tool-monty/src/lib.rs`
