use clara_core::{ClaraError, ClaraResult, EvalResult, EvalMetrics};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use log::{debug, error, warn};
use std::thread;

/// REPL Protocol handler for CLIPS subprocess communication
pub struct ReplHandler {
    process: Child,
    reader: BufReader<std::process::ChildStdout>,
    writer: BufWriter<std::process::ChildStdin>,
    ready: bool,
    sentinel_marker: String,
}

impl ReplHandler {
    /// Create a new REPL handler for a CLIPS subprocess
    pub fn new(clips_binary: &str, sentinel_marker: String) -> ClaraResult<Self> {
        debug!("Spawning CLIPS subprocess: {}", clips_binary);

        let mut process = Command::new(clips_binary)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| ClaraError::ProcessSpawnError(format!("Failed to spawn CLIPS: {}", e)))?;

        let stdout = process
            .stdout
            .take()
            .ok_or_else(|| ClaraError::ProcessCommunicationError("Cannot capture stdout".to_string()))?;
        
        let stdin = process
            .stdin
            .take()
            .ok_or_else(|| ClaraError::ProcessCommunicationError("Cannot capture stdin".to_string()))?;
        
        let stderr = process
            .stderr
            .take()
            .ok_or_else(|| ClaraError::ProcessCommunicationError("Cannot capture stderr".to_string()))?;

        let reader = BufReader::new(stdout);
        let writer = BufWriter::new(stdin);

        let mut handler = Self {
            process,
            reader,
            writer,
            ready: false,
            sentinel_marker,
        };

        // Initialize connection with handshake
        handler.initialize()?;

