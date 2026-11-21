//! Central Brain planning entry point. Plans are validated before any tool can execute.
use crate::core::{agents::TaskPlan, network::CenAiApi};

#[derive(Clone)]
pub struct CentralBrain {
    api: CenAiApi,
}

impl CentralBrain {
    pub fn new(api: CenAiApi) -> Self { Self { api } }

    pub async fn plan(&self, goal: &str, context: &str) -> Result<TaskPlan, String> {
        let goal = goal.trim();
        if goal.is_empty() || goal.len() > 12_000 { return Err("Task goal is empty or too large.".into()); }
        let context = context.chars().take(12_000).collect::<String>();
        let system = r#"You are CenAI Central Brain's task planner. Return only one valid JSON object matching this schema:
{"id":"task-1","goal":"short goal","stopped":false,"steps":[{"id":1,"role":"planner|researcher|developer|tester|security_reviewer|quality_reviewer","title":"short action","status":"pending","depends_on":[]}]}
Create 1-12 concrete steps, use unique positive integer IDs, list dependencies only on earlier steps, keep steps small and verifiable. Do not invent tools, credentials, permissions, or completed work. Planning does not authorize execution."#;
        let payload = serde_json::json!({
            "model": "cloudflare-gemma-4-26b-a4b-it",
            "messages": [
                {"role":"system","content":system},
                {"role":"user","content":format!("Goal:\n{goal}\n\nRelevant context:\n{context}")}
            ],
            "max_tokens": 1024,
            "stream": false
        });
        let response = self.api.cloud_chat(&payload).await?;
        let raw = response.pointer("/choices/0/message/content")
            .and_then(serde_json::Value::as_str)
            .ok_or("Central Brain returned no plan content")?;
        let plan: TaskPlan = serde_json::from_str(raw.trim())
            .or_else(|_| {
                let start = raw.find('{').ok_or_else(|| serde_json::Error::io(std::io::Error::other("JSON object not found")))?;
                let end = raw.rfind('}').ok_or_else(|| serde_json::Error::io(std::io::Error::other("JSON object not closed")))?;
                serde_json::from_str(&raw[start..=end])
            })
            .map_err(|error| format!("Central Brain produced an invalid plan: {error}"))?;
        plan.validate().map_err(str::to_owned)?;
        if plan.goal.len() > 12_000 || plan.steps.iter().any(|step| step.title.len() > 240) {
            return Err("Central Brain plan exceeds safe limits.".into());
        }
        Ok(plan)
    }
}
