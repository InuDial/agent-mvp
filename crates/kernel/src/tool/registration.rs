use super::adapter::ToolAdapter;
use crate::error::ToolError;
use crate::tool::ToolPlaneContext;
use mvp_contract::{ToolOutcome, ToolRequest, ToolSpec};

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
    pub(crate) fn new(spec: ToolSpec) -> Result<Self, ToolError> {
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
pub struct RegisteredTool {
    registration: ToolRegistration,
    adapter: Box<dyn ToolAdapter>,
}

impl RegisteredTool {
    pub(crate) fn new(registration: ToolRegistration, adapter: Box<dyn ToolAdapter>) -> Self {
        Self {
            registration,
            adapter,
        }
    }

    pub fn registration(&self) -> &ToolRegistration {
        &self.registration
    }

    pub fn spec(&self) -> &ToolSpec {
        self.registration.spec()
    }

    pub(crate) async fn invoke(
        &self,
        ctx: &ToolPlaneContext<'_>,
        req: ToolRequest,
    ) -> Result<ToolOutcome, ToolError> {
        self.adapter.invoke(ctx, req).await
    }
}
