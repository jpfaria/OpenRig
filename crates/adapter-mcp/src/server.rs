//! `ServerHandler` bound to the command bridge, served over Streamable HTTP.
//!
//! Tools/resources/prompts give an agent full control + read parity with the
//! GUI. Every command flows through `application`'s bridge to the frontend's
//! dispatcher; `PublishingDispatcher` fans the resulting events to the
//! `EventSink` so GUI- and MCP-originated changes are observable alike.

use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Result;
use application::bridge::CommandBridge;
use rmcp::handler::server::ServerHandler;
use rmcp::model::{
    CallToolRequestParams, CallToolResult, Content, GetPromptRequestParams, GetPromptResult,
    ListPromptsResult, ListResourcesResult, ListToolsResult, PaginatedRequestParams,
    ReadResourceRequestParams, ReadResourceResult, ServerCapabilities, ServerInfo,
};
use rmcp::service::RequestContext;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use rmcp::{ErrorData, RoleServer};
use serde_json::Value;

use crate::{prompts, resources, tools};

#[derive(Clone)]
pub struct OpenRigMcp {
    bridge: CommandBridge,
}

impl OpenRigMcp {
    pub fn new(bridge: CommandBridge) -> Self {
        Self { bridge }
    }
}

impl ServerHandler for OpenRigMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .enable_prompts()
                .build(),
        )
        .with_instructions("OpenRig: every Command is a tool; project/devices are resources.")
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListToolsResult, ErrorData>> + Send + '_ {
        async move { Ok(ListToolsResult::with_all_items(tools::tools())) }
    }

    fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<CallToolResult, ErrorData>> + Send + '_ {
        async move {
            let args = request.arguments.map(Value::Object).unwrap_or(Value::Null);
            match tools::dispatch_tool(&self.bridge, &request.name, args).await {
                Ok(events) => {
                    let json = serde_json::to_string(&events)
                        .unwrap_or_else(|e| format!("<events serialize error: {e}>"));
                    Ok(CallToolResult::success(vec![Content::text(json)]))
                }
                Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
            }
        }
    }

    fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListResourcesResult, ErrorData>> + Send + '_ {
        async move { Ok(ListResourcesResult::with_all_items(resources::resources())) }
    }

    fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ReadResourceResult, ErrorData>> + Send + '_ {
        async move {
            resources::read(&self.bridge, &request.uri)
                .await
                .map_err(|e| ErrorData::internal_error(e.to_string(), None))
        }
    }

    fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListPromptsResult, ErrorData>> + Send + '_ {
        async move { Ok(ListPromptsResult::with_all_items(prompts::prompts())) }
    }

    fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<GetPromptResult, ErrorData>> + Send + '_ {
        async move {
            prompts::get(&request.name).ok_or_else(|| {
                ErrorData::invalid_params(format!("unknown prompt: {}", request.name), None)
            })
        }
    }
}

/// Serve the MCP server over Streamable HTTP until the listener is dropped.
pub async fn serve(bridge: CommandBridge, addr: SocketAddr) -> Result<()> {
    let handler = OpenRigMcp::new(bridge);
    let service = StreamableHttpService::new(
        move || Ok(handler.clone()),
        Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig::default(),
    );
    let app = axum::Router::new().fallback_service(service);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
