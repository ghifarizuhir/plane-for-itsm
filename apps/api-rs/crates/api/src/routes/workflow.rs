//! Workflows workspace-level (state + transisi) dan materialization state ke
//! project. Spec: docs/superpowers/specs/2026-09-25-work-item-types-workflows-design.md

use uuid::Uuid;

/// Group valid untuk workflow state; `triage` bukan bagian workflow
/// (`StateGroup` di `apps/api/plane/db/models/state.py:14-20`).
/// Alias ke `state::ALLOWED_GROUPS` agar daftarnya tidak pernah menyimpang.
pub const STATE_GROUPS: &[&str] = super::state::ALLOWED_GROUPS;

/// Validasi nama ala DRF (required, <=255). Mengembalikan nama ter-trim.
pub fn validate_name(name: &str, field: &str) -> Result<String, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(format!("{field} is required"));
    }
    if trimmed.chars().count() > 255 {
        return Err(format!("Ensure {field} has no more than 255 characters."));
    }
    Ok(trimmed.to_string())
}

pub fn validate_state_group(group: &str) -> Result<(), String> {
    if STATE_GROUPS.contains(&group) {
        Ok(())
    } else {
        Err(format!("\"{group}\" is not a valid choice."))
    }
}

/// Target state (mirror) yang diizinkan dari `current_state`, dihitung murni
/// dari pasangan `(state_id, workflow_state_id)` dan daftar transisi
/// `(from_workflow_state_id, to_workflow_state_id)`. Urutan output mengikuti
/// urutan `mirror_pairs` dan tidak di-sort lebih lanjut.
pub fn allowed_target_state_ids(
    current_state: Uuid,
    mirror_pairs: &[(Uuid, Uuid)],
    transitions: &[(Uuid, Uuid)],
) -> Vec<Uuid> {
    let Some(current_workflow_state) = mirror_pairs
        .iter()
        .find(|(state_id, _)| *state_id == current_state)
        .map(|(_, workflow_state_id)| *workflow_state_id)
    else {
        return Vec::new();
    };
    let allowed_workflow_states: Vec<Uuid> = transitions
        .iter()
        .filter(|(from, _)| *from == current_workflow_state)
        .map(|(_, to)| *to)
        .collect();
    mirror_pairs
        .iter()
        .filter(|(_, workflow_state_id)| allowed_workflow_states.contains(workflow_state_id))
        .map(|(state_id, _)| *state_id)
        .collect()
}
