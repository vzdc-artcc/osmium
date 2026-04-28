# Adding Routes

When adding a new route:

1. Add or update the request/response DTO in the correct `src/models/*` module.
2. Add repo/query helpers if the route hits the database.
3. Implement the handler.
4. Register the route in `src/router.rs`.
5. Add `#[utoipa::path(...)]` metadata.
6. Add the DTO to the OpenAPI components list if needed.
7. Update the relevant markdown docs page.
8. Add or update tests.

## Rule

A route is not complete if its docs were not updated in the same change.
