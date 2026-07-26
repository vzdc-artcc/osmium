-- spec 010 — IP rate limiting.
--
-- Registers the rate-limit bypass permission so it satisfies the FK on
-- access.role_permissions / access.user_permissions and becomes grantable.
--
-- Deliberately granted to NO role here: a valid session or API key does not bypass
-- throttling by default. An admin must explicitly grant `system_rate_limit.update`
-- to the specific accounts (typically internal service accounts) that need it.
--
-- Single-segment name (not `system.rate_limit.update`) to avoid the leaf/parent
-- permission-tree collision with the existing `system.read` — see the
-- SystemRateLimitBypass marker in src/auth/permissions.rs.
insert into access.permissions (name, description)
values (
    'system_rate_limit.update',
    'Bypass per-IP API rate limiting (spec 010). Grant only to trusted service accounts.'
)
on conflict (name) do nothing;
