use atom_inference::ChatMessage;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatMetric {
    pub ttft_ms: u64,
    pub tokens_per_second: f64,
    pub completion_tokens: u64,
    pub total_ms: u64,
    pub at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub model_id: Option<String>,
    pub model_label: String,
    pub messages: Vec<ChatMessage>,
    pub metrics: Vec<ChatMetric>,
    pub created_at: String,
    pub updated_at: String,
}

impl Conversation {
    pub fn new(model_id: Option<String>, model_label: String) -> Self {
        let now = chrono::Local::now().to_rfc3339();
        Conversation {
            id: uuid::Uuid::new_v4().to_string(),
            title: "New conversation".into(),
            model_id,
            model_label,
            messages: Vec::new(),
            metrics: Vec::new(),
            created_at: now.clone(),
            updated_at: now,
        }
    }

    /// Title from the first user message, so the sidebar is useful.
    pub fn refresh_title(&mut self) {
        if self.title != "New conversation" {
            return;
        }
        if let Some(first) = self.messages.iter().find(|m| m.role == "user") {
            let text: String = first.content.chars().take(48).collect();
            self.title = if first.content.chars().count() > 48 {
                format!("{text}…")
            } else {
                text
            };
        }
    }

    pub fn to_markdown(&self) -> String {
        let mut out = format!("# {}\n\nModel: {}\nCreated: {}\n\n", self.title, self.model_label, self.created_at);
        for message in &self.messages {
            out.push_str(&format!("## {}\n\n{}\n\n", message.role, message.content));
        }
        out
    }
}
