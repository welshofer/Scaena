//! `scaena mcp` (PLAN 1.17): the MCP server on stdio, as an agent's client starts it.

use rmcp::ServiceExt;
use rmcp::model::{CallToolRequestParams, ClientConfig};
use std::process::Stdio;

#[tokio::test(flavor = "multi_thread")]
async fn scaena_mcp_serves_on_stdio_until_the_client_goes() {
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_scaena"))
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let io = (child.stdout.take().unwrap(), child.stdin.take().unwrap());
    let client = ClientConfig::default().serve(io).await.unwrap();
    assert_eq!(client.list_all_tools().await.unwrap().len(), 12);
    let args = serde_json::json!({ "bundle": "../../docs/examples/revenue.deck.json" });
    let request = CallToolRequestParams::new("spine_read").with_arguments(args.as_object().unwrap().clone());
    let result = client.call_tool(request).await.unwrap();
    assert_ne!(result.is_error, Some(true));
    assert_eq!(result.structured_content.unwrap()["title"], "Q3 Review");
    client.cancel().await.unwrap();
    assert!(child.wait().await.unwrap().success(), "the server exits cleanly when the client goes");
}
