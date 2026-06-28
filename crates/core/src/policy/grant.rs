use crate::{action::Action, error::ExecutionError};
use mvp_contract::{GrantId, GrantRecord};

pub struct Granted<A> {
    record: GrantRecord,
    action: A,
}

impl<A> Granted<A> {
    pub(crate) fn new(record: GrantRecord, action: A) -> Self {
        Self { record, action }
    }

    pub fn grant_id(&self) -> GrantId {
        self.record
            .grant_id()
            .expect("Granted values are always backed by an allow record")
    }

    pub fn record(&self) -> &GrantRecord {
        &self.record
    }

    pub fn action(&self) -> &A {
        &self.action
    }

    pub fn into_action(self) -> A {
        self.action
    }
}

impl<A> Granted<A>
where
    A: Action,
{
    pub async fn execute_with<E>(self, executor: &E) -> Result<E::Output, ExecutionError>
    where
        E: crate::action::ActionExecutor<A> + ?Sized,
    {
        executor.execute(self).await
    }
}
