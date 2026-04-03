// Clara Toolbox - Tool registry and execution framework

pub mod ffi;
pub mod manager;
pub mod tool;
pub mod tools;

// Re-export commonly used types
pub use manager::ToolboxManager;
pub use tool::{Tool, ToolError, ToolRequest, ToolResponse};
pub use tools::{ClassifyTool, ClaraSplinteredMindTool, EchoTool, EvaluateTool};

// Re-export FFI functions and cache types for convenience
pub use ffi::{
    evaluate_json_string, free_c_string,
    get_evaluate_call_count, reset_evaluate_call_count, clear_evaluate_cache,
    evaluate_cache_stats,
    evict_cache_older_than, evict_cache_by_deduction,
    set_current_deduction_id, current_deduction_id, deduction_context, DeductionContextGuard,
    set_domain_id, domain_id,
    CacheEntry, ToolboxCacheEviction,
};
