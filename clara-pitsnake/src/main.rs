use anyhow::Result;
use env_logger::Env;
use log::info;
use std::env;
use std::path::PathBuf;
use std::sync::Arc;

mod errors;
mod http_server;
mod lsp_client;
mod schemas;
mod server;
mod tools;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    // --- Configuration from environment variables ---
    let lsp_command = env::var("PITSNAKE_LSP_COMMAND").unwrap_or_else(|_| "rust-analyzer".to_string());

    let lsp_args: Vec<String> = env::var("PITSNAKE_LSP_ARGS")
        .unwrap_or_default()
        .split_whitespace()
        .map(str::to_owned)
        .collect();

    let workspace: PathBuf = env::var("PITSNAKE_WORKSPACE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| env::current_dir().expect("cannot determine current directory"));

    let transport = env::var("TRANSPORT").unwrap_or_else(|_| "stdio".to_string());
    let http_host = env::var("HTTP_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let http_port: u16 = env::var("HTTP_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8765);
    let lsp_timeout: u64 = env::var("PITSNAKE_LSP_TIMEOUT")
        .ok()
        .and_then(|t| t.parse().ok())
        .unwrap_or(30);

    info!("clara-pitsnake starting");
    info!("  LSP command : {} {}", lsp_command, lsp_args.join(" "));
    info!("  workspace   : {}", workspace.display());
    info!("  transport   : {}", transport);
    info!("  LSP timeout : {}s", lsp_timeout);

    // Spawn the language server and perform the initialize handshake.
    let lsp_client =
        lsp_client::LspClient::spawn(&lsp_command, &lsp_args, &workspace, lsp_timeout).await?;

    info!("LSP server ready");

    let mcp_server = Arc::new(server::McpServer::new(lsp_client));

    match transport.as_str() {
        "http" => http_server::run(mcp_server, &http_host, http_port).await?,
        _ => mcp_server.run_stdio().await?,
    }

    Ok(())
}
