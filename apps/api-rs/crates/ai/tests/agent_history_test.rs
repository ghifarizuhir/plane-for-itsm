//! Pure tests for `history_prompt` (no DB, no network).

use ai::agent::{history_prompt, HistoryMessage, HISTORY_MESSAGE_LIMIT};

fn message(role: &str, content: &str) -> HistoryMessage {
    HistoryMessage {
        role: role.to_string(),
        content: content.to_string(),
    }
}

#[test]
fn keeps_only_the_last_eight_messages_oldest_first() {
    let mut history = Vec::new();
    for index in 0..10 {
        history.push(message("user", &format!("q{index}")));
        history.push(message("assistant", &format!("a{index}")));
    }
    let prompt = history_prompt("ctx", &history, "new question");
    assert_eq!(HISTORY_MESSAGE_LIMIT, 8);
    // 20 messages -> only the newest 8 survive
    assert!(!prompt.contains("q0"));
    assert!(!prompt.contains("q5"));
    assert!(prompt.contains("q6"));
    assert!(prompt.contains("a9"));
    // oldest-first inside the window
    assert!(prompt.find("q6").unwrap() < prompt.find("a9").unwrap());
    assert!(prompt.contains("User's new question: new question"));
}

#[test]
fn labels_roles_and_marks_empty_history() {
    let prompt = history_prompt("ctx", &[], "hi");
    assert!(prompt.contains("Conversation so far:\n(empty)"));
    assert!(prompt.contains("User's new question: hi"));

    let prompt = history_prompt(
        "ctx",
        &[message("user", "hello"), message("assistant", "world")],
        "again",
    );
    assert!(prompt.contains("User: hello"));
    assert!(prompt.contains("Assistant: world"));
}

#[test]
fn blank_context_is_omitted() {
    let prompt = history_prompt("   ", &[], "hi");
    assert!(prompt.starts_with("Conversation so far:"));
}
