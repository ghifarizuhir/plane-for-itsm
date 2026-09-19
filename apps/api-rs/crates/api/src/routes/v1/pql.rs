use sqlx::{Postgres, QueryBuilder};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct V1Pql {
    pub project: Option<uuid::Uuid>,
    pub priority: Option<String>,
    pub state: Option<uuid::Uuid>,
    pub type_id: Option<uuid::Uuid>,
    pub assignee_me: bool,
}

/// The PQL subset this fork can evaluate. The MCP sends `project = "<pid>"`
/// when scoping `count`, and users may add equality filters on the columns
/// this fork has. Anything else is rejected (the MCP turns the 400 into a
/// correctable answer).
fn split_top_level_and(expr: &str) -> Result<Vec<String>, String> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut current = String::new();
    let tokens: Vec<char> = expr.chars().collect();
    let mut i = 0usize;
    while i < tokens.len() {
        let c = tokens[i];
        match c {
            '(' => { depth += 1; current.push(c); i += 1; }
            ')' => { depth -= 1; if depth < 0 { return Err("Unbalanced parentheses in PQL".into()); } current.push(c); i += 1; }
            'A' if depth == 0 && tokens[i..].starts_with(&['A', 'N', 'D']) => {
                let before_ok = i == 0 || tokens[i - 1].is_whitespace();
                let after = i + 3;
                let after_ok = after >= tokens.len() || tokens[after].is_whitespace() || tokens[after] == '(';
                if before_ok && after_ok {
                    parts.push(current.trim().to_string());
                    current.clear();
                    i += 3;
                } else { current.push(c); i += 1; }
            }
            _ => { current.push(c); i += 1; }
        }
    }
    if depth != 0 { return Err("Unbalanced parentheses in PQL".into()); }
    parts.push(current.trim().to_string());
    Ok(parts.into_iter().filter(|p| !p.is_empty()).collect())
}

/// Strips balanced outer parentheses, repeatedly: `((x))` → `x`.
fn strip_outer_parens(mut s: &str) -> &str {
    loop {
        let t = s.trim();
        if !(t.starts_with('(') && t.ends_with(')')) { return t; }
        let mut depth = 0i32;
        let mut balanced = true;
        for (idx, c) in t.char_indices() {
            match c {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 && idx != t.len() - 1 { balanced = false; break; }
                }
                _ => {}
            }
        }
        if balanced { s = &t[1..t.len() - 1]; } else { return t; }
    }
}

fn parse_condition(cond: &str) -> Result<V1Pql, String> {
    let cond = strip_outer_parens(cond);
    let (lhs, rhs) = cond.split_once('=').ok_or_else(|| format!("Unsupported PQL condition: {cond}"))?;
    let lhs = lhs.trim();
    let rhs = rhs.trim();
    let unquote = |v: &str| v.trim().trim_matches('"').to_string();
    let mut out = V1Pql::default();
    match lhs {
        "project" => out.project = Some(uuid::Uuid::parse_str(&unquote(rhs)).map_err(|_| format!("Invalid project id in PQL: {rhs}"))?),
        "priority" => {
            let v = unquote(rhs);
            if !["low", "medium", "high", "urgent", "none"].contains(&v.as_str()) {
                return Err(format!("Invalid priority in PQL: {rhs}"));
            }
            out.priority = Some(v);
        }
        "state" => out.state = Some(uuid::Uuid::parse_str(&unquote(rhs)).map_err(|_| format!("Invalid state id in PQL: {rhs}"))?),
        "type" => out.type_id = Some(uuid::Uuid::parse_str(&unquote(rhs)).map_err(|_| format!("Invalid type id in PQL: {rhs}"))?),
        "assignee" => {
            if rhs == "currentUser()" {
                out.assignee_me = true;
            } else {
                return Err(format!("Unsupported PQL assignee value: {rhs}"));
            }
        }
        other => return Err(format!("Unsupported PQL field: {other}")),
    }
    Ok(out)
}

pub fn parse_v1_pql(raw: &str) -> Result<V1Pql, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() { return Ok(V1Pql::default()); }
    let mut out = V1Pql::default();
    for cond in split_top_level_and(trimmed)? {
        let one = parse_condition(&cond)?;
        if one.project.is_some() { out.project = one.project; }
        if one.priority.is_some() { out.priority = one.priority; }
        if one.state.is_some() { out.state = one.state; }
        if one.type_id.is_some() { out.type_id = one.type_id; }
        if one.assignee_me { out.assignee_me = true; }
    }
    Ok(out)
}

pub fn push_pql_where(qb: &mut QueryBuilder<Postgres>, pql: &V1Pql, user_id: uuid::Uuid) {
    if let Some(p) = pql.project { qb.push(" AND i.project_id = ").push_bind(p); }
    if let Some(pr) = pql.priority.clone() { qb.push(" AND i.priority = ").push_bind(pr); }
    if let Some(st) = pql.state { qb.push(" AND i.state_id = ").push_bind(st); }
    if let Some(t) = pql.type_id { qb.push(" AND i.type_id = ").push_bind(t); }
    if pql.assignee_me {
        qb.push(" AND EXISTS(SELECT 1 FROM issue_assignees ia WHERE ia.issue_id = i.id AND ia.assignee_id = ")
          .push_bind(user_id)
          .push(" AND ia.deleted_at IS NULL)");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_project_scope() {
        let p = parse_v1_pql(r#"project = "11111111-1111-1111-1111-111111111111""#).unwrap();
        assert_eq!(p.project, Some(uuid::Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap()));
        assert_eq!(p.priority, None);
    }

    #[test]
    fn parses_and_combined_scope() {
        let raw = r#"(priority = "urgent") AND project = "11111111-1111-1111-1111-111111111111""#;
        let p = parse_v1_pql(raw).unwrap();
        assert_eq!(p.priority.as_deref(), Some("urgent"));
        assert!(p.project.is_some());
    }

    #[test]
    fn parses_assignee_current_user() {
        let p = parse_v1_pql("assignee = currentUser()").unwrap();
        assert!(p.assignee_me);
    }

    #[test]
    fn rejects_or_and_functions() {
        assert!(parse_v1_pql(r#"priority = "urgent" OR priority = "low""#).is_err());
        assert!(parse_v1_pql("isOverdue()").is_err());
        assert!(parse_v1_pql(r#"priority = "nope""#).is_err());
    }

    #[test]
    fn empty_is_default() {
        assert_eq!(parse_v1_pql("   ").unwrap(), V1Pql::default());
    }
}
