use async_trait::async_trait;
use mvp_contract::Capabilities;
use mvp_kernel::action::{Action, ActionExecutor, AuditResource};
use mvp_kernel::error::ExecutionError;
use mvp_kernel::policy::{
    Granted, KernelPolicyContext, KernelPolicyContextFactory, Policy, PolicyEngine, PolicyGrant,
    PolicyPlane,
};

struct ExternalEchoService<'a> {
    policy: &'a PolicyPlane<KernelPolicyContextFactory>,
    executor: &'a ExternalEchoExecutor,
}

impl ExternalEchoService<'_> {
    async fn echo(
        &self,
        ctx: &KernelPolicyContext<'_>,
        value: &str,
    ) -> Result<String, ExecutionError> {
        let action = ExternalEchoAction {
            value: value.to_owned(),
        };
        let granted = self
            .policy
            .grant(ctx, action)
            .await
            .map_err(ExecutionError::Authorization)?;

        granted.execute_with(self.executor).await
    }
}

struct ExternalEchoAction {
    value: String,
}

impl Action for ExternalEchoAction {
    fn capabilities(&self) -> Capabilities {
        Capabilities::empty()
    }

    fn audit_kind(&self) -> &'static str {
        "external.echo"
    }

    fn audit_resource(&self) -> AuditResource {
        AuditResource::Value(self.value.clone())
    }
}

struct ExternalEchoExecutor;

impl ExternalEchoExecutor {
    fn echo(&self, value: &str) -> String {
        format!("external:{value}")
    }
}

#[async_trait]
impl ActionExecutor<ExternalEchoAction> for ExternalEchoExecutor {
    type Output = String;

    async fn execute(
        &self,
        granted: Granted<ExternalEchoAction>,
    ) -> Result<Self::Output, ExecutionError> {
        let action = granted.into_action();
        Ok(self.echo(&action.value))
    }
}

struct AllowExternalEcho;

#[async_trait::async_trait]
impl Policy<KernelPolicyContextFactory, ExternalEchoAction> for AllowExternalEcho {
    fn name(&self) -> &'static str {
        "external.allow_echo"
    }

    async fn grant(
        &self,
        _ctx: &KernelPolicyContext<'_>,
        action: &ExternalEchoAction,
    ) -> PolicyGrant {
        PolicyGrant::allow(Some("external echo action allowed".into()))
            .with_predicate(format!("value is non-empty: {}", !action.value.is_empty()))
    }
}

#[tokio::test]
async fn external_crate_can_define_service_action_and_executor() {
    let root = std::fs::canonicalize(std::env::current_dir().unwrap()).unwrap();
    let ctx = KernelPolicyContext::new(Capabilities::empty(), &root);

    let mut policy = PolicyPlane::new();
    policy.append::<ExternalEchoAction, _>(AllowExternalEcho);

    let executor = ExternalEchoExecutor;
    let service = ExternalEchoService {
        policy: &policy,
        executor: &executor,
    };

    let output = service.echo(&ctx, "hello").await.unwrap();

    assert_eq!(output, "external:hello");
}
