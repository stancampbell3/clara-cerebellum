//! Subprocess management for CLIPS execution
//!
//! This module handles spawning and managing CLIPS subprocess instances,
//! implementing the REPL protocol for command execution and output capture.

pub mod repl;

pub use repl::ReplHandler;

use clara_core::{ClaraResult, ClaraError, EvalResult};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use log::{debug, info};

/// Pool of CLIPS subprocess instances, one per session
pub struct SubprocessPool {
    handlers: Arc<Mutex<HashMap<String, ReplHandler>>>,
    clips_binary: String,
    sentinel_marker: String,
}

impl SubprocessPool {
    /// Create a new subprocess pool
    pub fn new(clips_binary: String, sentinel_marker: String) -> Self {
        Self {
            handlers: Arc::new(Mutex::new(HashMap::new())),
            clips_binary,
            sentinel_marker,
        }
    }

    /// Get or create a subprocess for a session
    pub fn get_or_create(&self, session_id: &str) -> ClaraResult<()> {
        let mut handlers = self
            .handlers
            .lock()
            .map_err(|_| ClaraError::LockPoisoned)?;

        if !handlers.contains_key(session_id) {
            debug!("Creating new CLIPS subprocess for session: {}", session_id);
            let handler = ReplHandler::new(&self.clips_binary, self.sentinel_marker.clone())?;
            handlers.insert(session_id.to_string(), handler);
            info!("Subprocess created for session: {}", session_id);
        }

        Ok(())
    }
    
    /// Spin up the subprocess for a session if not already present
    pub fn ensure_subprocess(&self, session_id: &str) -> ClaraResult<()> {
        debug!("Ensuring subprocess for session: {}", session_id);
        self.get_or_create(session_id)
    }

    /// Execute a command in a session's subprocess
    pub fn execute(&self, session_id: &str, command: &str, timeout_ms: u64) -> ClaraResult<EvalResult> {
        debug!("SubprocessPool::execute called for session: {}", session_id);
        debug!("Command length: {} bytes, timeout: {}ms", command.len(), timeout_ms);
        
        // Ensure subprocess exists
        debug!("Ensuring subprocess exists for session: {}", session_id);
        self.get_or_create(session_id)?;
    
        debug!("Acquiring lock on handlers map");
        let mut handlers = self
            .handlers
            .lock()
            .map_err(|_| ClaraError::LockPoisoned)?;
    
        debug!("Lock acquired, looking up handler for session: {}", session_id);
        let handler = handlers
            .get_mut(session_id)
            .ok_or_else(|| ClaraError::Internal("Subprocess not found".to_string()))?;
    
        debug!("Handler found, checking if subprocess is alive");
        // Check if subprocess is alive
        if !handler.is_alive() {
            debug!("Subprocess is dead for session: {}", session_id);
            // Remove dead subprocess
            handlers.remove(session_id);
            drop(handlers); // Release lock before recursive call
    
            // Recreate and retry
            debug!("Subprocess was dead, recreating for session: {}", session_id);
            return self.execute(session_id, command, timeout_ms);
        }
    
        debug!("Subprocess is alive, delegating to handler.execute()");
        let result = handler.execute(command, timeout_ms);
        
        match &result {
            Ok(eval_result) => {
                debug!("Handler execution succeeded: exit_code={}, elapsed={}ms", 
                       eval_result.exit_code, eval_result.metrics.elapsed_ms);
            }
            Err(e) => {
                debug!("Handler execution failed: {:?}", e);
            }
        }
        
        result
    }

    /// Terminate a session's subprocess
    pub fn terminate(&self, session_id: &str) -> ClaraResult<()> {
        let mut handlers = self
            .handlers
            .lock()
            .map_err(|_| ClaraError::LockPoisoned)?;

        if let Some(mut handler) = handlers.remove(session_id) {
            debug!("Terminating subprocess for session: {}", session_id);
            handler.terminate()?;
            info!("Subprocess terminated for session: {}", session_id);
        }

        Ok(())
    }

    /// Get count of active subprocesses
    pub fn active_count(&self) -> ClaraResult<usize> {
        let handlers = self
            .handlers
            .lock()
            .map_err(|_| ClaraError::LockPoisoned)?;
        Ok(handlers.len())
    }

    /// Terminate all subprocesses
    pub fn terminate_all(&self) -> ClaraResult<()> {
        let mut handlers = self
            .handlers
            .lock()
            .map_err(|_| ClaraError::LockPoisoned)?;

        for (_session_id, mut handler) in handlers.drain() {
            let _ = handler.terminate();
        }

        Ok(())
    }
}

impl Clone for SubprocessPool {
    fn clone(&self) -> Self {
        Self {
            handlers: Arc::clone(&self.handlers),
            clips_binary: self.clips_binary.clone(),
            sentinel_marker: self.sentinel_marker.clone(),
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
        assert_eq!(pool.active_count().unwrap_or(0), 0);
    }
}
