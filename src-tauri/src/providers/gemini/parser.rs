use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiSession {
    #[serde(rename = "sessionId")]
    pub session_id: String,

    #[serde(rename = "projectHash")]
    pub project_hash: String,

    #[serde(rename = "startTime")]
    pub start_time: String,

    #[serde(rename = "lastUpdated")]
    pub last_updated: String,

    pub messages: Vec<GeminiMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<serde_json::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Vec<serde_json::Value>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,

    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>, // Preserve all other Gemini-specific fields
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiMessage {
    pub id: String,
    pub timestamp: String,

    #[serde(rename = "type")]
    pub message_type: String, // "user" or "gemini"

    pub content: String,

    #[serde(rename = "toolCalls", skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub thoughts: Option<Vec<Thought>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens: Option<TokenUsage>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thought {
    pub subject: String,
    pub description: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input: u32,
    pub output: u32,
    pub cached: u32,
    pub thoughts: u32,
    pub tool: u32,
    pub total: u32,
}

impl GeminiSession {
    /// Parse a Gemini session from JSON string
    pub fn from_json(json_str: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json_str)
    }

    /// Get the total number of messages in the session
    #[allow(dead_code)]
    pub fn message_count(&self) -> usize {
        self.messages.len()
    }

    /// Get user messages
    #[allow(dead_code)]
    pub fn user_messages(&self) -> Vec<&GeminiMessage> {
        self.messages
            .iter()
            .filter(|m| m.message_type == "user")
            .collect()
    }

    /// Get gemini (assistant) messages
    #[allow(dead_code)]
    pub fn gemini_messages(&self) -> Vec<&GeminiMessage> {
        self.messages
            .iter()
            .filter(|m| m.message_type == "gemini")
            .collect()
    }

    /// Get total thoughts across all messages
    #[allow(dead_code)]
    pub fn total_thoughts(&self) -> usize {
        self.messages
            .iter()
            .filter_map(|m| m.thoughts.as_ref())
            .map(|thoughts| thoughts.len())
            .sum()
    }

    /// Check if session has any thoughts
    #[allow(dead_code)]
    pub fn has_thoughts(&self) -> bool {
        self.messages
            .iter()
            .any(|m| m.thoughts.is_some() && !m.thoughts.as_ref().unwrap().is_empty())
    }

    /// Calculate total tokens across all messages
    #[allow(dead_code)]
    pub fn total_tokens(&self) -> TokenSummary {
        let mut summary = TokenSummary::default();

        for message in &self.messages {
            if let Some(tokens) = &message.tokens {
                summary.input += tokens.input;
                summary.output += tokens.output;
                summary.cached += tokens.cached;
                summary.thoughts += tokens.thoughts;
                summary.tool += tokens.tool;
                summary.total += tokens.total;
            }
        }

        summary
    }
}

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct TokenSummary {
    pub input: u32,
    pub output: u32,
    pub cached: u32,
    pub thoughts: u32,
    pub tool: u32,
    pub total: u32,
}

impl TokenSummary {
    /// Calculate cache hit rate (0.0 - 1.0)
    #[allow(dead_code)]
    pub fn cache_hit_rate(&self) -> f32 {
        let total_input = self.input + self.cached;
        if total_input > 0 {
            self.cached as f32 / total_input as f32
        } else {
            0.0
        }
    }

    /// Calculate thinking overhead (thoughts / output)
    #[allow(dead_code)]
    pub fn thinking_overhead(&self) -> f32 {
        if self.output > 0 {
            self.thoughts as f32 / self.output as f32
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_gemini_session() {
        let json = r#"{
            "sessionId": "test-123",
            "projectHash": "abc123",
            "startTime": "2025-10-11T00:00:00Z",
            "lastUpdated": "2025-10-11T00:01:00Z",
            "messages": [
                {
                    "id": "msg-1",
                    "timestamp": "2025-10-11T00:00:00Z",
                    "type": "user",
                    "content": "Hello"
                },
                {
                    "id": "msg-2",
                    "timestamp": "2025-10-11T00:00:05Z",
                    "type": "gemini",
                    "content": "Hi there!",
                    "thoughts": [
                        {
                            "subject": "Greeting",
                            "description": "User is saying hello",
                            "timestamp": "2025-10-11T00:00:01Z"
                        }
                    ],
                    "tokens": {
                        "input": 100,
                        "output": 50,
                        "cached": 20,
                        "thoughts": 10,
                        "tool": 0,
                        "total": 160
                    },
                    "model": "gemini-2.5-pro"
                }
            ]
        }"#;

        let session = GeminiSession::from_json(json).unwrap();

        assert_eq!(session.session_id, "test-123");
        assert_eq!(session.project_hash, "abc123");
        assert_eq!(session.messages.len(), 2);
        assert_eq!(session.user_messages().len(), 1);
        assert_eq!(session.gemini_messages().len(), 1);
        assert!(session.has_thoughts());
        assert_eq!(session.total_thoughts(), 1);

        let tokens = session.total_tokens();
        assert_eq!(tokens.total, 160);
        assert_eq!(tokens.cached, 20);
        assert_eq!(tokens.thoughts, 10);
        assert!(tokens.cache_hit_rate() > 0.0);
        assert!(tokens.thinking_overhead() > 0.0);
    }
}
