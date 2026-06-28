use std::collections::VecDeque;

use anymap::{Map as AnyMap, any::Any as AnyMapAny};
use mvp_contract::PolicyId;
use mvp_core::{
    action::Action,
    policy::{Policy, PolicyAny, PolicyContextFactory},
};

type SyncAnyMap = AnyMap<dyn AnyMapAny + Send + Sync>;

pub(super) struct RegisteredPolicy<F: PolicyContextFactory, A: Action> {
    pub(super) id: PolicyId,
    pub(super) inner: Box<dyn Policy<F, A>>,
}

pub(super) struct RegisteredPolicyAny<F: PolicyContextFactory> {
    pub(super) id: PolicyId,
    pub(super) inner: Box<dyn PolicyAny<F>>,
}

pub(super) struct PolicyRegistry<F: PolicyContextFactory> {
    next_policy_id: PolicyId,
    policy_entries: SyncAnyMap,
    global_policies_inbound: VecDeque<RegisteredPolicyAny<F>>,
    global_policies_outbound: VecDeque<RegisteredPolicyAny<F>>,
}

impl<F: PolicyContextFactory> PolicyRegistry<F> {
    pub(super) fn new() -> Self {
        Self {
            next_policy_id: 1,
            policy_entries: SyncAnyMap::new(),
            global_policies_inbound: VecDeque::new(),
            global_policies_outbound: VecDeque::new(),
        }
    }

    pub(super) fn action_policies<A>(&self) -> Option<&VecDeque<RegisteredPolicy<F, A>>>
    where
        A: Action,
    {
        self.policy_entries
            .get::<VecDeque<RegisteredPolicy<F, A>>>()
    }

    pub(super) fn inbound_policies(&self) -> impl Iterator<Item = &RegisteredPolicyAny<F>> {
        self.global_policies_inbound.iter()
    }

    pub(super) fn outbound_policies(&self) -> impl Iterator<Item = &RegisteredPolicyAny<F>> {
        self.global_policies_outbound.iter()
    }

    pub(super) fn prepend_action<A, P>(&mut self, policy: P)
    where
        A: Action,
        P: Policy<F, A> + 'static,
    {
        let registered = self.registered_action_policy(policy);
        self.get_mut_or_default::<A>().push_front(registered);
    }

    pub(super) fn append_action<A, P>(&mut self, policy: P)
    where
        A: Action,
        P: Policy<F, A> + 'static,
    {
        let registered = self.registered_action_policy(policy);
        self.get_mut_or_default::<A>().push_back(registered);
    }

    pub(super) fn prepend_inbound<P>(&mut self, policy: P)
    where
        P: PolicyAny<F> + 'static,
    {
        let policy_id = self.allocate_policy_id();
        self.global_policies_inbound
            .push_front(RegisteredPolicyAny {
                id: policy_id,
                inner: Box::new(policy),
            });
    }

    pub(super) fn append_inbound<P>(&mut self, policy: P)
    where
        P: PolicyAny<F> + 'static,
    {
        let policy_id = self.allocate_policy_id();
        self.global_policies_inbound.push_back(RegisteredPolicyAny {
            id: policy_id,
            inner: Box::new(policy),
        });
    }

    pub(super) fn prepend_outbound<P>(&mut self, policy: P)
    where
        P: PolicyAny<F> + 'static,
    {
        let policy_id = self.allocate_policy_id();
        self.global_policies_outbound
            .push_front(RegisteredPolicyAny {
                id: policy_id,
                inner: Box::new(policy),
            });
    }

    pub(super) fn append_outbound<P>(&mut self, policy: P)
    where
        P: PolicyAny<F> + 'static,
    {
        let policy_id = self.allocate_policy_id();
        self.global_policies_outbound
            .push_back(RegisteredPolicyAny {
                id: policy_id,
                inner: Box::new(policy),
            });
    }

    fn allocate_policy_id(&mut self) -> PolicyId {
        let id = self.next_policy_id;
        self.next_policy_id += 1;
        id
    }

    fn get_mut_or_default<A>(&mut self) -> &mut VecDeque<RegisteredPolicy<F, A>>
    where
        A: Action,
    {
        self.policy_entries.entry().or_insert_with(VecDeque::new)
    }

    fn registered_action_policy<A, P>(&mut self, policy: P) -> RegisteredPolicy<F, A>
    where
        A: Action,
        P: Policy<F, A> + 'static,
    {
        RegisteredPolicy {
            id: self.allocate_policy_id(),
            inner: Box::new(policy),
        }
    }
}
