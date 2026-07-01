# Architecture

This project is a small runtime architecture for executing tools with explicit
authority boundaries.

The central rule is:

```text
tool code never performs side effects directly
```

Instead, tools ask controlled access facades to do work. Access facades translate requests
into typed actions, the policy engine grants or denies those actions, and only a
granted action can execute against the domain executor. Audit records describe
the invocation and policy decision.

## Runtime Flow

```text
Host
  -> App::invoke
  -> ToolContext
  -> ToolImpl
  -> Access facade
  -> Action
  -> PolicyEngine / PolicyPipeline
  -> Granted<Action>
  -> Executor
  -> Audit
```

The important part is the direction of control:

- `ToolImpl` expresses intent.
- An access facade owns side-effect translation.
- `Action` is the unit policy understands.
- `PolicyEngine::grant` creates a grant from the concrete engine's decision.
- `Granted<Action>` is the only value that can execute.
- The executor performs the domain operation.

The crate ownership is intentionally split: `mvp-core` owns the generic
`ToolHost` / `ToolContext` / `ToolImpl` API, `mvp-kernel` owns kernel runtime,
policy pipeline, backends, and audit helpers, and `mvp-app` owns the concrete tool
registry, invocation context, and nested invocation behavior.

## Main Concepts

### Tool

Purpose:
Represent user or builtin behavior without giving it direct authority to perform
side effects.

Code:
- `crates/core/src/tool`
- `crates/app/src/lib.rs`
- `crates/tool-builtin/src`

### Access

Purpose:
Keep tool logic separate from side-effect domains.

An access facade exposes ergonomic methods such as `ctx.fs().read_file(...)`.
Internally it resolves the request into one or more actions, asks policy for a
grant, and executes only granted actions.

Code:
- `crates/access-fs/src/access.rs`
- `crates/access-network/src/access.rs`
- `crates/access-monty/src/access.rs`

### Action

Purpose:
Make side-effect intent explicit and auditable.

Actions declare their required capabilities, audit kind, and audit resource.
Executors know how to run granted actions against their domain backend or store.

Code:
- `crates/core/src/action.rs`
- `crates/access-fs/src/action.rs`
- `crates/access-network/src/action.rs`
- `crates/access-monty/src/action.rs`

### Policy

Purpose:
Separate authorization rules from tool and access code.

The core crate defines the `PolicyEngine` trait. Its default `grant` implementation
creates `Granted<Action>` values after the concrete engine returns a decision.
The kernel's `PolicyPipeline` evaluates actions in this order:

1. inbound global policies
2. typed action-specific policies
3. outbound global policies
4. default deny

The capability envelope is an inbound policy. Resource policies are typed
policies.

Code:
- `crates/core/src/policy/traits.rs`
- `crates/kernel/src/policy/pipeline.rs`
- `crates/access-fs/src/policy.rs`
- `crates/access-network/src/policy.rs`
- `crates/access-monty/src/policy.rs`

### Grant

Purpose:
Represent authorization as a value.

Access facades cannot execute an action directly. They need `Granted<Action>`, which
is created only by the core-owned grant path. `GrantId` identifies final allow records.

Code:
- `crates/core/src/policy/grant.rs`

### Executor

Purpose:
Keep domain side effects on backend or store objects, while actions remain
policy-facing intent values.

Backend and store traits accept only concrete `Granted<Action>` values for
side-effecting methods. This keeps the shared audit boundary around execution,
so access facades can delegate to domain executors without letting ungranted
actions run. Concrete runtimes wrap this path with audit before moving the grant
into the backend or store.

Code:
- `crates/core/src/policy/grant.rs`
- `crates/access-fs/src/backend.rs`
- `crates/access-network/src/backend.rs`
- `crates/access-monty/src/store.rs`

### Audit

Purpose:
Make decisions inspectable.

Final grant decisions are `INFO`. Per-policy evaluation records are `DEBUG`
because they are diagnostic detail rather than the final authorization fact.

Audit events use stable dot-separated names such as `grant.allow`,
`grant.deny`, and `policy.evaluate`. Optional values are emitted only when present, so JSON and OTel
consumers do not need to parse `Some(...)` / `None` debug strings or filter
empty sentinel values.

`examples/demo/` can emit newline-delimited JSON with span metadata by setting
`MVP_LOG_FORMAT=json`; the default subscriber remains the existing human-readable
formatter.

The demo can also export OpenTelemetry traces by setting
`MVP_TRACE_EXPORTER=otlp`. This installs an OTel trace layer alongside the fmt
layer and sends OTLP to `OTEL_EXPORTER_OTLP_ENDPOINT`, defaulting to
`http://localhost:4318/v1/traces` with OTLP/HTTP. Jaeger's `4317` port is
OTLP/gRPC; `4318` is OTLP/HTTP.

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
- `crates/access-fs/src/action.rs`
- `crates/access-fs/src/policy.rs`

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

The kernel's `CapabilityEnvelopePolicy` denies actions whose required capabilities
exceed the current effective envelope.

Code:
- `crates/contract/src/lib.rs`
- `crates/kernel/src/policy/builtins.rs`
- `crates/app/src/lib.rs`

## Where To Start Reading

Read in this order:

1. `crates/contract/src/lib.rs`
2. `crates/core/src/action.rs`
3. `crates/core/src/policy/traits.rs`
4. `crates/core/src/policy/grant.rs`
5. `crates/kernel/src/policy/pipeline.rs`
6. `crates/access-fs/src/access.rs`
7. `crates/access-fs/src/action.rs`
8. `crates/access-fs/src/policy.rs`
9. `crates/access-network/src/access.rs`
10. `crates/access-monty/src/access.rs`
11. `crates/kernel/src/audit.rs`
12. `crates/kernel/src/runtime/mod.rs`
13. `crates/app/src/lib.rs`
14. `crates/tool-builtin/src`
15. `crates/tool-monty/src/lib.rs`
