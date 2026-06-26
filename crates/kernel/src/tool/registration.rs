use super::adapter::{KernelToolAdapter, ToolAdapter};
use crate::error::ToolError;
use crate::kernel::Kernel;
use crate::tool::ToolImpl;
use mvp_contract::{ToolOutcome, ToolSpec};
use serde_json::Value;

/// Kernel-owned metadata attached when a `ToolImpl` is accepted by `ToolPlane`.
///
/// `ToolSpec` is what an implementation declares. `ToolRegistration` is the
/// kernel's registered identity for that tool and is passed into service
/// sub-contexts so they can check declared capabilities and emit audit records.
pub struct ToolRegistration {
    pub(crate) spec: ToolSpec,
    registered_at: std::time::SystemTime,
}

impl ToolRegistration {
    pub fn new(spec: ToolSpec) -> Result<Self, ToolError> {
        if spec.name.is_empty() {
            return Err(ToolError::InvalidSpec);
        }

        Ok(Self {
            spec,
            registered_at: std::time::SystemTime::now(),
        })
    }

    pub fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    pub fn registered_at(&self) -> std::time::SystemTime {
        self.registered_at
    }
}

/// One registered entry: registration metadata plus its runtime adapter.
pub struct RegisteredTool<K: Kernel> {
    registration: ToolRegistration,
    adapter: Box<dyn ToolAdapter<K>>,
}

impl<K: Kernel> RegisteredTool<K> {
    pub(crate) fn new(registration: ToolRegistration, adapter: Box<dyn ToolAdapter<K>>) -> Self {
        Self {
            registration,
            adapter,
        }
    }

    pub fn from_tool<T: ToolImpl<K>>(tool: T) -> Result<Self, ToolError>
    where
        K: 'static,
    {
        let registration = ToolRegistration::new(tool.spec())?;
        let adapter = Box::new(KernelToolAdapter::new(tool));
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
        ctx: &K::ToolCx<'_>,
        payload: Value,
    ) -> Result<ToolOutcome, ToolError> {
        self.adapter.invoke(ctx, payload).await
    }
}
