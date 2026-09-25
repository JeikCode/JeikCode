//! Bilingual Thesaurus & Dense Vector Semantic Index for MCP Tools.
//!
//! Enables zero-token, deferred tool mounting where MCP tools are discovered
//! on demand via bilingual query expansion + 128-dim dense embedding matching.

use std::collections::HashSet;
use std::sync::{Arc, RwLock};

use crate::codeintel::bilingual_nlp::{
    calculate_text_similarity, compute_dense_embedding, parse_bilingual_query_with_thesaurus,
    split_identifier, DynamicThesaurus,
};
use super::client::McpToolInfo;
use super::tool::mcp_tool_full_name;

/// One searchable MCP tool index entry.
#[derive(Clone, Debug)]
pub struct McpToolIndexItem {
    /// Fully-qualified tool name in runtime, e.g. "mcp__chrome-devtools__take_screenshot"
    pub full_name: String,
    /// Configured server name
    pub server_name: String,
    /// Raw tool name from server
    pub raw_tool_name: String,
    /// Description provided by the server
    pub description: String,
    /// Tokenized representation combining identifier tokens and description
    pub search_tokens_text: String,
    /// Precomputed 128-dimensional dense vector
    pub dense_vector: Vec<f32>,
    /// Parameter JSON schema
    pub parameters_schema: serde_json::Value,
    /// Whether explicitly marked read-only
    pub read_only: bool,
    /// Optional server-wide orchestration instructions (from initialize handshake)
    pub server_instructions: Option<String>,
}

/// In-memory bilingual semantic index for MCP tools.
#[derive(Clone)]
pub struct McpToolIndex {
    items: Arc<RwLock<Vec<McpToolIndexItem>>>,
    server_instructions: Arc<RwLock<std::collections::HashMap<String, String>>>,
    thesaurus: Arc<RwLock<DynamicThesaurus>>,
}

impl Default for McpToolIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl McpToolIndex {
    /// Create a new empty MCP tool index with default bilingual thesaurus.
    pub fn new() -> Self {
        Self {
            items: Arc::new(RwLock::new(Vec::new())),
            server_instructions: Arc::new(RwLock::new(std::collections::HashMap::new())),
            thesaurus: Arc::new(RwLock::new(DynamicThesaurus::new())),
        }
    }

    /// Set server-level usage instructions.
    pub fn set_server_instructions(&self, server: &str, instructions: Option<String>) {
        if let Ok(mut map) = self.server_instructions.write() {
            if let Some(inst) = instructions {
                if !inst.trim().is_empty() {
                    map.insert(server.to_string(), inst);
                } else {
                    map.remove(server);
                }
            } else {
                map.remove(server);
            }
        }
    }

    /// Get server-level usage instructions.
    pub fn get_server_instructions(&self, server: &str) -> Option<String> {
        self.server_instructions.read().ok()?.get(server).cloned()
    }

    /// Create index with an existing shared thesaurus.
    pub fn with_thesaurus(thesaurus: Arc<RwLock<DynamicThesaurus>>) -> Self {
        Self {
            items: Arc::new(RwLock::new(Vec::new())),
            server_instructions: Arc::new(RwLock::new(std::collections::HashMap::new())),
            thesaurus,
        }
    }

    /// Rebuild or append tools into the index.
    pub fn update_tools(&self, tools: &[McpToolInfo]) {
        let mut new_items = Vec::with_capacity(tools.len());

        for tool in tools {
            let full_name = mcp_tool_full_name(&tool.server_name, &tool.tool_name);
            let name_tokens = split_identifier(&tool.tool_name).join(" ");
            let server_tokens = split_identifier(&tool.server_name).join(" ");
            
            // Build rich search text: full_name + raw name + split tokens + description
            let search_tokens_text = format!(
                "{} {} {} {} {}",
                full_name, tool.server_name, server_tokens, name_tokens, tool.description
            );

            let dense_vector = compute_dense_embedding(&search_tokens_text, &HashSet::new());

            let server_inst = self.get_server_instructions(&tool.server_name);
            new_items.push(McpToolIndexItem {
                full_name,
                server_name: tool.server_name.clone(),
                raw_tool_name: tool.tool_name.clone(),
                description: tool.description.clone(),
                search_tokens_text,
                dense_vector,
                parameters_schema: tool.input_schema.clone(),
                read_only: tool.read_only,
                server_instructions: server_inst,
            });
        }

        if let Ok(mut items) = self.items.write() {
            *items = new_items;
        }
    }

