use async_trait::async_trait;
use mvp_contract::{
    AuditResource, Capabilities, PolicyDecision, PolicyEvaluation, PolicyGrant, PolicyReport,
};
use mvp_core::action::{Action, ActionExecutor};
use mvp_core::error::ExecutionError;
use mvp_core::policy::{Granted, Policy, PolicyEngine};
use mvp_kernel::policy::{KernelPolicyContext, KernelPolicyContextFactory};

struct ExternalEchoAccess<'a, E> {
    policy: &'a E,
    executor: &'a ExternalEchoExecutor,
}

impl<E> ExternalEchoAccess<'_, E>
where
    E: PolicyEngine<KernelPolicyContextFactory>,
{
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

struct ExternalEchoPolicyEngine;

#[async_trait]
impl PolicyEngine<KernelPolicyContextFactory> for ExternalEchoPolicyEngine {
    async fn decide<A: Action>(&self, ctx: &KernelPolicyContext<'_>, action: &A) -> PolicyReport {
        let action_any = action as &dyn std::any::Any;
        let Some(action) = action_any.downcast_ref::<ExternalEchoAction>() else {
            return PolicyReport::deny_without_match(
                Vec::new(),
                Some("No matching policy.".to_owned()),
            );
        };

        let policy = AllowExternalEcho;
        let policy_grant = policy.grant(ctx, action).await;
        let (decision, reason) = policy_grant.clone().into_decision_and_reason();
        let evaluations = vec![PolicyEvaluation::new(
            policy.name(),
            1,
            "action",
            policy_grant,
        )];

        match decision {
            PolicyDecision::Allow => PolicyReport::allow(evaluations, policy.name(), 1, reason),
            PolicyDecision::Deny => {
                PolicyReport::deny_from_policy(evaluations, policy.name(), 1, reason)
            }
            PolicyDecision::Abstain => PolicyReport::deny_without_match(
                evaluations,
                Some("No matching policy.".to_owned()),
            ),
        }
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
async fn external_crate_can_define_access_action_and_executor() {
    let root = std::fs::canonicalize(std::env::current_dir().unwrap()).unwrap();
    let ctx = KernelPolicyContext::new(Capabilities::empty(), &root);

    let policy = ExternalEchoPolicyEngine;

    let executor = ExternalEchoExecutor;
    let access = ExternalEchoAccess {
        policy: &policy,
        executor: &executor,
    };

    let output = access.echo(&ctx, "hello").await.unwrap();

    assert_eq!(output, "external:hello");
}
