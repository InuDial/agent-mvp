use std::{any::Any, marker::PhantomData};

use async_trait::async_trait;
use mvp_contract::PolicyGrant;
use mvp_core::{
    action::Action,
    policy::{Policy, PolicyAny, PolicyContextFactory},
};

pub struct PolicyAnyWrapper<F: PolicyContextFactory, A: Action, P: Policy<F, A>> {
    inner: P,
    _phantom_data: PhantomData<(fn() -> F, A)>,
}

impl<F, A, P> PolicyAnyWrapper<F, A, P>
where
    F: PolicyContextFactory,
    A: Action,
    P: Policy<F, A>,
{
    pub fn new(inner: P) -> Self {
        Self {
            inner,
            _phantom_data: PhantomData,
        }
    }
}

#[async_trait]
impl<F, A, P> PolicyAny<F> for PolicyAnyWrapper<F, A, P>
where
    F: PolicyContextFactory,
    A: Action,
    P: Policy<F, A>,
{
    fn name(&self) -> &'static str {
        self.inner.name()
    }

    async fn grant(&self, ctx: &F::Context<'_>, action: &dyn Action) -> PolicyGrant {
        let action_any = action as &dyn Any;
        match action_any.downcast_ref::<A>() {
            Some(action) => self.inner.grant(ctx, action).await,
            None => PolicyGrant::abstain(Some("policy does not apply to this action type".into()))
                .with_predicate(format!(
                    "action downcasts to policy action type: {}",
                    std::any::type_name::<A>()
                )),
        }
    }
}