    /// Number of indexed tools.
    pub fn len(&self) -> usize {
        self.items.read().map(|i| i.len()).unwrap_or(0)
    }

    /// Whether the index is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Find an item by exact tool name or fully-qualified name.
    pub fn find_by_name(&self, name: &str) -> Option<McpToolIndexItem> {
        let items = self.items.read().ok()?;
        items.iter().find(|item| {
            item.full_name == name 
                || item.raw_tool_name == name 
                || item.full_name.ends_with(&format!("__{}", name))
        }).cloned()
    }

    /// Perform a bilingual lexical + semantic cosine search.
    pub fn search(&self, query: &str, top_k: usize) -> Vec<McpToolIndexItem> {
        let query = query.trim();
        if query.is_empty() {
            return Vec::new();
        }

        let th = match self.thesaurus.read() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        let query_tokens = parse_bilingual_query_with_thesaurus(query, &th);
        drop(th);

        let items = match self.items.read() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };

        let mut scored: Vec<(f64, &McpToolIndexItem)> = items
            .iter()
            .map(|item| {
                let score = calculate_text_similarity(&query_tokens, &item.search_tokens_text);
                (score, item)
            })
            .filter(|(score, _)| *score >= 12.0)
            .collect();

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored.into_iter().take(top_k).map(|(_, item)| item.clone()).collect()
    }

    /// Returns a compact summary string of all available deferred tools for the prompt.
    pub fn summary_list(&self) -> String {
        let items = match self.items.read() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        if items.is_empty() {
            return String::new();
        }

        let mut out = String::new();
        for item in items.iter() {
            let desc_brief = item.description.lines().next().unwrap_or("").trim();
            out.push_str(&format!("- {}: {}\n", item.full_name, desc_brief));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bilingual_mcp_search() {
        let index = McpToolIndex::new();
        let tools = vec![
            McpToolInfo {
                server_name: "devtools".to_string(),
                tool_name: "take_screenshot".to_string(),
                description: "Capture a screenshot of the active browser viewport or element.".to_string(),
                input_schema: serde_json::json!({}),
                read_only: true,
            },
            McpToolInfo {
                server_name: "mail".to_string(),
                tool_name: "send_mail".to_string(),
                description: "Send an email message to specified recipients.".to_string(),
                input_schema: serde_json::json!({}),
                read_only: false,
            },
            McpToolInfo {
                server_name: "db".to_string(),
                tool_name: "exec_sql".to_string(),
                description: "Execute a raw SQL query against PostgreSQL database.".to_string(),
                input_schema: serde_json::json!({}),
                read_only: false,
            },
        ];

        index.update_tools(&tools);

        // 1. Search in Chinese: "截屏" -> should hit take_screenshot
        let res_screenshot = index.search("截屏", 1);
        assert!(!res_screenshot.is_empty(), "Chinese query '截屏' must hit");
        assert_eq!(res_screenshot[0].raw_tool_name, "take_screenshot");

        // 2. Search in Chinese: "发邮件" -> should hit send_mail
        let res_mail = index.search("发邮件", 1);
        assert!(!res_mail.is_empty(), "Chinese query '发邮件' must hit");
        assert_eq!(res_mail[0].raw_tool_name, "send_mail");

        // 3. Search in Chinese: "执行数据库查询" -> should hit exec_sql
        let res_sql = index.search("执行数据库查询", 1);
        assert!(!res_sql.is_empty(), "Chinese query '执行数据库查询' must hit");
        assert_eq!(res_sql[0].raw_tool_name, "exec_sql");

        // 4. Search in English: "screenshot"
        let res_en = index.search("screenshot", 1);
        assert_eq!(res_en[0].raw_tool_name, "take_screenshot");
    }
}
