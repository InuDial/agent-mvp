use anymap::AnyMap;
use async_trait::async_trait;
use std::{any::Any, collections::VecDeque, marker::PhantomData};

use crate::action::Action;

#[derive(Clone, Debug)]
pub enum Decision {
    Allow(Option<String>),
    Deny(Option<String>),
    Abstain,
    // Maybe here should be a `Ask` Decision, the composite behaviour may
    // be similar to JavaScript `null` and `undefined`
    //
    // Now simply Allow = Deny > Abstain
}

impl Decision {
    /// Combine two policy decisions.
    ///
    /// The current combination rule is intentionally simple and conservative:
    ///
    /// - `Deny` dominates everything
    /// - otherwise `Allow` dominates `Abstain`
    /// - `Abstain` means “this rule does not apply”
    pub fn combine(self, other: Self) -> Self {
        use Decision::*;

        match (self, other) {
            (Deny(lhs), Deny(rhs)) => Deny(lhs.or(rhs)),
            (Deny(reason), _) | (_, Deny(reason)) => Deny(reason),
            (Allow(lhs), Allow(rhs)) => Allow(lhs.or(rhs)),
            (Allow(reason), Abstain) | (Abstain, Allow(reason)) => Allow(reason),
            (Abstain, Abstain) => Abstain,
        }
    }
}

/// Marker trait for types that may be used as policy evaluation context.
///
/// This is intentionally small: the concrete context shape is decided by the
/// app/kernel integration layer, but it must at least be shareable and have a
/// stable type so it can participate in typed policy buckets.
pub trait PolicyContext: 'static + Send + Sync {}

#[async_trait]
pub trait Policy<Ctx: PolicyContext, A: Action>: Send + Sync {
    /// Give an opinion for an action.
    ///
    /// Policies are combined later by the `PolicyPlane`, so an implementation
    /// should return `Abstain` when it does not apply.
    async fn grant(&self, ctx: &Ctx, action: &A) -> Decision;
}

#[async_trait]
pub trait PolicyAny<Ctx: PolicyContext>: Send + Sync {
    async fn grant(&self, ctx: &Ctx, action: &dyn Action) -> Decision;
}

pub struct PolicyAnyWrapper<Ctx: PolicyContext, A: Action, P: Policy<Ctx, A>> {
    inner: P,
    _phantom_data: PhantomData<(Ctx, A)>,
}

impl<Ctx, A, P> PolicyAnyWrapper<Ctx, A, P>
where
    Ctx: PolicyContext,
    A: Action,
    P: Policy<Ctx, A>,
{
    pub fn new(inner: P) -> Self {
        Self {
            inner,
            _phantom_data: PhantomData,
        }
    }
}

#[async_trait]
impl<Ctx, A, P> PolicyAny<Ctx> for PolicyAnyWrapper<Ctx, A, P>
where
    Ctx: PolicyContext,
    A: Action,
    P: Policy<Ctx, A>,
{
    async fn grant(&self, ctx: &Ctx, action: &dyn Action) -> Decision {
        let action_any = action as &dyn Any;
        match action_any.downcast_ref::<A>() {
            Some(action) => self.inner.grant(ctx, action).await,
            None => Decision::Abstain,
        }
    }
}

/// A typed policy plane.
///
/// Policies are stored in per-action-type buckets, while global policies are
/// evaluated for every action. This keeps the common path type-directed, but
/// still allows cross-cutting rules to exist.
pub struct PolicyPlane<Ctx: PolicyContext> {
    policy_entries: AnyMap,
    global_policies: VecDeque<Box<dyn PolicyAny<Ctx>>>,
}

impl<Ctx: PolicyContext> Default for PolicyPlane<Ctx> {
    fn default() -> Self {
        Self::new()
    }
}

impl<Ctx: PolicyContext> PolicyPlane<Ctx> {
    pub fn new() -> Self {
        Self {
            policy_entries: AnyMap::new(),
            global_policies: VecDeque::new(),
        }
    }

    fn get_mut_or_default<A>(&mut self) -> &mut VecDeque<Box<dyn Policy<Ctx, A>>>
    where
        A: Action,
    {
        self.policy_entries.entry().or_insert_with(VecDeque::new)
    }

    /// Register a typed policy at the front of a specific action bucket.
    pub fn prepend<A, P>(&mut self, policy: P)
    where
        A: Action,
        P: Policy<Ctx, A> + 'static,
    {
        self.get_mut_or_default().push_front(Box::new(policy));
    }

    /// Register a typed policy at the back of a specific action bucket.
    pub fn append<A, P>(&mut self, policy: P)
    where
        A: Action,
        P: Policy<Ctx, A> + 'static,
    {
        self.get_mut_or_default().push_back(Box::new(policy));
    }

    /// Register a global policy at the front of the global policy list.
    pub fn prepend_global<P>(&mut self, policy: P)
    where
        P: PolicyAny<Ctx> + 'static,
    {
        self.global_policies.push_front(Box::new(policy));
    }

    /// Register a global policy at the back of the global policy list.
    pub fn append_global<P>(&mut self, policy: P)
    where
        P: PolicyAny<Ctx> + 'static,
    {
        self.global_policies.push_back(Box::new(policy));
    }

    /// Evaluate policies for a typed action.
    pub async fn grant<A>(&self, ctx: &Ctx, action: &A) -> Decision
    where
        A: Action,
    {
        let mut decision = Decision::Abstain;

        if let Some(entries) = self
            .policy_entries
            .get::<VecDeque<Box<dyn Policy<Ctx, A>>>>()
        {
            for policy in entries {
                decision = decision.combine(policy.grant(ctx, action).await);
            }
        }

        for policy in &self.global_policies {
            decision = decision.combine(policy.grant(ctx, action).await);
        }

        decision
    }
}
