use anyhow::Result;
use log::{debug, error, trace, warn};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::Path,
    sync::{
        atomic::{AtomicI64, Ordering},
        Arc,
    },
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
    sync::{mpsc, oneshot, Mutex},
    time::{sleep, timeout, Duration},
};

use crate::errors::PitsnakeError;

// ---------------------------------------------------------------------------
// Internal types
// ---------------------------------------------------------------------------

struct PendingRequest {
    method: String,
    tx: oneshot::Sender<Result<Value, PitsnakeError>>,
}

// ---------------------------------------------------------------------------
// LspClient
// ---------------------------------------------------------------------------

/// A handle to a running LSP server subprocess.
/// Cheap to clone — all state is behind `Arc`.
#[derive(Clone)]
pub struct LspClient {
    outgoing_tx: mpsc::Sender<Vec<u8>>,
    pending: Arc<Mutex<HashMap<i64, PendingRequest>>>,
    diagnostics: Arc<Mutex<HashMap<String, Vec<Value>>>>,
    next_id: Arc<AtomicI64>,
    timeout_secs: u64,
}

impl LspClient {
    /// Spawn the language server, perform the LSP initialize handshake, and
    /// return a ready-to-use client.
    pub async fn spawn(
        command: &str,
        args: &[String],
        workspace_root: &Path,
        timeout_secs: u64,
    ) -> Result<Self> {
        let mut child: Child = Command::new(command)
            .args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .spawn()?;

        let stdin: ChildStdin = child.stdin.take().expect("stdin missing");
        let stdout: ChildStdout = child.stdout.take().expect("stdout missing");

        let pending: Arc<Mutex<HashMap<i64, PendingRequest>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let diagnostics: Arc<Mutex<HashMap<String, Vec<Value>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let next_id = Arc::new(AtomicI64::new(1));

        let (outgoing_tx, outgoing_rx) = mpsc::channel::<Vec<u8>>(64);

        tokio::spawn(writer_task(stdin, outgoing_rx));
        tokio::spawn(reader_task(
            stdout,
            Arc::clone(&pending),
            Arc::clone(&diagnostics),
        ));

        let client = LspClient {
            outgoing_tx,
            pending,
            diagnostics,
            next_id,
            timeout_secs,
        };

        client.initialize(workspace_root).await?;
        Ok(client)
    }

    // -----------------------------------------------------------------------
    // Public API
    // -----------------------------------------------------------------------

