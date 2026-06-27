//! Monty-backed tool runtime.

use std::collections::BTreeMap;

use async_trait::async_trait;
use monty::{
    DictPairs, ExcType, ExtFunctionResult, LimitedTracker, MontyException, MontyObject, MontyRepl,
    NameLookupResult, PrintWriter, ReplProgress, ResourceLimits,
};
use mvp_contract::{Capabilities, OutputClassification, ToolOutcome, ToolSpec};
use mvp_kernel::{
    error::{ExecutionError, InputError, ToolError},
    kernel::Kernel,
    tool::{ToolContext, ToolImpl},
};
use mvp_service_fs::{FsBackend, HasFsService};
use mvp_service_monty::{HasMontySessionService, MontySessionStore};
use serde_json::{Map, Number, Value, json};

const DEFAULT_TOOL_NAME: &str = "monty";
const DEFAULT_OS_TOOL_NAME: &str = "monty_os";

/// Tool that runs Monty snippets and routes host-boundary calls back through
/// the tool plane.
#[derive(Clone, Debug)]
pub struct MontyTool {
    name: String,
    exposed_tools: BTreeMap<String, Value>,
    os_tool: Value,
    limits: ResourceLimits,
}

#[derive(Debug)]
pub struct MontyInput {
    code: String,
    session_id: String,
}

#[derive(Clone, Debug, Default)]
pub struct MontyOsTool;

#[derive(Debug)]
pub struct MontyOsInput {
    function: String,
    args: Vec<Value>,
    kwargs: Map<String, Value>,
}

struct MontyStepOutput {
    repl: MontyRepl<LimitedTracker>,
    value: MontyObject,
    classification: OutputClassification,
}

impl Default for MontyTool {
    fn default() -> Self {
        Self::new()
    }
}

impl MontyTool {
    #[must_use]
    pub fn new() -> Self {
        Self {
            name: DEFAULT_TOOL_NAME.into(),
            exposed_tools: BTreeMap::new(),
            os_tool: Value::String(DEFAULT_OS_TOOL_NAME.into()),
            limits: ResourceLimits::new(),
        }
    }

