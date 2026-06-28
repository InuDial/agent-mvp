use std::time::SystemTime;

use mvp_contract::{ToolOutcome, ToolSpec};
use serde_json::Value;

use crate::{
    error::ToolError,
    tool::{
        ToolHost, ToolImpl,
        adapter::{CoreToolAdapter, ToolAdapter},
    },
};

pub struct ToolRegistration {
    pub spec: ToolSpec,
    registered_at: SystemTime,
}

impl ToolRegistration {
    pub fn new(spec: ToolSpec) -> Result<Self, ToolError> {
        if spec.name.is_empty() {
            return Err(ToolError::InvalidSpec);
        }

        Ok(Self {
            spec,
            registered_at: SystemTime::now(),
        })
    }

    pub fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    pub fn registered_at(&self) -> SystemTime {
        self.registered_at
    }
}

pub struct RegisteredTool<H: ToolHost> {
    registration: ToolRegistration,
    adapter: Box<dyn ToolAdapter<H>>,
}

impl<H: ToolHost> RegisteredTool<H> {
    pub(crate) fn new(registration: ToolRegistration, adapter: Box<dyn ToolAdapter<H>>) -> Self {
        Self {
            registration,
            adapter,
        }
    }

    pub fn from_tool<T: ToolImpl<H>>(tool: T) -> Result<Self, ToolError>
    where
        H: 'static,
    {
        let registration = ToolRegistration::new(tool.spec())?;
        let adapter = Box::new(CoreToolAdapter::new(tool));
        Ok(Self::new(registration, adapter))
    }

    pub fn registration(&self) -> &ToolRegistration {
        &self.registration
    }

    pub fn spec(&self) -> &ToolSpec {
        self.registration.spec()
    }

    pub async fn invoke(
        &self,
        ctx: &H::ToolCx<'_>,
        payload: Value,
    ) -> Result<ToolOutcome, ToolError> {
        self.adapter.invoke(ctx, payload).await
    }
}