    /// Send an LSP request and wait for the response (up to `timeout_secs`).
    pub async fn request(&self, method: &str, params: Value) -> Result<Value, PitsnakeError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);

        let body = serde_json::to_vec(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }))?;

        let (tx, rx) = oneshot::channel();

        // Insert into the pending map *before* writing to the channel so the
        // reader task can never dispatch a response before we're registered.
        self.pending.lock().await.insert(
            id,
            PendingRequest {
                method: method.to_string(),
                tx,
            },
        );

        self.outgoing_tx
            .send(body)
            .await
            .map_err(|e| PitsnakeError::ChannelError(e.to_string()))?;

        let method_owned = method.to_string();
        let secs = self.timeout_secs;

        timeout(Duration::from_secs(secs), rx)
            .await
            .map_err(|_| PitsnakeError::Timeout {
                method: method_owned,
                timeout_secs: secs,
            })?
            .map_err(|_| PitsnakeError::ProcessDied)?
    }

    /// Send an LSP notification (fire-and-forget — no response expected).
    pub async fn notify(&self, method: &str, params: Value) -> Result<(), PitsnakeError> {
        let body = serde_json::to_vec(&json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        }))?;

        self.outgoing_tx
            .send(body)
            .await
            .map_err(|e| PitsnakeError::ChannelError(e.to_string()))
    }

    /// Open a file via `didOpen`, wait for `publishDiagnostics` (up to
    /// `timeout_secs`), close the file, and return the diagnostics.
    /// A timeout (server sent nothing) is treated as an empty list.
    pub async fn fetch_diagnostics(
        &self,
        uri: &str,
        text: &str,
        language_id: &str,
    ) -> Result<Vec<Value>, PitsnakeError> {
        // Clear stale data so we don't return results from a previous open.
        self.diagnostics.lock().await.remove(uri);

        self.notify(
            "textDocument/didOpen",
            json!({
                "textDocument": {
                    "uri": uri,
                    "languageId": language_id,
                    "version": 1,
                    "text": text,
                }
            }),
        )
        .await?;

        let uri_owned = uri.to_string();
        let diags_map = Arc::clone(&self.diagnostics);
        let secs = self.timeout_secs;

        let poll_result = timeout(Duration::from_secs(secs), async move {
            loop {
                sleep(Duration::from_millis(50)).await;
                if let Some(diags) = diags_map.lock().await.get(&uri_owned).cloned() {
                    return diags;
                }
            }
        })
        .await;

        self.notify(
            "textDocument/didClose",
            json!({ "textDocument": { "uri": uri } }),
        )
        .await?;

        Ok(poll_result.unwrap_or_default())
    }

    // -----------------------------------------------------------------------
    // Private
    // -----------------------------------------------------------------------

    async fn initialize(&self, workspace_root: &Path) -> Result<()> {
        let root_uri = path_to_uri(workspace_root);
        debug!("LSP initialize, rootUri={}", root_uri);

        self.request(
            "initialize",
            json!({
                "processId": std::process::id(),
                "rootUri": root_uri,
                "capabilities": {
                    "textDocument": {
                        "synchronization": {
                            "didOpen": true,
                            "didClose": true,
                        },
                        "definition": { "linkSupport": false },
                        "references": {},
                        "hover": {
                            "contentFormat": ["markdown", "plaintext"]
                        },
                        "completion": {},
                        "publishDiagnostics": {}
                    },
                    "workspace": {
                        "symbol": {}
                    }
                },
                "clientInfo": { "name": "clara-pitsnake", "version": "0.1.0" }
            }),
        )
        .await
        .map_err(|e| anyhow::anyhow!("LSP initialize failed: {}", e))?;

        self.notify("initialized", json!({}))
            .await
            .map_err(|e| anyhow::anyhow!("LSP initialized notification failed: {}", e))?;

        debug!("LSP handshake complete");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Writer task — owns ChildStdin, serialises all outgoing bytes
// ---------------------------------------------------------------------------

async fn writer_task(mut stdin: ChildStdin, mut rx: mpsc::Receiver<Vec<u8>>) {
    while let Some(body) = rx.recv().await {
        let header = format!("Content-Length: {}\r\n\r\n", body.len());
        if stdin.write_all(header.as_bytes()).await.is_err()
            || stdin.write_all(&body).await.is_err()
            || stdin.flush().await.is_err()
        {
            error!("LSP writer: write failed — process may have exited");
            break;
        }
        trace!("LSP → server: {} bytes", body.len());
    }
    debug!("LSP writer task exiting");
}

// ---------------------------------------------------------------------------
// Reader task — owns ChildStdout, dispatches responses and notifications
// ---------------------------------------------------------------------------

async fn reader_task(
    stdout: ChildStdout,
    pending: Arc<Mutex<HashMap<i64, PendingRequest>>>,
    diagnostics: Arc<Mutex<HashMap<String, Vec<Value>>>>,
) {
    let mut reader = BufReader::new(stdout);

    loop {
        let content_len = match read_content_length(&mut reader).await {
            Some(n) => n,
            None => break,
        };

        let mut body = vec![0u8; content_len];
        if let Err(e) = reader.read_exact(&mut body).await {
            error!("LSP reader: body read error: {}", e);
            break;
        }

        let value: Value = match serde_json::from_slice(&body) {
            Ok(v) => v,
            Err(e) => {
                warn!("LSP reader: JSON parse error: {}", e);
                continue;
            }
        };

        trace!("LSP ← server: {}", &value.to_string()[..value.to_string().len().min(120)]);
        dispatch_message(value, &pending, &diagnostics).await;
    }

    debug!("LSP reader task exiting — draining pending requests");
    for (_, req) in pending.lock().await.drain() {
        let _ = req.tx.send(Err(PitsnakeError::ProcessDied));
    }
}

/// Read LSP headers until the blank line; return the Content-Length value.
/// Returns `None` on EOF or unrecoverable I/O error.
async fn read_content_length(reader: &mut BufReader<ChildStdout>) -> Option<usize> {
    let mut content_len: Option<usize> = None;

    loop {
        let mut line = String::new();
        match reader.read_line(&mut line).await {
            Ok(0) => return None, // EOF
            Ok(_) => {}
            Err(e) => {
                error!("LSP reader: header read error: {}", e);
                return None;
            }
        }

        // Trim \r\n / \n — deliberately NOT using str::lines() which is
        // UTF-8 only and may panic on binary content inside the body.
        let trimmed = line.trim_end_matches(|c| c == '\r' || c == '\n');

        if trimmed.is_empty() {
            break; // blank line = end of headers
        }

        if let Some(rest) = trimmed.strip_prefix("Content-Length:") {
            match rest.trim().parse::<usize>() {
                Ok(n) => content_len = Some(n),
                Err(_) => warn!("LSP reader: unparseable Content-Length: '{}'", rest.trim()),
            }
        }
        // Other headers (Content-Type: application/vscode-jsonrpc; charset=utf-8) are ignored.
    }

    content_len
}

async fn dispatch_message(
    value: Value,
    pending: &Arc<Mutex<HashMap<i64, PendingRequest>>>,
    diagnostics: &Arc<Mutex<HashMap<String, Vec<Value>>>>,
) {
    let has_id = value.get("id").map(|v| !v.is_null()).unwrap_or(false);
    let method = value.get("method").and_then(|m| m.as_str()).map(str::to_owned);

    if has_id && method.is_none() {
        // Response to one of our requests.
        let id = match value["id"].as_i64() {
            Some(i) => i,
            None => {
                warn!("LSP reader: non-integer response id: {}", value["id"]);
                return;
            }
        };

        let result = if let Some(err) = value.get("error") {
            let code = err.get("code").and_then(|c| c.as_i64()).unwrap_or(-1) as i32;
            let message = err
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("unknown LSP error")
                .to_string();
            Err(PitsnakeError::LspError { code, message })
        } else {
            Ok(value.get("result").cloned().unwrap_or(Value::Null))
        };

        if let Some(req) = pending.lock().await.remove(&id) {
            trace!("dispatched response id={} method={}", id, req.method);
            let _ = req.tx.send(result);
        } else {
            warn!("LSP reader: response for unregistered id={}", id);
        }
    } else if let Some(m) = method {
        // Notification (or server-initiated request we don't handle).
        if m == "textDocument/publishDiagnostics" {
            if let Some(params) = value.get("params") {
                let uri = params
                    .get("uri")
                    .and_then(|u| u.as_str())
                    .unwrap_or("")
                    .to_string();
                let diags = params
                    .get("diagnostics")
                    .and_then(|d| d.as_array())
                    .cloned()
                    .unwrap_or_default();
                debug!("publishDiagnostics uri={} count={}", uri, diags.len());
                diagnostics.lock().await.insert(uri, diags);
            }
        } else {
            trace!("ignoring notification method={}", m);
        }
    }
}

// ---------------------------------------------------------------------------
// URI / extension helpers (pub for use in tools.rs)
// ---------------------------------------------------------------------------

fn path_to_uri(path: &Path) -> String {
    let s = path.to_string_lossy();
    if s.starts_with('/') {
        format!("file://{}", s)
    } else {
        format!("file:///{}", s)
    }
}

pub fn file_path_to_uri(path: &str) -> String {
    if path.starts_with('/') {
        format!("file://{}", path)
    } else {
        format!("file:///{}", path)
    }
}

pub fn language_id_for(path: &str) -> &'static str {
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    match ext {
        "rs" => "rust",
        "py" => "python",
        "ts" | "tsx" => "typescript",
        "js" | "jsx" => "javascript",
        "go" => "go",
        "c" | "h" => "c",
        "cpp" | "cc" | "cxx" | "hpp" => "cpp",
        "java" => "java",
        "rb" => "ruby",
        "sh" => "shellscript",
        "json" => "json",
        "toml" => "toml",
        "yaml" | "yml" => "yaml",
        "md" => "markdown",
        _ => "plaintext",
    }
}
