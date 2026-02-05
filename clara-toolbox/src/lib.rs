// Clara Toolbox - Tool registry and execution framework

pub mod ffi;
pub mod manager;
pub mod tool;
pub mod tools;

// Re-export commonly used types
pub use manager::ToolboxManager;
pub use tool::{Tool, ToolError, ToolRequest, ToolResponse};
pub use tools::{ClassifyTool, ClaraSplinteredMindTool, EchoTool, EvaluateTool};

// Re-export FFI functions for convenience
pub use ffi::{evaluate_json_string, free_c_string};
