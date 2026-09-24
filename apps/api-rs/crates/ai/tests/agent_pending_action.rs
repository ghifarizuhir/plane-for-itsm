//! `pending_action` extraction from a tool trace.

use ai::agent::{new_trace, pending_action, record};
use ai::tools::{CreateSchedule, CreateScheduleArgs};
use rig::tool::{Tool, ToolContext};
use serde_json::json;

#[tokio::test]
async fn pending_action_returns_last_create_schedule_proposal() {
    let trace = new_trace();
    record(&trace, "list_projects", &json!({}));
    let tool = CreateSchedule {
        trace: trace.clone(),
    };
    tool.call(
        &mut ToolContext::new(),
        CreateScheduleArgs {
            name: "Weekly backlog".to_string(),
            prompt: "Summarize backlog".to_string(),
            frequency: "weekly".to_string(),
            time: Some("09:00".to_string()),
            day_of_week: Some(1),
            day_of_month: None,
            timezone: Some("Asia/Jakarta".to_string()),
        },
    )
    .await
    .unwrap();

    let action = pending_action(&trace).expect("proposal recorded");
    assert_eq!(action["kind"], json!("create_schedule"));
    assert_eq!(action["proposal"]["name"], json!("Weekly backlog"));
    assert_eq!(action["proposal"]["day_of_week"], json!(1));
}

#[test]
fn pending_action_is_none_without_proposal() {
    let trace = new_trace();
    record(&trace, "count_work_items", &json!({"priority": "urgent"}));
    assert!(pending_action(&trace).is_none());
}
