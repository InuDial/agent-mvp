use super::adapter::ToolAdapter;
use crate::error::ToolError;
use crate::tool::{RegisteredTool, ToolRegistration};
use mvp_contract::ToolName;
use std::collections::BTreeMap;

/// Registry of all registered tools.
///
/// The registry keeps tool metadata and runtime adapter together so registration
/// data is inspectable without reaching into adapter implementation details.
pub struct ToolRegistry {
    tools: BTreeMap<ToolName, RegisteredTool>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: BTreeMap::new(),
        }
    }

    pub(crate) fn insert(
        &mut self,
        registration: ToolRegistration,
        adapter: Box<dyn ToolAdapter>,
    ) -> Result<(), ToolError> {
        let name = registration.spec.name.clone();

        if self.tools.contains_key(&name) {
            return Err(ToolError::DuplicateTool(name));
        }

        self.tools
            .insert(name, RegisteredTool::new(registration, adapter));
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&RegisteredTool> {
        self.tools.get(name)
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}