    #[must_use]
    pub fn named(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    #[must_use]
    pub fn expose(mut self, monty_name: impl Into<String>, tool_path: impl Into<Value>) -> Self {
        self.exposed_tools
            .insert(monty_name.into(), tool_path.into());
        self
    }

    #[must_use]
    pub fn os_tool(mut self, tool_path: impl Into<Value>) -> Self {
        self.os_tool = tool_path.into();
        self
    }

    #[must_use]
    pub fn limits(mut self, limits: ResourceLimits) -> Self {
        self.limits = limits;
        self
    }

    async fn drive<K>(
        &self,
        ctx: &K::ToolCx<'_>,
        repl: MontyRepl<LimitedTracker>,
        code: &str,
    ) -> Result<MontyStepOutput, ToolError>
    where
        K: Kernel,
    {
        let mut classification = OutputClassification::Public;
        let mut progress = repl
            .feed_start(code, Vec::new(), PrintWriter::Disabled)
            .map_err(monty_start_error_to_tool_error)?;

        loop {
            progress = match progress {
                ReplProgress::Complete { repl, value } => {
                    return Ok(MontyStepOutput {
                        repl,
                        value,
                        classification,
                    });
                }
                ReplProgress::NameLookup(lookup) => {
                    let result = if self.exposed_tools.contains_key(&lookup.name) {
                        NameLookupResult::Value(MontyObject::Function {
                            name: lookup.name.clone(),
                            docstring: None,
                        })
                    } else {
                        NameLookupResult::Undefined
                    };

                    lookup
                        .resume(result, PrintWriter::Disabled)
                        .map_err(monty_start_error_to_tool_error)?
                }
                ReplProgress::FunctionCall(call) => {
                    if call.method_call {
                        let function_name = call.function_name.clone();
                        call.resume(
                            ExtFunctionResult::NotFound(function_name),
                            PrintWriter::Disabled,
                        )
                        .map_err(monty_start_error_to_tool_error)?
                    } else {
                        let resume_result = if let Some(tool_path) =
                            self.exposed_tools.get(&call.function_name).cloned()
                        {
                            match monty_tool_payload(&call.args, &call.kwargs) {
                                Ok(payload) => match K::decode_tool_path(&tool_path)
                                    .map_err(ToolError::InvalidInput)
                                {
                                    Ok(path) => match ctx.invoke_tool(path, None, payload).await {
                                        Ok(outcome) => {
                                            merge_classification(
                                                &mut classification,
                                                outcome.classification,
                                            );
                                            match json_to_monty(outcome.payload) {
                                                Ok(value) => call.resume(
                                                    ExtFunctionResult::Return(value),
                                                    PrintWriter::Disabled,
                                                ),
                                                Err(error) => call.resume(
                                                    ExtFunctionResult::Error(tool_error_to_monty(
                                                        error,
                                                    )),
                                                    PrintWriter::Disabled,
                                                ),
                                            }
                                        }
                                        Err(error) => call.resume(
                                            ExtFunctionResult::Error(tool_error_to_monty(error)),
                                            PrintWriter::Disabled,
                                        ),
                                    },
                                    Err(error) => call.resume(
                                        ExtFunctionResult::Error(tool_error_to_monty(error)),
                                        PrintWriter::Disabled,
                                    ),
                                },
                                Err(error) => call.resume(
                                    ExtFunctionResult::Error(tool_error_to_monty(error)),
                                    PrintWriter::Disabled,
                                ),
                            }
                        } else {
                            let function_name = call.function_name.clone();
                            call.resume(
                                ExtFunctionResult::NotFound(function_name),
                                PrintWriter::Disabled,
                            )
                        };

                        resume_result.map_err(monty_start_error_to_tool_error)?
                    }
                }
                ReplProgress::OsCall(mut call) => {
                    let os_call = call.take_function_call();
                    let function_name = os_call.name().to_owned();
                    let (args, kwargs) = os_call.to_args();
                    let payload = json!({
                        "function": function_name,
                        "args": monty_list_to_json(&args)?,
                        "kwargs": monty_kwargs_to_json(&kwargs)?,
                    });

                    match K::decode_tool_path(&self.os_tool).map_err(ToolError::InvalidInput) {
                        Ok(path) => match ctx.invoke_tool(path, None, payload).await {
                            Ok(outcome) => {
                                merge_classification(&mut classification, outcome.classification);
                                match json_to_monty(outcome.payload) {
                                    Ok(value) => call.resume(
                                        ExtFunctionResult::Return(value),
                                        PrintWriter::Disabled,
                                    ),
                                    Err(error) => call.resume(
                                        ExtFunctionResult::Error(tool_error_to_monty(error)),
                                        PrintWriter::Disabled,
                                    ),
                                }
                            }
                            Err(error) => call.resume(
                                ExtFunctionResult::Error(tool_error_to_monty(error)),
                                PrintWriter::Disabled,
                            ),
                        },
                        Err(error) => call.resume(
                            ExtFunctionResult::Error(tool_error_to_monty(error)),
                            PrintWriter::Disabled,
                        ),
                    }
                    .map_err(monty_start_error_to_tool_error)?
                }
                ReplProgress::ResolveFutures(state) => {
                    return Err(execution_error(format!(
                        "async Monty futures are not wired yet; pending call ids: {:?}",
                        state.pending_call_ids()
                    )));
                }
            };
        }
    }
}

#[async_trait]
impl<K> ToolImpl<K> for MontyTool
where
    K: Kernel + MontySessionStore,
    for<'a> K::ToolCx<'a>: HasMontySessionService<K>,
{
    type Input = MontyInput;
    type Output = ToolOutcome;

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: self.name.clone(),
            description: "Run Monty code as an agent automation tool.".into(),
            capabilities: Capabilities::empty(),
        }
    }

    fn parse_input(&self, payload: Value) -> Result<Self::Input, InputError> {
        let code = payload
            .get("code")
            .and_then(Value::as_str)
            .ok_or(InputError::MissingField("code"))?
            .to_owned();
        let session_id = payload
            .get("session_id")
            .and_then(Value::as_str)
            .unwrap_or("default")
            .to_owned();

        Ok(MontyInput { code, session_id })
    }

    async fn execute(
        &self,
        ctx: &K::ToolCx<'_>,
        input: Self::Input,
    ) -> Result<Self::Output, ToolError> {
        let sessions = ctx.monty_sessions();
        let repl = match sessions
            .load(&input.session_id)
            .await
            .map_err(ToolError::Execution)?
        {
            Some(bytes) => MontyRepl::load(&bytes)
                .map_err(|err| execution_error(format!("invalid Monty session: {err}")))?,
            None => MontyRepl::new("<agent-monty>", LimitedTracker::new(self.limits.clone())),
        };

        let output = self.drive::<K>(ctx, repl, &input.code).await?;
        let session = output
            .repl
            .dump()
            .map_err(|err| execution_error(format!("failed to serialize Monty session: {err}")))?;
        sessions
            .save(&input.session_id, session)
            .await
            .map_err(ToolError::Execution)?;

        Ok(ToolOutcome {
            payload: json!({
                "value": monty_to_json(&output.value)?,
                "session_id": input.session_id,
            }),
            classification: output.classification,
        })
    }
}

