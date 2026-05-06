use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

#[derive(Debug, Clone, Default, Serialize, Deserialize, IntoParams, ToSchema)]
pub struct PaginationQuery {
    pub page: Option<i64>,
    pub page_size: Option<i64>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
pub struct PaginationMeta {
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
    pub total_pages: i64,
    pub has_next: bool,
    pub has_prev: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct ResolvedPagination {
    pub page: i64,
    pub page_size: i64,
    pub offset: i64,
}

impl PaginationQuery {
    pub fn from_parts(
        page: Option<i64>,
        page_size: Option<i64>,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Self {
        Self {
            page,
            page_size,
            limit,
            offset,
        }
    }

    pub fn resolve(&self, default_page_size: i64, max_page_size: i64) -> ResolvedPagination {
        let page_mode = self.page.is_some() || self.page_size.is_some();
        let page_size = if page_mode {
            self.page_size.unwrap_or(default_page_size)
        } else {
            self.limit.unwrap_or(default_page_size)
        }
        .clamp(1, max_page_size);

        if page_mode {
            let page = self.page.unwrap_or(1).max(1);
            let offset = (page - 1).saturating_mul(page_size);
            ResolvedPagination {
                page,
                page_size,
                offset,
            }
        } else {
            let offset = self.offset.unwrap_or(0).max(0);
            let page = (offset / page_size) + 1;
            ResolvedPagination {
                page,
                page_size,
                offset,
            }
        }
    }
}

impl PaginationMeta {
    pub fn new(total: i64, page: i64, page_size: i64) -> Self {
        let total = total.max(0);
        let total_pages = if total == 0 {
            0
        } else {
            ((total - 1) / page_size) + 1
        };

        Self {
            total,
            page,
            page_size,
            total_pages,
            has_next: total_pages > 0 && page < total_pages,
            has_prev: page > 1 && total_pages > 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{PaginationMeta, PaginationQuery};

    #[test]
    fn resolve_defaults() {
        let resolved = PaginationQuery {
            page: None,
            page_size: None,
            limit: None,
            offset: None,
        }
        .resolve(25, 200);

        assert_eq!(resolved.page, 1);
        assert_eq!(resolved.page_size, 25);
        assert_eq!(resolved.offset, 0);
    }

    #[test]
    fn resolve_clamps_values() {
        let resolved = PaginationQuery {
            page: Some(0),
            page_size: Some(999),
            limit: None,
            offset: None,
        }
        .resolve(25, 200);

        assert_eq!(resolved.page, 1);
        assert_eq!(resolved.page_size, 200);
        assert_eq!(resolved.offset, 0);
    }

    #[test]
    fn page_fields_take_precedence() {
        let resolved = PaginationQuery {
            page: Some(3),
            page_size: Some(20),
            limit: Some(5),
            offset: Some(0),
        }
        .resolve(25, 200);

        assert_eq!(resolved.page, 3);
        assert_eq!(resolved.page_size, 20);
        assert_eq!(resolved.offset, 40);
    }

    #[test]
    fn resolve_alias_mode() {
        let resolved = PaginationQuery {
            page: None,
            page_size: None,
            limit: Some(10),
            offset: Some(30),
        }
        .resolve(25, 200);

        assert_eq!(resolved.page, 4);
        assert_eq!(resolved.page_size, 10);
        assert_eq!(resolved.offset, 30);
    }

    #[test]
    fn meta_for_non_empty_result() {
        let meta = PaginationMeta::new(51, 2, 25);

        assert_eq!(meta.total_pages, 3);
        assert!(meta.has_next);
        assert!(meta.has_prev);
    }

    #[test]
    fn meta_for_empty_result() {
        let meta = PaginationMeta::new(0, 1, 25);

        assert_eq!(meta.total_pages, 0);
        assert!(!meta.has_next);
        assert!(!meta.has_prev);
    }
}
