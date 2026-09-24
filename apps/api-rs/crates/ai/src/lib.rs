//! Shared Rig agent runtime: LLM config resolution, prompt preamble, tools,
//! and schedule preset math. Consumed by `crates/api` (HTTP handlers) and
//! `crates/worker` (scheduled runs).

pub mod agent;
pub mod llm;
pub mod tools;

pub use llm::{host_of, resolve_llm_config, response_html, LlmConfig, LlmError};
