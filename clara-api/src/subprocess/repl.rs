use clara_core::{ClaraError, ClaraResult, EvalResult, EvalMetrics};
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Instant;
use log::debug;

/// REPL Protocol handler for CLIPS subprocess communication
/// Uses transactional interaction - spawns a fresh process for each eval
pub struct ReplHandler {
    clips_binary: String,
}

impl ReplHandler {
    /// Create a new REPL handler (doesn't spawn a process until eval)
    pub fn new(clips_binary: &str) -> ClaraResult<Self> {
        debug!("Initializing REPL handler for CLIPS binary: {}", clips_binary);

        Ok(Self {
            clips_binary: clips_binary.to_owned(),
        })
    }

    /// Execute a command in a fresh CLIPS subprocess (transactional)
    /// Spawns a new process, sends command + (exit), and waits for completion
    /// (( todo timeout handling )))
    pub fn execute(&mut self, command: &str, _timeout_ms: u64) -> ClaraResult<EvalResult> {
        let start = Instant::now();

        debug!("Spawning fresh CLIPS subprocess for command: {}", command);

        // Create a child process with piped stdin/stdout
        let mut child = Command::new(&self.clips_binary)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| ClaraError::ProcessSpawnError(format!("Failed to spawn CLIPS: {}", e)))?;

        // Get stdin handle
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| ClaraError::ProcessCommunicationError("Cannot capture stdin".to_string()))?;

        // Write command and exit marker
        writeln!(stdin, "{}", command).map_err(|e| {
            ClaraError::ProcessCommunicationError(format!("Failed to write command: {}", e))
        })?;

        writeln!(stdin, "(exit)").map_err(|e| {
            ClaraError::ProcessCommunicationError(format!("Failed to write exit: {}", e))
        })?;

        // Close stdin to signal EOF to CLIPS
        drop(stdin);

        debug!("Command and exit sent, waiting for subprocess completion...");

        // Wait for the process to complete (this will block until it exits)
        let output = child
            .wait_with_output()
            .map_err(|e| ClaraError::ProcessCommunicationError(format!("Failed to wait for subprocess: {}", e)))?;

        let elapsed = start.elapsed().as_millis() as u64;
        let metrics = EvalMetrics::with_elapsed(elapsed);

        // Parse stdout as the output transcript
        let stdout_str = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr_str = String::from_utf8_lossy(&output.stderr).to_string();

        debug!("Subprocess completed in {}ms", elapsed);
        debug!("STDOUT:\n{}", stdout_str);
        if !stderr_str.is_empty() {
            debug!("STDERR:\n{}", stderr_str);
        }

        // Check exit status
        if !output.status.success() {
            debug!("CLIPS process exited with non-zero status: {:?}", output.status);
        }

        // Return the full transcript as output
        let result = if stderr_str.is_empty() {
            EvalResult::success(stdout_str, metrics)
        } else {
            EvalResult::failure(format!("{}\n{}", stdout_str, stderr_str), metrics)
        };

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_repl_handler_creation() {
        let handler = ReplHandler::new("/bin/ls");
        assert!(handler.is_ok());
    }
}
