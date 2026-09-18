use serde::Deserialize;

use crate::routes::issue_common::{parse_cursor, parse_per_page, DetailCursor, PageWindow, page_window};

/// `?cursor=&per_page=` params shared by v1 list endpoints.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct PageParams {
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default)]
    pub per_page: Option<String>,
}

impl PageParams {
    /// Byte-exact DRF parse: per_page default/max 1000, cursor `value:page:is_prev`.
    pub(crate) fn resolve(&self) -> Result<(i64, DetailCursor), String> {
        let per_page = parse_per_page(self.per_page.as_deref())?;
        let raw = self.cursor.clone().unwrap_or_else(|| format!("{per_page}:0:0"));
        let cursor = parse_cursor(&raw)?;
        Ok((per_page, cursor))
    }
}

/// Offset window for a parsed cursor (re-exported so handlers don't import
/// `issue_common` directly). `BeyondEnd` renders an empty page.
pub(crate) fn window_for(cursor: DetailCursor, limit: i64) -> Result<PageWindow, ()> {
    page_window(cursor.page, limit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_params_default_matches_django() {
        let (per_page, cursor) = PageParams::default().resolve().unwrap();
        assert_eq!(per_page, 1000);
        assert_eq!(cursor.page, 0);
        assert!(!cursor.is_prev);
    }
}
