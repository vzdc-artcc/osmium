# Documenting Endpoints

Every meaningful API change should update both:

- generated OpenAPI coverage
- narrative markdown docs

## Minimum Standard

- route appears in OpenAPI
- request body or query shape is documented
- auth requirements are stated
- permission requirements are stated
- major success and failure cases are described
- collection routes either use the shared pagination contract or explicitly document why they are intentionally bounded and unpaginated

## Drift Prevention

- keep handler `#[utoipa::path]` annotations current
- keep docs page references current
- update README when the developer entry flow changes
