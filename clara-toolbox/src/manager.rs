// ToolboxManager: Registry and execution engine for tools

use crate::tool::{Tool, ToolError, ToolRequest, ToolResponse};
use crate::tools::EchoTool;
use lazy_static::lazy_static;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// ToolboxManager manages the registry of available tools and routes execution
pub struct ToolboxManager {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolboxManager {
    /// Create a new empty ToolboxManager
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Register a tool
    pub fn register_tool(&mut self, tool: Arc<dyn Tool>) {
        let name = tool.name().to_string();
        log::info!("Registering tool: {}", name);
        self.tools.insert(name, tool);
    }

    /// Execute a tool by name with the given arguments
    pub fn execute_tool(&self, request: &ToolRequest) -> Result<ToolResponse, ToolError> {
        log::debug!("Executing tool: {} with args: {}", request.tool, request.arguments);

        let tool = self
            .tools
            .get(&request.tool)
            .ok_or_else(|| ToolError::NotFound(request.tool.clone()))?;

        match tool.execute(request.arguments.clone()) {
            Ok(result) => {
                log::debug!("Tool {} succeeded", request.tool);
                Ok(ToolResponse::success(result))
            }
            Err(e) => {
                log::error!("Tool {} failed: {}", request.tool, e);
                Ok(ToolResponse::error(format!("{}", e)))
            }
        }
    }

    /// List all registered tool names
    pub fn list_tools(&self) -> Vec<String> {
        self.tools.keys().cloned().collect()
    }

    /// Get access to the global ToolboxManager instance
    pub fn global() -> &'static Mutex<ToolboxManager> {
        &GLOBAL_TOOLBOX
    }

    /// Initialize the global ToolboxManager with default tools
    pub fn init_global() {
        log::info!("Initializing global ToolboxManager");
        let mut mgr = GLOBAL_TOOLBOX.lock().unwrap();

        // Register default tools
        mgr.register_tool(Arc::new(EchoTool));

        log::info!("Global ToolboxManager initialized with {} tools", mgr.tools.len());
    }
}

impl Default for ToolboxManager {
    fn default() -> Self {
        Self::new()
    }
}

// Global singleton ToolboxManager
lazy_static! {
    static ref GLOBAL_TOOLBOX: Mutex<ToolboxManager> = Mutex::new(ToolboxManager::new());
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_toolbox_manager_create() {
        let mgr = ToolboxManager::new();
        assert_eq!(mgr.list_tools().len(), 0);
    }

    #[test]
    fn test_toolbox_manager_register() {
        let mut mgr = ToolboxManager::new();
        mgr.register_tool(Arc::new(EchoTool));
        assert_eq!(mgr.list_tools().len(), 1);
        assert!(mgr.list_tools().contains(&"echo".to_string()));
    }

    #[test]
    fn test_toolbox_manager_execute() {
        let mut mgr = ToolboxManager::new();
        mgr.register_tool(Arc::new(EchoTool));

        let request = ToolRequest {
            tool: "echo".to_string(),
            arguments: json!({"message": "test"}),
        };

        let response = mgr.execute_tool(&request).unwrap();
        assert_eq!(response.status, "success");
    }

    #[test]
    fn test_toolbox_manager_tool_not_found() {
        let mgr = ToolboxManager::new();

        let request = ToolRequest {
            tool: "nonexistent".to_string(),
            arguments: json!({}),
        };

        let result = mgr.execute_tool(&request);
        assert!(result.is_err());

        match result {
            Err(ToolError::NotFound(name)) => assert_eq!(name, "nonexistent"),
            _ => panic!("Expected NotFound error"),
        }
    }

    #[test]
    fn test_global_toolbox() {
        // Access the global instance
        let _mgr = ToolboxManager::global();
        // Just verify it doesn't panic
    }

    #[test]
    fn test_init_global() {
        ToolboxManager::init_global();
        let mgr = ToolboxManager::global().lock().unwrap();
        assert!(mgr.list_tools().len() > 0);
    }
}
