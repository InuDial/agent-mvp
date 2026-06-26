use std::future::Future;
use std::pin::Pin;

use mvp_contract::Capabilities;
use mvp_kernel::action::{Action, AuditResource, ExecutableAction};
use mvp_kernel::error::ExecutionError;
use mvp_kernel::policy::{
    Granted, KernelPolicyContext, KernelPolicyContextFactory, Policy, PolicyEngine, PolicyGrant,
    PolicyPlane,
};
use mvp_kernel::service::fs::CanonicalRoot;

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

        granted.execute(self.executor).await
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

impl ExecutableAction for ExternalEchoAction {
    type Executor<'a>
        = ExternalEchoExecutor
    where
        Self: 'a;
    type Output = String;

    fn execute<'a>(
        executor: &'a Self::Executor<'a>,
        granted: Granted<Self>,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Output, ExecutionError>> + Send + 'a>>
    where
        Self: 'a,
    {
        Box::pin(async move {
            let action = granted.into_action();
            Ok(executor.echo(&action.value))
        })
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
    let root = CanonicalRoot::existing(std::env::current_dir().unwrap()).unwrap();
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
