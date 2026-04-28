# Code Organization

Current key areas:

- `src/auth/`: auth context, ACL, middleware, provider auth
- `src/handlers/`: HTTP layer
- `src/models/`: request/response and row contract types
- `src/repos/`: DB query helpers
- `src/jobs/`: background job logic
- `src/state.rs`: shared app state
- `src/router.rs`: route composition
- `src/docs.rs`: docs registry and OpenAPI wiring

Preferred direction:

- keep handlers thin
- move SQL into repos
- keep DTOs in domain model modules
- keep docs and OpenAPI tied to the real route surface
