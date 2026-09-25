//! Model-facing tool to batch load and execute deferred MCP tools.
//!
//! Handles initial loading + execution into active tools list, as well as fallback
//! routing for tools that are already active in the catalog.

use std::sync::Arc;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use jeikcode_kernel::tool::{Tool, ToolContext, ToolRegistry, ToolResult};
use crate::mcp::McpRegistry;

/// Callback interface for dynamically mounting tools into active execution.
#[async_trait]
pub trait ToolMountActivator: Send + Sync {
    /// Request that tools with the given names be mounted into the active catalog.
    async fn activate_tools(&self, tool_names: &[String]);
    /// Check whether a tool is already active in the catalog.
    fn is_tool_active(&self, tool_name: &str) -> bool;
}

/// An invocation entry in the batch.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ToolLoadInvocation {
    /// Fully qualified tool name, e.g. "mcp__chrome-devtools__take_screenshot"
    pub tool_name: String,
    /// Arguments payload for the tool. Must be a valid JSON object.
    /// If the tool does not require parameters, pass an empty JSON object: `{}`.
    #[serde(default)]
    pub arguments: Value,
}

#[derive(Deserialize)]
struct BatchArgs {
    invocations: Vec<ToolLoadInvocation>,
}

/// Dynamic batch loader and router tool.
pub struct ToolBatchLoadAndExecTool {
    tool_registry: ToolRegistry,
    mcp_registries: Vec<Arc<McpRegistry>>,
    activator: Option<Arc<dyn ToolMountActivator>>,
}

impl ToolBatchLoadAndExecTool {
    pub fn new(
        tool_registry: ToolRegistry,
        mcp_registries: Vec<Arc<McpRegistry>>,
        activator: Option<Arc<dyn ToolMountActivator>>,
    ) -> Self {
        Self {
            tool_registry,
            mcp_registries,
            activator,
        }
    }
}

#[async_trait]
impl Tool for ToolBatchLoadAndExecTool {
    fn name(&self) -> &str {
        "tool_batch_load_and_exec"
    }