        Ok(handler)
    }

    /// Initialize the subprocess connection with a handshake
    fn initialize(&mut self) -> ClaraResult<()> {
        debug!("Initializing CLIPS subprocess");
        
        // First, check if the subprocess started successfully
        thread::sleep(Duration::from_millis(100));
        match self.process.try_wait() {
            Ok(Some(status)) => {
                error!("Subprocess exited immediately with status: {}", status);
                return Err(ClaraError::ProcessSpawnError(
                    format!("CLIPS subprocess exited immediately with status: {}", status)
                ));
            }
            Ok(None) => {
                debug!("Subprocess is running");
                self.writer.write_all("(clear)\n".as_bytes()).map_err(|e| {
                    ClaraError::ProcessCommunicationError(
                        format!("Failed to write initial command: {}", e)
                    )
                })?;                
                self.writer.flush().map_err(|e| {
                    ClaraError::ProcessCommunicationError(
                        format!("Failed to flush initial command: {}", e)
                    )
                })?;
            }
            Err(e) => {
                error!("Failed to check subprocess status: {}", e);
                return Err(ClaraError::ProcessCommunicationError(
                    format!("Failed to check subprocess status: {}", e)
                ));
            }
        }
        
        // Give CLIPS a moment to initialize and print its banner/prompt
        debug!("Waiting 500ms for CLIPS to initialize");
        thread::sleep(Duration::from_millis(500));
        
        // Try reading any initial output (banner, version info, etc.)
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut buffer = String::new();
        let mut all_output = String::new();
        let mut iteration = 0;
        let mut found_prompt = false;
    
        loop {
            iteration += 1;
            debug!("Initialize loop iteration {}", iteration);
    
            if Instant::now() > deadline {
                error!("Timeout waiting for CLIPS prompt after {} iterations", iteration);
                error!("Accumulated output so far: '{}'", all_output);
                
                // Check if subprocess is still alive
                if let Ok(Some(status)) = self.process.try_wait() {
                    error!("Subprocess died during initialization with status: {}", status);
                    return Err(ClaraError::SubprocessCrashed);
                }
                
                return Err(ClaraError::ProcessCommunicationError(
                    format!("Timeout waiting for CLIPS prompt. Output received: '{}'", all_output),
                ));
            }
    
            buffer.clear();
            debug!("Reading line from subprocess stdout (attempt {})", iteration);
            
            match self.reader.read_line(&mut buffer) {
                Ok(0) => {
                    error!("Subprocess stdout closed (EOF) during initialization");
                    error!("Output received before EOF: '{}'", all_output);
                    return Err(ClaraError::SubprocessCrashed);
                }
                Ok(n) => {
                    debug!("Read {} bytes: '{}'", n, buffer.trim());
                    all_output.push_str(&buffer);
                    
                    // Look for the CLIPS> prompt
                    if buffer.contains("CLIPS") {
                        debug!("CLIPS version line detected, subprocess ready");
                        found_prompt = true;
                        break;
                    } else {
                        debug!("No CLIPS version in line: '{}'", buffer.trim());
                        // Continue reading more lines
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    debug!("Would block - no data available yet");
                    thread::sleep(Duration::from_millis(50));
                    continue;
                }
                Err(e) => {
                    error!("Error reading from subprocess: {}", e);
                    return Err(ClaraError::ProcessCommunicationError(format!(
                        "Error reading from subprocess: {}",
                        e
                    )));
                }
            }
        }
        
        if found_prompt {
            debug!("Successfully initialized CLIPS subprocess");
            debug!("Full initialization output: '{}'", all_output);
            self.ready = true;
            Ok(())
        } else {
            error!("Exited initialization loop without finding prompt");
            Err(ClaraError::ProcessCommunicationError(
                "Failed to detect CLIPS prompt".to_string()
            ))
        }
    }

    /// Execute a command in the CLIPS subprocess
    pub fn execute(&mut self, command: &str, timeout_ms: u64) -> ClaraResult<EvalResult> {
        if !self.ready {
            return Err(ClaraError::Internal("Subprocess not ready".to_string()));
        }

        let start = Instant::now();
        let timeout = Duration::from_millis(timeout_ms);

        debug!("Executing command with {}ms timeout: {}", timeout_ms, command);

        // Get mutable stdin
        let stdin = self
            .process
            .stdin
            .as_mut()
            .ok_or_else(|| ClaraError::ProcessCommunicationError("Cannot write to stdin".to_string()))?;

        // Send command with newline
        writeln!(stdin, "{}", command).map_err(|e| {
            ClaraError::ProcessCommunicationError(format!("Failed to write command: {}", e))
        })?;

        // Send sentinel marker command to frame output
        writeln!(stdin, "(printout t \"{}\" crlf)", self.sentinel_marker)
            .map_err(|e| {
                ClaraError::ProcessCommunicationError(format!("Failed to write sentinel: {}", e))
            })?;

        // Collect output until sentinel
        let mut stdout = String::new();
        let mut stderr = String::new();

        loop {
            if start.elapsed() > timeout {
                error!("Command execution timeout after {}ms", timeout_ms);
                return Err(ClaraError::EvalTimeout {
                    timeout_ms,
                });
            }

            let mut line = String::new();
            match self.reader.read_line(&mut line) {
                Ok(0) => {
                    // EOF - subprocess crashed
                    error!("Unexpected EOF from subprocess");
                    self.ready = false;
                    return Err(ClaraError::SubprocessCrashed);
                }
                Ok(_) => {
                    debug!("Output: {}", line.trim());

                    // Check if this is the sentinel marker
                    if line.contains(&self.sentinel_marker) {
                        debug!("Found sentinel marker");
                        break;
                    }

                    // Check for error patterns (basic heuristic)
                    if line.contains("[ERROR]") || line.contains("Error:")  {
                        stderr.push_str(&line);
                    } else {
                        stdout.push_str(&line);
                    }
                }
                Err(e) => {
                    error!("Error reading from subprocess: {}", e);
                    self.ready = false;
                    return Err(ClaraError::ProcessCommunicationError(format!(
                        "Failed to read output: {}",
                        e
                    )));
                }
            }
        }

        let elapsed = start.elapsed().as_millis() as u64;
        let metrics = EvalMetrics::with_elapsed(elapsed);

        let result = if stderr.is_empty() {
            EvalResult::success(stdout, metrics)
        } else {
            EvalResult::failure(stderr, metrics)
        };

        Ok(result)
    }

    /// Check if subprocess is alive
    pub fn is_alive(&mut self) -> bool {
        self.ready && self.process.try_wait().ok().flatten().is_none()
    }

    /// Terminate the subprocess gracefully
    pub fn terminate(&mut self) -> ClaraResult<()> {
        debug!("Terminating CLIPS subprocess");

        // Try graceful shutdown first
        if let Ok(Some(_)) = self.process.try_wait() {
            // Already terminated
            return Ok(());
        }

        // Send (exit) command
        if let Some(stdin) = self.process.stdin.as_mut() {
            let _ = writeln!(stdin, "(exit)");
        }

        // Give it a moment to shut down
        std::thread::sleep(Duration::from_millis(100));

        // Force kill if needed
        match self.process.kill() {
            Ok(_) => {
                debug!("Successfully killed CLIPS subprocess");
                self.ready = false;
                Ok(())
            }
            Err(e) if e.kind() == std::io::ErrorKind::InvalidInput => {
                // Already terminated
                self.ready = false;
                Ok(())
            }
            Err(e) => Err(ClaraError::SubprocessError(format!("Failed to kill subprocess: {}", e))),
        }
    }
}

impl Drop for ReplHandler {
    fn drop(&mut self) {
        let _ = self.terminate();
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_sentinel_marker() {
        let marker = "__END__".to_string();
        assert!(!marker.is_empty());
    }
}
