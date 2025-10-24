//! Subprocess management for CLIPS execution
//!
//! This module handles CLIPS subprocess execution using a transactional model
//! where each eval spawns a fresh process, executes commands, and collects output.

pub mod repl;

pub use repl::ReplHandler;

use clara_core::{ClaraResult, ClaraError, EvalResult};
use log::debug;

/// Transactional CLIPS subprocess manager
/// Each execute() call spawns a fresh CLIPS process
pub struct SubprocessPool {
    clips_binary: String,
}

impl SubprocessPool {
    /// Create a new subprocess manager
    pub fn new(clips_binary: String, _sentinel_marker: String) -> Self {
        Self { clips_binary }
    }

    /// Execute a command in a fresh CLIPS subprocess (transactional model)
    /// Sessions are used for resource management and login tracking only
    pub fn execute(&self, _session_id: &str, command: &str, timeout_ms: u64) -> ClaraResult<EvalResult> {
        debug!("SubprocessPool::execute spawning fresh CLIPS process");
        debug!("Command length: {} bytes, timeout: {}ms", command.len(), timeout_ms);

        // Create a fresh handler and execute (it spawns and cleans up its own process)
        let mut handler = ReplHandler::new(&self.clips_binary)?;
        handler.execute(command, timeout_ms)
    }
}

impl Clone for SubprocessPool {
    fn clone(&self) -> Self {
        Self {
            clips_binary: self.clips_binary.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subprocess_pool_creation() {
        let pool = SubprocessPool::new(
            "./clips".to_string(),
            "__END__".to_string(),
        );
        // Pool is now created with just the binary path
        assert!(pool.clips_binary.contains("clips"));
    }
}