    fn description(&self) -> &str {
        "Load and immediately execute one or more tools discovered via `tool_search`. \
        Use this tool when you need to call a tool that is not yet present in your active tools list. \
        Parameters: an array of `invocations`, each containing `tool_name` (exact name) and `arguments` (JSON object; \
        if the tool requires no parameters, you MUST provide an empty object `{}`). \
        Once executed, the tool is mounted to your active tools list for all subsequent turns. \
        If called for a tool already mounted in your tools list, it automatically routes execution to the active instance \
        and reminds you to call it directly as a native tool."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "invocations": {
                    "type": "array",
                    "description": "List of tool invocations to load and execute.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "tool_name": {
                                "type": "string",
                                "description": "Exact name of the tool (e.g. 'mcp__chrome-devtools__take_screenshot')."
                            },
                            "arguments": {
                                "type": "object",
                                "description": "Arguments JSON object for the tool. Required; if the tool takes no parameters, pass `{}`."
                            }
                        },
                        "required": ["tool_name", "arguments"]
                    }
                }
            },
            "required": ["invocations"]
        })
    }

    async fn execute(&self, args: &str, ctx: &ToolContext) -> ToolResult {
        let batch_args: BatchArgs = match serde_json::from_str(args) {
            Ok(b) => b,
            Err(e) => {
                return ToolResult {
                    call_id: String::new(),
                    content: format!("Invalid arguments for tool_batch_load_and_exec: {e}. 'invocations' must be an array of objects with 'tool_name' and 'arguments' object."),
                    is_error: true,
                    images: Vec::new(),
                };
            }
        };

        if batch_args.invocations.is_empty() {
            return ToolResult {
                call_id: String::new(),
                content: "No invocations provided in tool_batch_load_and_exec.".to_string(),
                is_error: true,
                images: Vec::new(),
            };
        }

        let mut results_text = Vec::new();
        let mut all_images = Vec::new();
        let mut had_error = false;
        let mut newly_activated = Vec::new();

        for (idx, inv) in batch_args.invocations.into_iter().enumerate() {
            let tool_name = inv.tool_name.trim();
            let is_already_active = self.activator.as_ref().map_or(false, |act| act.is_tool_active(tool_name));

            // Ensure arguments are valid object
            let args_value = if inv.arguments.is_object() {
                inv.arguments
            } else {
                serde_json::json!({})
            };
            let args_str = serde_json::to_string(&args_value).unwrap_or_else(|_| "{}".to_string());

            let mut executed_result = None;

            // 1. Try resolving from active ToolRegistry first (supports both native & already registered MCP adapters)
            let registered_tool = self.tool_registry.mount(&[tool_name]).get(tool_name);
            if let Some(tool) = registered_tool {
                let res = tool.execute(&args_str, ctx).await;
                executed_result = Some(res);
            } else {
                // 2. Direct route via McpRegistry if not yet mounted in ToolRegistry
                for mcp_reg in &self.mcp_registries {
                    if let Some((server, actual_tool)) = mcp_reg.split_tool_name(tool_name).await {
                        let res = match mcp_reg.call_tool_with_images(&server, &actual_tool, args_value.clone()).await {
                            Ok((text, imgs)) => ToolResult {
                                call_id: String::new(),
                                content: text,
                                is_error: false,
                                images: imgs,
                            },
                            Err(e) => ToolResult {
                                call_id: String::new(),
                                content: format!("MCP execution error: {e}"),
                                is_error: true,
                                images: Vec::new(),
                            },
                        };
                        executed_result = Some(res);
                        break;
                    }
                }
            }

            // Record for mount activation if not already active
            if !is_already_active {
                newly_activated.push(tool_name.to_string());
            }

            match executed_result {
                Some(res) => {
                    if res.is_error {
                        had_error = true;
                    }
                    all_images.extend(res.images);

                    let mut section = format!("### Result for `{}` (invocation #{}) ###\n", tool_name, idx + 1);
                    if is_already_active {
                        section.push_str("> 💡 提示：检测到当前工具已加载到原生 tools 列表，后续轮次请直接使用原生工具调用方式调用。\n\n");
                    } else {
                        section.push_str("> ✅ 工具已成功加载并挂载到会话，后续轮次可直接作为原生工具调用。\n\n");
                    }
                    section.push_str(&res.content);
                    results_text.push(section);
                }
                None => {
                    had_error = true;
                    results_text.push(format!(
                        "### Error for `{}` (invocation #{}) ###\nTool not found in registered MCP servers or active catalog. Please verify the tool name using `tool_search`.",
                        tool_name, idx + 1
                    ));
                }
            }
        }

        // Trigger dynamic mounting for all newly referenced tools
        if let Some(activator) = &self.activator {
            if !newly_activated.is_empty() {
                activator.activate_tools(&newly_activated).await;
            }
        }

        ToolResult {
            call_id: String::new(),
            content: format!("<!-- jeikcode:protected -->\n{}", results_text.join("\n\n---\n\n")),
            is_error: had_error,
            images: all_images,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jeikcode_kernel::tool::{ProgressSink, ToolContext};
    use tokio::sync::Mutex;

    struct MockTool {
        name: String,
    }

    #[async_trait::async_trait]
    impl Tool for MockTool {
        fn name(&self) -> &str {
            &self.name
        }
        fn description(&self) -> &str {
            "mock tool"
        }
        fn parameters_schema(&self) -> Value {
            serde_json::json!({})
        }
        async fn execute(&self, args: &str, _ctx: &ToolContext) -> ToolResult {
            ToolResult {
                call_id: String::new(),
                content: format!("executed with args: {}", args),
                is_error: false,
                images: Vec::new(),
            }
        }
    }

    struct MockActivator {
        activated: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait::async_trait]
    impl ToolMountActivator for MockActivator {
        async fn activate_tools(&self, tool_names: &[String]) {
            let mut list = self.activated.lock().await;
            list.extend(tool_names.iter().cloned());
        }

        fn is_tool_active(&self, tool_name: &str) -> bool {
            tool_name == "mcp__test__already_active"
        }
    }

    #[tokio::test]
    async fn test_tool_batch_load_and_exec() {
        let mut reg = ToolRegistry::new();
        reg.register(Arc::new(MockTool {
            name: "mcp__test__echo".to_string(),
        }));
        reg.register(Arc::new(MockTool {
            name: "mcp__test__already_active".to_string(),
        }));

        let activated = Arc::new(Mutex::new(Vec::new()));
        let activator = Arc::new(MockActivator {
            activated: activated.clone(),
        });

        let batch_tool = ToolBatchLoadAndExecTool::new(
            reg,
            Vec::new(),
            Some(activator),
        );

        let ctx = ToolContext {
            working_dir: std::path::PathBuf::from("."),
            cancel: tokio_util::sync::CancellationToken::new(),
            progress: ProgressSink::noop(),
            requester: None,
        };

        // 1. Invoke two tools: one newly mounted, one already active
        let payload = r#"{
            "invocations": [
                { "tool_name": "mcp__test__echo", "arguments": { "msg": "hello" } },
                { "tool_name": "mcp__test__already_active", "arguments": {} }
            ]
        }"#;

        let res = batch_tool.execute(payload, &ctx).await;
        assert!(!res.is_error);
        assert!(res.content.contains("executed with args: {\"msg\":\"hello\"}"));
        assert!(res.content.contains("工具已成功加载并挂载到会话"));
        assert!(res.content.contains("检测到当前工具已加载到原生 tools 列表"));

        let list = activated.lock().await;
        assert_eq!(*list, vec!["mcp__test__echo".to_string()]);
    }
}

