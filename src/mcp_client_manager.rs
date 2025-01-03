use crate::config::MCPServerConfig;
use anyhow::{Error, Result};
use mcp_client_rs::client::{Client, ClientBuilder};
use mcp_client_rs::CallToolResult;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug)]
pub struct ToolDescription {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

pub struct MCPClientManager {
    clients: HashMap<String, Arc<Client>>,
    tool_mapping: HashMap<String, (String, String)>,
}

impl MCPClientManager {
    pub async fn new(configs: &HashMap<String, MCPServerConfig>) -> Result<Self> {
        // TODO: go find the AIchat repo and use that

        let mut clients = HashMap::new();
        let mut tool_mapping = HashMap::new();

        for (name, server_conf) in configs {
            let mut builder = ClientBuilder::new(&server_conf.command);
            for arg in &server_conf.args {
                builder = builder.arg(arg);
            }

            for (k, v) in &server_conf.env {
                builder = builder.env(k, v);
            }

            let client = builder.spawn_and_initialize().await?;
            let client = Arc::new(client);

            // fetch tools from this server (if needed)
            let tools_val = client.request("tools/list", None).await?;
            if let Some(tools_arr) = tools_val.get("tools").and_then(|v| v.as_array()) {
                for t in tools_arr {
                    if let Some(name_str) = t.get("name").and_then(|x| x.as_str()) {
                        // For simplicity, assume unique names across servers
                        // if multiple servers have same tool name, you need a strategy
                        tool_mapping
                            .insert(name_str.to_string(), (name.clone(), name_str.to_string()));
                    }
                }
            }

            clients.insert(name.clone(), client);
        }

        Ok(Self {
            clients,
            tool_mapping,
        })
    }

    pub async fn call_tool(
        &self,
        tool_name: &str,
        arguments: Value,
    ) -> std::result::Result<CallToolResult, Error> {
        let (server_name, tool_id) = self
            .tool_mapping
            .get(tool_name)
            .ok_or_else(|| anyhow::anyhow!("Tool '{}' not found", tool_name))?;

        let client = self.clients.get(server_name).ok_or_else(|| {
            anyhow::anyhow!(
                "Server '{}' not found for tool '{}'",
                server_name,
                tool_name
            )
        })?;

        client
            .call_tool(tool_id, arguments)
            .await
            .map_err(Into::into)
    }

    pub async fn get_available_tools(&self) -> Result<Vec<ToolDescription>> {
        // Just pick one server to fetch from if you want, or aggregate from all
        // For simplicity, pick the first server
        if let Some((_, client)) = self.clients.iter().next() {
            let tools_val = client.request("tools/list", None).await?;

            if let Some(tools_arr) = tools_val.get("tools").and_then(|v| v.as_array()) {
                let mut tools = Vec::new();

                for tool in tools_arr {
                    if let (Some(name), Some(description), Some(schema)) = (
                        tool.get("name").and_then(|x| x.as_str()),
                        tool.get("description").and_then(|x| x.as_str()),
                        tool.get("inputSchema"),
                    ) {
                        // inputSchema expected by MCP is something like {type: object, properties: ...}
                        // OpenAI requires a parameters object with a schema
                        // Just pass it directly as the parameters
                        tools.push(ToolDescription {
                            name: name.to_string(),
                            description: description.to_string(),
                            parameters: schema.clone(),
                        });
                    }
                }

                return Ok(tools);
            } else {
                Ok(vec![])
            }
        } else {
            Ok(vec![])
        }
    }
}
