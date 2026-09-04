use serde::Deserialize;

/// Mirrors `plane/app/serializers/user.py:UserSerializer`
/// validate_first_name / validate_last_name (no URL).
#[derive(Debug, Clone, Deserialize, Default)]
pub struct UpdateUser {
    pub first_name: Option<String>,
    pub last_name: Option<String>,
}

fn contains_url(value: &str) -> bool {
    if value.len() > 1000 {
        return false;
    }
    let lower = value.to_lowercase();
    lower.contains("http://") || lower.contains("https://") || lower.contains("www.")
}

pub fn validate_update(body: &UpdateUser) -> Result<(), String> {
    if let Some(first) = &body.first_name {
        if contains_url(first) {
            return Err("First name cannot contain a URL.".to_string());
        }
    }
    if let Some(last) = &body.last_name {
        if contains_url(last) {
            return Err("Last name cannot contain a URL.".to_string());
        }
    }
    Ok(())
}