#[async_trait]
impl<K> ToolImpl<K> for MontyOsTool
where
    K: Kernel + FsBackend,
    for<'a> K::ToolCx<'a>: HasFsService<K>,
{
    type Input = MontyOsInput;
    type Output = ToolOutcome;

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: DEFAULT_OS_TOOL_NAME.into(),
            description: "Handle Monty OS calls through SAP services.".into(),
            capabilities: Capabilities::empty(),
        }
    }

    fn parse_input(&self, payload: Value) -> Result<Self::Input, InputError> {
        let function = payload
            .get("function")
            .and_then(Value::as_str)
            .ok_or(InputError::MissingField("function"))?
            .to_owned();
        let args = payload
            .get("args")
            .and_then(Value::as_array)
            .ok_or(InputError::MissingField("args"))?
            .clone();
        let kwargs = payload
            .get("kwargs")
            .and_then(Value::as_object)
            .ok_or(InputError::MissingField("kwargs"))?
            .clone();

        Ok(MontyOsInput {
            function,
            args,
            kwargs,
        })
    }

    async fn execute(
        &self,
        ctx: &K::ToolCx<'_>,
        input: Self::Input,
    ) -> Result<Self::Output, ToolError> {
        match input.function.as_str() {
            "Path.read_text" => {
                ensure_no_os_kwargs(&input)?;
                let path = os_arg_path(&input, 0)?;
                let content = ctx
                    .fs()
                    .read_file(path)
                    .await
                    .map_err(ToolError::Execution)?;
                Ok(ToolOutcome {
                    payload: Value::String(content),
                    classification: OutputClassification::WorkspaceLocal,
                })
            }
            "Path.write_text" => {
                ensure_no_os_kwargs(&input)?;
                let path = os_arg_path(&input, 0)?.to_owned();
                let data = os_arg_str(&input, 1)?.to_owned();
                ctx.fs()
                    .write_file(&path, &data)
                    .await
                    .map_err(ToolError::Execution)?;
                Ok(ToolOutcome {
                    payload: Value::Number(Number::from(data.chars().count() as u64)),
                    classification: OutputClassification::Public,
                })
            }
            function => Err(execution_error(format!(
                "Monty OS call {function:?} is not supported by {DEFAULT_OS_TOOL_NAME}"
            ))),
        }
    }
}

fn monty_tool_payload(
    args: &[MontyObject],
    kwargs: &[(MontyObject, MontyObject)],
) -> Result<Value, ToolError> {
    if kwargs.is_empty()
        && args.len() == 1
        && let Value::Object(map) = monty_to_json(&args[0])?
    {
        return Ok(Value::Object(map));
    }

    Ok(json!({
        "args": monty_list_to_json(args)?,
        "kwargs": monty_kwargs_to_json(kwargs)?,
    }))
}

fn monty_list_to_json(values: &[MontyObject]) -> Result<Value, ToolError> {
    values.iter().map(monty_to_json).collect()
}

