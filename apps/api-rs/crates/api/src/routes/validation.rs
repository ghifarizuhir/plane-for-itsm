//! Shared DRF-style name validation (dipindah dari `routes/workflow.rs`).

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
