//! `pending_action` extraction from a tool trace.

use ai::agent::{new_trace, pending_action, record};
use ai::tools::{CreateSchedule, CreateScheduleArgs, UpdateSchedule, UpdateScheduleArgs};
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
            description: "Summarize the backlog every week".to_string(),
            how_to: vec!["Count backlog items".to_string()],
            tools: vec!["count_work_items".to_string()],
            expected_output: "A short summary".to_string(),
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
    assert_eq!(action["proposal"]["tools"][0], json!("count_work_items"));
}

#[test]
fn pending_action_is_none_without_proposal() {
    let trace = new_trace();
    record(&trace, "count_work_items", &json!({"priority": "urgent"}));
    assert!(pending_action(&trace).is_none());
}

#[tokio::test]
async fn pending_action_returns_the_last_schedule_action() {
    let trace = new_trace();
    record(&trace, "list_projects", &json!({}));

    let create = CreateSchedule {
        trace: trace.clone(),
    };
    create
        .call(
            &mut ToolContext::new(),
            CreateScheduleArgs {
                name: "Daily".to_string(),
                description: "Report".to_string(),
                how_to: vec!["Count".to_string()],
                tools: vec!["count_work_items".to_string()],
                expected_output: "Summary".to_string(),
                frequency: "daily".to_string(),
                time: None,
                day_of_week: None,
                day_of_month: None,
                timezone: None,
            },
        )
        .await
        .unwrap();

    let update = UpdateSchedule {
        trace: trace.clone(),
    };
    update
        .call(
            &mut ToolContext::new(),
            UpdateScheduleArgs {
                schedule_id: "00000000-0000-0000-0000-000000000001".to_string(),
                frequency: Some("weekly".to_string()),
                day_of_week: Some(2),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    let action = pending_action(&trace).expect("proposal recorded");
    assert_eq!(action["kind"], json!("update_schedule"));
    assert_eq!(
        action["proposal"]["schedule_id"],
        json!("00000000-0000-0000-0000-000000000001")
    );
    assert_eq!(action["proposal"]["changes"]["day_of_week"], json!(2));
}