fn monty_kwargs_to_json(kwargs: &[(MontyObject, MontyObject)]) -> Result<Value, ToolError> {
    let mut object = Map::new();
    for (key, value) in kwargs {
        let key = match key {
            MontyObject::String(key) => key.clone(),
            other => {
                return Err(execution_error(format!(
                    "Monty keyword key must be str, got {other}"
                )));
            }
        };
        object.insert(key, monty_to_json(value)?);
    }
    Ok(Value::Object(object))
}

fn os_arg_path(input: &MontyOsInput, index: usize) -> Result<&str, ToolError> {
    match input.args.get(index) {
        Some(Value::String(path)) => Ok(path),
        _ => Err(execution_error(format!(
            "{} argument {index} must be a path string",
            input.function
        ))),
    }
}

fn ensure_no_os_kwargs(input: &MontyOsInput) -> Result<(), ToolError> {
    if input.kwargs.is_empty() {
        Ok(())
    } else {
        Err(execution_error(format!(
            "{} does not support keyword arguments in {DEFAULT_OS_TOOL_NAME}",
            input.function
        )))
    }
}

fn os_arg_str(input: &MontyOsInput, index: usize) -> Result<&str, ToolError> {
    match input.args.get(index) {
        Some(Value::String(value)) => Ok(value),
        _ => Err(execution_error(format!(
            "{} argument {index} must be a string",
            input.function
        ))),
    }
}

fn monty_to_json(value: &MontyObject) -> Result<Value, ToolError> {
    match value {
        MontyObject::None | MontyObject::Ellipsis => Ok(Value::Null),
        MontyObject::Bool(value) => Ok(Value::Bool(*value)),
        MontyObject::Int(value) => Ok(Value::Number(Number::from(*value))),
        MontyObject::BigInt(value) => Ok(Value::String(value.to_string())),
        MontyObject::Float(value) => Number::from_f64(*value)
            .map(Value::Number)
            .ok_or_else(|| execution_error("Monty float cannot be represented as JSON")),
        MontyObject::String(value) | MontyObject::Path(value) | MontyObject::Repr(value) => {
            Ok(Value::String(value.clone()))
        }
        MontyObject::Bytes(value) => Ok(Value::Array(
            value
                .iter()
                .map(|byte| Value::Number(Number::from(*byte)))
                .collect(),
        )),
        MontyObject::List(values)
        | MontyObject::Tuple(values)
        | MontyObject::Set(values)
        | MontyObject::FrozenSet(values) => values.iter().map(monty_to_json).collect(),
        MontyObject::Dict(pairs) => dict_pairs_to_json(pairs),
        MontyObject::Exception { exc_type, arg } => Ok(json!({
            "type": exc_type.to_string(),
            "message": arg,
        })),
        MontyObject::Function { name, docstring } => Ok(json!({
            "name": name,
            "docstring": docstring,
        })),
        other => serde_json::to_value(other)
            .map_err(|err| execution_error(format!("failed to encode Monty value as JSON: {err}"))),
    }
}

fn dict_pairs_to_json(pairs: &DictPairs) -> Result<Value, ToolError> {
    let mut object = Map::new();
    for (key, value) in pairs {
        let key = match key {
            MontyObject::String(key) => key.clone(),
            other => {
                return Err(execution_error(format!(
                    "Monty dict key must be str, got {other}"
                )));
            }
        };
        object.insert(key, monty_to_json(value)?);
    }
    Ok(Value::Object(object))
}

fn json_to_monty(value: Value) -> Result<MontyObject, ToolError> {
    match value {
        Value::Null => Ok(MontyObject::None),
        Value::Bool(value) => Ok(MontyObject::Bool(value)),
        Value::Number(value) => {
            if let Some(value) = value.as_i64() {
                Ok(MontyObject::Int(value))
            } else if let Some(value) = value.as_f64() {
                Ok(MontyObject::Float(value))
            } else {
                Err(execution_error(
                    "JSON number cannot be represented by Monty",
                ))
            }
        }
        Value::String(value) => Ok(MontyObject::String(value)),
        Value::Array(values) => values
            .into_iter()
            .map(json_to_monty)
            .collect::<Result<Vec<_>, _>>()
            .map(MontyObject::List),
        Value::Object(values) => values
            .into_iter()
            .map(|(key, value)| Ok((MontyObject::String(key), json_to_monty(value)?)))
            .collect::<Result<Vec<_>, ToolError>>()
            .map(DictPairs::from)
            .map(MontyObject::Dict),
    }
}

