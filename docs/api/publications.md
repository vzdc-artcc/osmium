# Publications API

## Purpose

Expose the public downloads/publications catalog and provide staff CRUD for publication metadata and categories.

## Public Routes

- `GET /api/v1/publications`
- `GET /api/v1/publications/{publication_id}`
- `GET /api/v1/publications/categories`

## Admin Routes

- `GET /api/v1/admin/publications`
- `GET /api/v1/admin/publications/{publication_id}`
- `POST /api/v1/admin/publications`
- `PATCH /api/v1/admin/publications/{publication_id}`
- `DELETE /api/v1/admin/publications/{publication_id}`
- `GET /api/v1/admin/publications/categories`
- `POST /api/v1/admin/publications/categories`
- `PATCH /api/v1/admin/publications/categories/{category_id}`
- `DELETE /api/v1/admin/publications/categories/{category_id}`

## Visibility Rules

Public publication reads return only rows where:

- `is_public = true`
- `status = published`
- `effective_at <= now()`
- linked file asset is public
- linked file asset is not soft-deleted

## Notes

- publication records store metadata and linked `file_id`; bytes remain in `media.file_assets`
- public responses include `cdn_url` in the form `/cdn/{file_id}`
- admin publication and category routes require `web.update`
- publication status is constrained to `draft`, `published`, and `archived`
