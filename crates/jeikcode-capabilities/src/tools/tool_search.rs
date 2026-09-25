//! Model-facing tool to discover deferred MCP tools via bilingual thesaurus search.

use std::sync::Arc;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;

use jeikcode_kernel::tool::{Tool, ToolContext, ToolResult};
use crate::mcp::McpToolIndex;

/// Callback interface for dynamically mounting tools into active execution.
#[async_trait]
pub trait ToolMountActivator: Send + Sync {
    /// Request that tools with the given names be mounted into the active catalog.
    async fn activate_tools(&self, tool_names: &[String]);
}

/// Dynamic MCP Tool Search Tool using bilingual thesaurus & dense embeddings.
pub struct ToolSearchTool {
    index: Arc<McpToolIndex>,
    activator: Option<Arc<dyn ToolMountActivator>>,
}

impl ToolSearchTool {
    pub fn new(index: Arc<McpToolIndex>, activator: Option<Arc<dyn ToolMountActivator>>) -> Self {
        Self { index, activator }
    }
}

#[derive(Deserialize)]
struct SearchArgs {
    #[serde(default)]
    queries: Option<Vec<String>>,
    #[serde(default)]
    tool_names: Option<Vec<String>>,
    #[serde(default)]
    top_k: Option<usize>,
}

#[async_trait]
impl Tool for ToolSearchTool {
    fn name(&self) -> &str {
        "tool_search"
    }

    fn description(&self) -> &str {
        "Discover and dynamically activate deferred tools. \
        Use this tool when a user task needs external capabilities (such as web browsing, \
        database access, spreadsheets, email, or third-party MCP servers) that are not present \
        in the default tools list. Once activated, the tools become directly callable in the next round. \
        Supports bilingual natural language queries (both Chinese and English) or exact tool names."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "queries": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Natural-language query terms in Chinese or English describing desired capability (e.g. ['截图', 'screenshot', '发邮件', 'exec_sql'])."
                },
                "tool_names": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Exact tool names if known from candidate list (e.g. ['mcp__chrome-devtools__take_screenshot'])."
                },
                "top_k": {
                    "type": "integer",
                    "description": "Max tools to return per query (default: 3, max: 10)."
                }
            }
        })
    }

    async fn execute(&self, args: &str, _ctx: &ToolContext) -> ToolResult {
        let args: SearchArgs = match serde_json::from_str(args) {
            Ok(a) => a,
            Err(e) => {
                return ToolResult {
                    call_id: String::new(),
                    content: format!("Invalid arguments: {e}"),
                    is_error: true,
                    images: Vec::new(),
                };
            }
        };

        let mut matched = Vec::new();

        // 1. Exact name lookup
        if let Some(names) = args.tool_names {
            for name in names {
                if let Some(item) = self.index.find_by_name(&name) {
                    matched.push(item);
                }
            }
        }

        // 2. Bilingual search
        if let Some(queries) = args.queries {
            let k = args.top_k.unwrap_or(3).clamp(1, 10);
            for q in queries {
                let hits = self.index.search(&q, k);
                matched.extend(hits);
            }
        }

        matched.dedup_by(|a, b| a.full_name == b.full_name);

        if matched.is_empty() {
            let summary = self.index.summary_list();
            let hint = if summary.is_empty() {
                "No deferred tools currently registered in this session.".to_string()
            } else {
                format!("No matching tools found. Available deferred tools:\n{summary}")
            };
            return ToolResult {
                call_id: String::new(),
                content: hint,
                is_error: false,
                images: Vec::new(),
            };
        }

        let tool_names: Vec<String> = matched.iter().map(|item| item.full_name.clone()).collect();

        // 3. Trigger activation via activator if wired
        if let Some(activator) = &self.activator {
            activator.activate_tools(&tool_names).await;
        }

        let mut out = String::from("Successfully discovered and activated the following tools for subsequent turns:\n\n");
        let mut seen_servers = std::collections::HashSet::new();

        for tool in &matched {
            out.push_str(&format!(
                "- **`{}`** (server: `{}`): {}\n  Parameters schema: {}\n",
                tool.full_name, tool.server_name, tool.description.trim(), tool.parameters_schema
            ));

            if let Some(inst) = &tool.server_instructions {
                if seen_servers.insert(tool.server_name.clone()) {
                    out.push_str(&format!(
                        "\n> [Workflow Guidance for `{}`]:\n> {}\n\n",
                        tool.server_name,
                        inst.trim().replace('\n', "\n> ")
                    ));
                }
            } else {
                out.push('\n');
            }
        }

        ToolResult {
            call_id: String::new(),
            content: format!("<!-- jeikcode:protected -->\n{out}"),
            is_error: false,
            images: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::client::McpToolInfo;
    use tokio::sync::Mutex;

    struct MockActivator {
        activated: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait::async_trait]
    impl ToolMountActivator for MockActivator {
        async fn activate_tools(&self, tool_names: &[String]) {
            let mut list = self.activated.lock().await;
            list.extend(tool_names.iter().cloned());
        }
    }

    #[tokio::test]
    async fn test_tool_search_and_mount_activation() {
        let index = Arc::new(McpToolIndex::new());
        let tools = vec![
            McpToolInfo {
                server_name: "playwright".to_string(),
                tool_name: "navigate_url".to_string(),
                description: "Navigate browser to a given URL.".to_string(),
                input_schema: serde_json::json!({"type": "object", "properties": {"url": {"type": "string"}}}),
                read_only: false,
            },
            McpToolInfo {
                server_name: "devtools".to_string(),
                tool_name: "take_screenshot".to_string(),
                description: "Capture screenshot of the screen.".to_string(),
                input_schema: serde_json::json!({"type": "object"}),
                read_only: true,
            }
        ];
        index.update_tools(&tools);

        let activated = Arc::new(Mutex::new(Vec::new()));
        let activator = Arc::new(MockActivator {
            activated: activated.clone(),
        });

        let search_tool = ToolSearchTool::new(index, Some(activator));
        let ctx = ToolContext {
            working_dir: std::path::PathBuf::from("."),
            cancel: tokio_util::sync::CancellationToken::new(),
            progress: jeikcode_kernel::tool::ProgressSink::noop(),
            requester: None,
        };

        // 1. Search in Chinese: "截屏"
        let res = search_tool.execute(r#"{"queries": ["截屏"]}"#, &ctx).await;
        assert!(!res.is_error);
        assert!(res.content.contains("mcp__devtools__take_screenshot"));

        let current_activated = activated.lock().await;
        assert_eq!(current_activated.len(), 1);
        assert_eq!(current_activated[0], "mcp__devtools__take_screenshot");
    }
}