fn merge_classification(current: &mut OutputClassification, next: OutputClassification) {
    if classification_rank(&next) > classification_rank(current) {
        *current = next;
    }
}

fn classification_rank(classification: &OutputClassification) -> u8 {
    match classification {
        OutputClassification::Public => 0,
        OutputClassification::WorkspaceLocal => 1,
        OutputClassification::Sensitive => 2,
    }
}

fn monty_start_error_to_tool_error(error: Box<monty::ReplStartError<LimitedTracker>>) -> ToolError {
    monty_exception_to_tool_error(error.error)
}

fn monty_exception_to_tool_error(error: MontyException) -> ToolError {
    execution_error(error.summary())
}

fn monty_exception(message: impl Into<String>) -> MontyException {
    MontyException::new(ExcType::RuntimeError, Some(message.into()))
}

fn execution_error(message: impl Into<String>) -> ToolError {
    ToolError::Execution(ExecutionError::Other(message.into()))
}

fn tool_error_to_monty(error: ToolError) -> MontyException {
    monty_exception(format!("{error:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mvp_contract::{Capability, InvocationParams};
    use mvp_kernel::{
        audit,
        error::AuthorizationError,
        policy::{
            CapabilityEnvelopePolicy, KernelPolicyContext, KernelPolicyContextFactory, PolicyPlane,
        },
        tool::{RegisteredTool, ToolContext, ToolImpl, ToolRegistration},
    };
    use mvp_service_fs::{
        AllowWorkspaceFsPolicy, AllowWorkspaceReadPolicy, CanonicalRoot, FsAction, FsService,
        HasFsBackend, HasFsService, StdFsBackend,
    };
    use mvp_service_monty::{
        AllowMontySessionPolicy, HasMontySessionService, HasMontySessionStore,
        MemoryMontySessionStore, MontySessionLoadAction, MontySessionSaveAction,
        MontySessionService,
    };
    use mvp_test_support::TempWorkspace;
    use std::collections::BTreeMap;
    use std::path::Path;

    struct TestKernel {
        tools: BTreeMap<String, RegisteredTool<TestKernel>>,
        fs: StdFsBackend,
        monty_sessions: MemoryMontySessionStore,
        policy: PolicyPlane<KernelPolicyContextFactory>,
    }

    impl TestKernel {
        fn new() -> Self {
            let mut policy = PolicyPlane::new();
            policy.prepend_inbound(CapabilityEnvelopePolicy);
            policy.append::<FsAction, _>(AllowWorkspaceFsPolicy);
            policy.append::<MontySessionLoadAction, _>(AllowMontySessionPolicy);
            policy.append::<MontySessionSaveAction, _>(AllowMontySessionPolicy);

            Self {
                tools: BTreeMap::new(),
                fs: StdFsBackend,
                monty_sessions: MemoryMontySessionStore::new(),
                policy,
            }
        }

        fn register<T: ToolImpl<Self>>(
            &mut self,
            path: <Self as Kernel>::ToolPath,
            tool: T,
        ) -> Result<(), ToolError> {
            if self.tools.contains_key(&path) {
                return Err(ToolError::DuplicateTool(format!("{path:?}")));
            }

            let registered = RegisteredTool::from_tool(tool)?;
            self.tools.insert(path, registered);
            Ok(())
        }
    }

    impl HasFsBackend for TestKernel {
        type FsBackend = StdFsBackend;

        fn fs_backend(&self) -> &Self::FsBackend {
            &self.fs
        }
    }

    impl HasMontySessionStore for TestKernel {
        type MontySessionStore = MemoryMontySessionStore;

        fn monty_session_store(&self) -> &Self::MontySessionStore {
            &self.monty_sessions
        }
    }

    struct TestToolContext<'a> {
        kernel: &'a TestKernel,
        tool_path: &'a <TestKernel as Kernel>::ToolPath,
        registration: &'a ToolRegistration,
        effective_capabilities: Capabilities,
        canonical_workspace_root: CanonicalRoot,
    }

    impl<'a> TestToolContext<'a> {
        fn new(
            kernel: &'a TestKernel,
            tool_path: &'a <TestKernel as Kernel>::ToolPath,
            registration: &'a ToolRegistration,
            params: &'a InvocationParams,
        ) -> Result<Self, AuthorizationError> {
            let canonical_workspace_root = CanonicalRoot::existing(&params.workspace_root)?;
            let declared_capabilities = registration.spec().capabilities;
            let effective_capabilities = match params.capabilities_override {
                Some(caps) => caps,
                None => declared_capabilities,
            };

            Ok(Self {
                kernel,
                tool_path,
                registration,
                effective_capabilities,
                canonical_workspace_root,
            })
        }
    }

    #[async_trait]
    impl ToolContext<TestKernel> for TestToolContext<'_> {
        fn policy_context(&self) -> KernelPolicyContext<'_> {
            KernelPolicyContext::new(
                self.effective_capabilities,
                self.canonical_workspace_root.as_path(),
            )
        }

        fn effective_capabilities(&self) -> Capabilities {
            self.effective_capabilities
        }

        fn tool_path(&self) -> &<TestKernel as Kernel>::ToolPath {
            self.tool_path
        }

        fn registration(&self) -> &ToolRegistration {
            self.registration
        }

        fn workspace_root(&self) -> &Path {
            self.canonical_workspace_root.as_path()
        }

        async fn invoke_tool(
            &self,
            path: <TestKernel as Kernel>::ToolPath,
            capabilities_override: Option<Capabilities>,
            payload: Value,
        ) -> Result<ToolOutcome, ToolError> {
            let (effective_capabilities, attempted_expand) = match capabilities_override {
                Some(capabilities) => {
                    let attempted_expand = !self.effective_capabilities.contains(capabilities);
                    if attempted_expand {
                        audit::record_nested_capability_override(
                            self.tool_path,
                            self.registration,
                            &path,
                            self.effective_capabilities,
                            Some(capabilities),
                            None,
                            true,
                        );
                        return Err(ToolError::Authorization(AuthorizationError::Denied(
                            "nested invocation attempted to expand capabilities".into(),
                        )));
                    }
                    (capabilities, false)
                }
                None => (self.effective_capabilities, false),
            };

            audit::record_nested_capability_override(
                self.tool_path,
                self.registration,
                &path,
                self.effective_capabilities,
                capabilities_override,
                Some(effective_capabilities),
                attempted_expand,
            );

            let params = InvocationParams::new(self.workspace_root(), Some(effective_capabilities));
            self.kernel.invoke(&path, &params, payload).await
        }
    }

    impl HasFsService<TestKernel> for TestToolContext<'_> {
        fn fs(&self) -> FsService<'_, TestKernel> {
            FsService::new(self.kernel, self.workspace_root(), self.policy_context())
        }
    }

    impl HasMontySessionService<TestKernel> for TestToolContext<'_> {
        fn monty_sessions(&self) -> MontySessionService<'_, TestKernel> {
            MontySessionService::new(self.kernel, self.workspace_root(), self.policy_context())
        }
    }

    #[async_trait]
    impl Kernel for TestKernel {
        type PolicyCxFactory = KernelPolicyContextFactory;
        type PolicyPlane<'a>
            = PolicyPlane<KernelPolicyContextFactory>
        where
            Self: 'a;

        type ToolPath = String;
        type ToolCx<'a>
            = TestToolContext<'a>
        where
            Self: 'a;

        fn policy_plane(&self) -> &Self::PolicyPlane<'_> {
            &self.policy
        }

        fn decode_tool_path(value: &Value) -> Result<Self::ToolPath, InputError> {
            value
                .as_str()
                .map(ToOwned::to_owned)
                .ok_or(InputError::InvalidField("tool_path"))
        }

        async fn invoke(
            &self,
            path: &Self::ToolPath,
            params: &InvocationParams,
            payload: Value,
        ) -> Result<ToolOutcome, ToolError> {
            let (registered_path, registered) = self
                .tools
                .get_key_value(path)
                .ok_or_else(|| ToolError::UnknownTool(format!("{path:?}")))?;
            let ctx =
                TestToolContext::new(self, registered_path, registered.registration(), params)
                    .map_err(ToolError::Authorization)?;
            registered.invoke(&ctx, payload).await
        }
    }

    struct EchoTool;

    #[async_trait]
    impl<K> ToolImpl<K> for EchoTool
    where
        K: Kernel,
    {
        type Input = Value;
        type Output = ToolOutcome;

        fn spec(&self) -> ToolSpec {
            ToolSpec {
                name: "echo".into(),
                description: "Echo input.".into(),
                capabilities: Capabilities::empty(),
            }
        }

        fn parse_input(&self, payload: Value) -> Result<Self::Input, InputError> {
            Ok(payload)
        }

        async fn execute(
            &self,
            _ctx: &K::ToolCx<'_>,
            input: Self::Input,
        ) -> Result<Self::Output, ToolError> {
            Ok(ToolOutcome {
                payload: input,
                classification: OutputClassification::WorkspaceLocal,
            })
        }
    }

    #[tokio::test]
    async fn monty_function_call_invokes_exposed_tool() {
        let mut kernel = TestKernel::new();
        kernel
            .register(
                DEFAULT_TOOL_NAME.to_owned(),
                MontyTool::new().expose("echo", "echo"),
            )
            .unwrap();
        kernel.register("echo".to_owned(), EchoTool).unwrap();

        let params = InvocationParams::new(std::env::temp_dir(), Some([Capability::FsRead].into()));
        let outcome = mvp_kernel::kernel::Kernel::invoke(
            &kernel,
            &DEFAULT_TOOL_NAME.to_string(),
            &params,
            json!({
                "code": "echo({'message': 'hello from monty'})",
            }),
        )
        .await
        .unwrap();

        assert_eq!(outcome.payload["value"]["message"], "hello from monty");
        assert_eq!(outcome.payload["session_id"], "default");
        assert_eq!(outcome.classification, OutputClassification::WorkspaceLocal);
    }

    #[tokio::test]
    async fn monty_session_store_preserves_repl_state() {
        let mut kernel = TestKernel::new();
        kernel
            .register(DEFAULT_TOOL_NAME.to_owned(), MontyTool::new())
            .unwrap();

        let params = InvocationParams::new(std::env::temp_dir(), None);
        let first = mvp_kernel::kernel::Kernel::invoke(
            &kernel,
            &DEFAULT_TOOL_NAME.to_string(),
            &params,
            json!({
                "session_id": "agent-main",
                "code": "x = 41",
            }),
        )
        .await
        .unwrap();
        assert_eq!(first.payload["session_id"], "agent-main");

        let second = mvp_kernel::kernel::Kernel::invoke(
            &kernel,
            &DEFAULT_TOOL_NAME.to_string(),
            &params,
            json!({
                "session_id": "agent-main",
                "code": "x + 1",
            }),
        )
        .await
        .unwrap();

        assert_eq!(second.payload["value"], 42);
    }

    #[tokio::test]
    async fn monty_os_call_invokes_os_tool_through_fs_service() {
        let ws = TempWorkspace::with_prefix("monty-os-read-text");
        std::fs::write(ws.root.join("hello.txt"), "hello through os call").unwrap();

        let mut kernel = TestKernel::new();
        kernel
            .register(DEFAULT_TOOL_NAME.to_owned(), MontyTool::new())
            .unwrap();
        kernel
            .register(DEFAULT_OS_TOOL_NAME.to_owned(), MontyOsTool)
            .unwrap();
        kernel.policy.append(AllowWorkspaceFsPolicy);
        kernel.policy.append(AllowWorkspaceReadPolicy);

        let params = InvocationParams::new(&ws.root, Some([Capability::FsRead].into()));
        let outcome = mvp_kernel::kernel::Kernel::invoke(
            &kernel,
            &DEFAULT_TOOL_NAME.to_string(),
            &params,
            json!({
                "code": "from pathlib import Path\nPath('hello.txt').read_text()",
            }),
        )
        .await
        .unwrap();

        assert_eq!(outcome.payload["value"], "hello through os call");
        assert_eq!(outcome.classification, OutputClassification::WorkspaceLocal);
    }
}
