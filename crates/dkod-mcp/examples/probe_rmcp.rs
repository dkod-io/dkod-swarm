//! Probe that verifies the rmcp 1.5 surface the M2 plan leans on:
//! - `rmcp::transport::stdio()` returns a transport.
//! - A handler struct with `#[tool_router(server_handler)]` exposes tools.
//! - `ServiceExt::serve` is available on the handler.
//!
//! Run with: `cargo run -p dkod-mcp --example probe_rmcp`
//! Expected: exits 0 and prints "probe ok" on stderr.

use rmcp::{
    RoleServer, ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    schemars, tool, tool_handler, tool_router,
    transport::stdio,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
struct PingArgs {
    msg: String,
}

#[derive(Clone, Default)]
struct ProbeServer {
    // The `#[tool_router]` macro generates methods that consult this field;
    // it is read through the macro expansion, which the dead-code pass does
    // not track. Allow the false-positive to keep the probe warning-free.
    #[allow(dead_code)]
    tool_router: ToolRouter<ProbeServer>,
}

#[tool_router]
impl ProbeServer {
    #[tool(description = "probe tool, returns the input string prefixed with 'pong: '")]
    async fn ping(
        &self,
        Parameters(args): Parameters<PingArgs>,
    ) -> Result<String, rmcp::ErrorData> {
        Ok(format!("pong: {}", args.msg))
    }
}

#[tool_handler]
impl ServerHandler for ProbeServer {}

fn main() {
    // Verify constructibility without actually starting the stdio loop
    // (that would block forever waiting on stdin).
    let _server = ProbeServer::default();
    // `stdio()` is a compile-time check that the transport symbol exists
    // and returns the expected type; don't serve.
    let _t: fn() -> (tokio::io::Stdin, tokio::io::Stdout) = stdio;
    // `ServiceExt::serve` is in scope via the trait import — touch a bound
    // on the handler to ensure the blanket `ServiceExt<RoleServer>` impl
    // resolves for our server type.
    fn _assert_service_ext<S: ServiceExt<RoleServer>>(_: &S) {}
    _assert_service_ext(&_server);
    eprintln!("probe ok");
}
