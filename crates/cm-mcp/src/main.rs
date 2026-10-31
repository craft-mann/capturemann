//! `cm-mcp`: the MCP server binary. Connects to the daemon socket.
//!
//! * `cm-mcp [--socket <path>]`: MCP over stdio (default). The socket defaults to `CAPTUREMANN_SOCKET`, then the platform path.
//! * `cm-mcp --http [--port <n>] [--socket <path>]`: local Streamable HTTP on `127.0.0.1`
//!   (random port unless `--port`), bearer token from `CAPTUREMANN_MCP_TOKEN` or a fresh
//!   random one. The address and the token are printed to stderr once.
//!
//! The default daemon endpoint is the platform one. Logs go to stderr only: stdout is the MCP
//! channel in stdio mode.

use std::net::SocketAddr;
use std::process::ExitCode;

use cm_ipc::Endpoint;

fn usage() -> ExitCode {
    eprintln!("usage: cm-mcp [--http [--port <n>]] [--socket <path>]");
    ExitCode::from(2)
}

#[tokio::main]
async fn main() -> ExitCode {
    let mut endpoint = Endpoint::from_env_or_default();
    let mut http = false;
    let mut port: u16 = 0;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--socket" => match args.next() {
                Some(v) => endpoint = Endpoint::from_arg(&v),
                None => return usage(),
            },
            "--http" => http = true,
            "--port" => match args.next().and_then(|v| v.parse().ok()) {
                Some(p) => port = p,
                None => return usage(),
            },
            _ => return usage(),
        }
    }
    let connector = cm_mcp::endpoint_connector(endpoint);
    let result = if http {
        run_http(connector, port).await
    } else {
        cm_mcp::serve_stdio(connector).await
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("cm-mcp: {e}");
            ExitCode::FAILURE
        }
    }
}

async fn run_http(
    connector: cm_mcp::Connector,
    port: u16,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let token = match std::env::var("CAPTUREMANN_MCP_TOKEN") {
        Ok(t) if !t.is_empty() => t,
        _ => cm_mcp::generate_token()?,
    };
    let opts = cm_mcp::HttpOptions {
        addr: SocketAddr::from(([127, 0, 0, 1], port)),
        token: token.clone(),
    };
    let server = cm_mcp::serve_http(connector, opts).await?;
    eprintln!("cm-mcp listening on http://{}/mcp", server.local_addr());
    eprintln!("bearer token: {token}");
    server.wait().await;
    Ok(())
}
