//! Registry of statically-known permission marker types for [`RequirePermission`].
//!
//! Each entry pairs a marker type name with the `(segments, action)` it enforces. Add
//! entries here as handlers migrate from manual `ensure_permission(...)` calls to
//! `RequirePermission<P>` — see `specs/004-structural-permission-enforcement.md`.
//!
//! [`RequirePermission`]: crate::auth::require_permission::RequirePermission

use crate::auth::require_permission::permission;

// feedback (also used by incidents.rs, which shares the "feedback.items" permission
// namespace at the ACL layer)
permission!(FeedbackItemsCreate, ["feedback", "items"], Create);
permission!(FeedbackItemsDecide, ["feedback", "items"], Decide);
// "items_self" (not ["feedback", "items", "self"]) deliberately avoids being
// a path-prefix collision with ["feedback", "items"] above — the JSON
// permission tree (src/auth/acl.rs) represents a segment as either a leaf
// action-array or a parent of further segments, never both, so a 3-segment
// permission nested one level under a 2-segment one at the same prefix
// would silently clobber (or be clobbered by) it. See migration 0048.
permission!(FeedbackItemsSelfRead, ["feedback", "items_self"], Read);

// files
permission!(FilesAuditRead, ["files", "audit"], Read);
permission!(FilesAssetsRead, ["files", "assets"], Read);
permission!(FilesAssetsCreate, ["files", "assets"], Create);
permission!(FilesContentCreate, ["files", "content"], Create);
permission!(FilesContentUpdate, ["files", "content"], Update);
permission!(FilesAssetsDelete, ["files", "assets"], Delete);

// publications
permission!(
    PublicationsCategoriesRead,
    ["publications", "categories"],
    Read
);
permission!(
    PublicationsCategoriesCreate,
    ["publications", "categories"],
    Create
);
permission!(
    PublicationsCategoriesUpdate,
    ["publications", "categories"],
    Update
);
permission!(
    PublicationsCategoriesDelete,
    ["publications", "categories"],
    Delete
);
permission!(PublicationsItemsRead, ["publications", "items"], Read);
permission!(PublicationsItemsCreate, ["publications", "items"], Create);
permission!(PublicationsItemsUpdate, ["publications", "items"], Update);
permission!(PublicationsItemsDelete, ["publications", "items"], Delete);

// events (EventsItemsUpdate is shared by events.rs::update_event and every
// event_ops.rs mutation, which all gate on the same "events.items.update" permission)
permission!(EventsItemsCreate, ["events", "items"], Create);
permission!(EventsItemsUpdate, ["events", "items"], Update);
permission!(EventsItemsDelete, ["events", "items"], Delete);
permission!(
    EventsPositionsSelfRequest,
    ["events", "positions", "self"],
    Request
);
permission!(EventsPositionsAssign, ["events", "positions"], Assign);
permission!(EventsPositionsDelete, ["events", "positions"], Delete);
permission!(EventsPositionsPublish, ["events", "positions"], Publish);
// named event-position-preset bundles (events.event_position_presets) — admin-only
// tool, unlike the public events/positions themselves.
permission!(EventsPresetsRead, ["events", "presets"], Read);
permission!(EventsPresetsCreate, ["events", "presets"], Create);
permission!(EventsPresetsUpdate, ["events", "presets"], Update);
permission!(EventsPresetsDelete, ["events", "presets"], Delete);
// ops-plan file attachments (events.ops_plan_files) — listing is public (the
// published ops-plan page shows them), mutation is staff-only.
permission!(
    EventsOpsPlanFilesCreate,
    ["events", "ops_plan_files"],
    Create
);
permission!(
    EventsOpsPlanFilesDelete,
    ["events", "ops_plan_files"],
    Delete
);

// integrations (IntegrationsStatsUpdate is also the permission behind the
// `ensure_integrations_manage` helper in handlers/integrations.rs, which additionally
// restricts callers to authenticated users only, unlike this extractor)
permission!(IntegrationsStatsUpdate, ["integrations", "stats"], Update);

// auth (shared by the "/me/discord" self-service endpoints)
permission!(AuthProfileRead, ["auth", "profile"], Read);

// training admin (training_admin.rs's progression/performance-indicator CRUD all gate on
// the "training.lessons" permission namespace; also reused by training.rs once migrated)
permission!(TrainingLessonsRead, ["training", "lessons"], Read);
permission!(TrainingLessonsUpdate, ["training", "lessons"], Update);

// dossier entries (training_admin.rs's get_user_dossier/create_dossier_entry).
// Base read access is still gated by TrainingLessonsRead/AuthProfileRead
// (data-dependent self-vs-other, not a single RequirePermission<P> case).
// Who may write an entry at all is a static RequirePermission<P> case:
permission!(TrainingDossierCreate, ["training", "dossier"], Create);
// Who may see entries marked confidential is also data-dependent (checked
// alongside the self-vs-other read gate above, not on its own) — no marker
// struct needed since it's never used as a RequirePermission<P> extractor;
// see get_user_dossier's manual PermissionPath::from_segments(["training",
// "dossier_confidential"], Read) check.

// stats.rs (statistics prefixes admin config; the other stats.rs handlers are
// intentionally public/unauthenticated and gate nothing)
permission!(StatsPrefixesRead, ["stats", "prefixes"], Read);
permission!(StatsPrefixesUpdate, ["stats", "prefixes"], Update);

// broadcasts.rs (admin CRUD; self-service seen/agree endpoints reuse
// AuthProfileRead/AuthProfileUpdate like org.rs's "/loa/me" does)
permission!(WebBroadcastsRead, ["web", "broadcasts"], Read);
permission!(WebBroadcastsCreate, ["web", "broadcasts"], Create);
permission!(WebBroadcastsUpdate, ["web", "broadcasts"], Update);
permission!(WebBroadcastsDelete, ["web", "broadcasts"], Delete);

// welcome_messages.rs (admin CRUD on the home/visitor text; self-service get/ack
// reuse AuthProfileRead/AuthProfileUpdate like broadcasts.rs and org.rs's "/loa/me" do)
permission!(WebWelcomeMessagesRead, ["web", "welcome_messages"], Read);
permission!(
    WebWelcomeMessagesUpdate,
    ["web", "welcome_messages"],
    Update
);

// org.rs (loas, solo_certs, staffing_requests, sua_requests, controller_lifecycle, jobs)
permission!(AuthProfileUpdate, ["auth", "profile"], Update);
permission!(UsersDirectoryRead, ["users", "directory"], Read);
permission!(
    UsersControllerStatusUpdate,
    ["users", "controller_status"],
    Update
);
// Roster purge (transition to NONE + VATUSA roster removal + cleanup) is a
// far more destructive, irreversible action than an ordinary status change
// (e.g. approving a visitor), so it gets its own dedicated permission rather
// than reusing UsersControllerStatusUpdate — matches the live site's
// ATM/DATM-only gate on the Purge Assistant, narrower than STAFF-wide.
permission!(
    UsersControllerStatusDelete,
    ["users", "controller_status"],
    Delete
);
// Display-only staff position tags (ATM/DATM/TA/EC/WM/FE/AEC/AWM/AFE/EP/
// TMU/FC/INS/MTR) — never grant permissions themselves, distinct from
// access.user_roles. Own permission pair so it can be granted without also
// granting controller-status/roster admin capabilities.
permission!(UsersStaffPositionsRead, ["users", "staff_positions"], Read);
permission!(
    UsersStaffPositionsUpdate,
    ["users", "staff_positions"],
    Update
);
permission!(SystemRead, ["system"], Read);
// staffing requests get their own dedicated permissions (rather than reusing the
// Users-domain ones above) so EVENT_STAFF can be granted admin access without also
// gaining unrelated Users-domain capabilities (solo-cert admin CRUD, SUA admin list).
permission!(OrgStaffingRequestsRead, ["org", "staffing_requests"], Read);
permission!(
    OrgStaffingRequestsDelete,
    ["org", "staffing_requests"],
    Delete
);
// Certification-type management + writing a controller's certification grid
// (org.rs's certification-type CRUD and save_user_certifications). Own
// permission pair (granted to STAFF) so it can be held without controller-status
// or roster-purge admin capabilities. Reading a user's certification grid stays
// data-dependent (self via auth.profile.read, else users.directory.read) and is
// not gated by these — see org.rs::get_user_certifications. The cert-type
// listing/editor UI needs the broader read.
permission!(OrgCertificationsRead, ["org", "certifications"], Read);
permission!(OrgCertificationsUpdate, ["org", "certifications"], Update);

// training.rs (assignments, ots, lessons, appointments, sessions, assignment_requests,
// release_requests). TrainingLessonsRead/TrainingLessonsUpdate above are reused here since
// training.rs's "lessons" endpoints share the same "training.lessons" permission namespace
// as training_admin.rs's progression/performance-indicator CRUD.
permission!(TrainingAssignmentsRead, ["training", "assignments"], Read);
permission!(
    TrainingAssignmentsCreate,
    ["training", "assignments"],
    Create
);
permission!(
    TrainingAssignmentsUpdate,
    ["training", "assignments"],
    Update
);
permission!(
    TrainingAssignmentsDelete,
    ["training", "assignments"],
    Delete
);
permission!(
    TrainingOtsRecommendationsRead,
    ["training", "ots_recommendations"],
    Read
);
permission!(
    TrainingOtsRecommendationsCreate,
    ["training", "ots_recommendations"],
    Create
);
permission!(
    TrainingOtsRecommendationsUpdate,
    ["training", "ots_recommendations"],
    Update
);
permission!(
    TrainingOtsRecommendationsDelete,
    ["training", "ots_recommendations"],
    Delete
);
permission!(TrainingLessonsCreate, ["training", "lessons"], Create);
permission!(TrainingLessonsDelete, ["training", "lessons"], Delete);
permission!(TrainingAppointmentsRead, ["training", "appointments"], Read);
permission!(
    TrainingAppointmentsCreate,
    ["training", "appointments"],
    Create
);
permission!(
    TrainingAppointmentsUpdate,
    ["training", "appointments"],
    Update
);
permission!(
    TrainingAppointmentsDelete,
    ["training", "appointments"],
    Delete
);
permission!(TrainingSessionsRead, ["training", "sessions"], Read);
permission!(TrainingSessionsCreate, ["training", "sessions"], Create);
permission!(TrainingSessionsUpdate, ["training", "sessions"], Update);
permission!(TrainingSessionsDelete, ["training", "sessions"], Delete);
permission!(
    TrainingAssignmentRequestsRead,
    ["training", "assignment_requests"],
    Read
);
permission!(
    TrainingAssignmentRequestsSelfRequest,
    ["training", "assignment_requests", "self"],
    Request
);
permission!(
    TrainingAssignmentRequestsDecide,
    ["training", "assignment_requests"],
    Decide
);
permission!(
    TrainingAssignmentRequestsInterestRequest,
    ["training", "assignment_requests", "interest"],
    Request
);
permission!(
    TrainingAssignmentRequestsInterestDelete,
    ["training", "assignment_requests", "interest"],
    Delete
);
permission!(
    TrainingReleaseRequestsRead,
    ["training", "release_requests"],
    Read
);
permission!(
    TrainingReleaseRequestsSelfRequest,
    ["training", "release_requests", "self"],
    Request
);
permission!(
    TrainingReleaseRequestsDecide,
    ["training", "release_requests"],
    Decide
);

// ---------------------------------------------------------------------------
// Worker A — Auth, Sessions & Infra Hardening (spec 009 handler cleanup + 010/011)
// ---------------------------------------------------------------------------

// emails.rs (spec 009 — static checks converted from manual ensure_permission).
// get_preferences/update_preferences stay public (unsubscribe-token routes), so
// they get no marker. The branding pair predates spec 009 (added by spec 013) but
// used manual ensure_permission with these same segments — converted here too.
permission!(EmailsTemplatesRead, ["emails", "templates"], Read);
permission!(EmailsPreviewCreate, ["emails", "preview"], Create);
permission!(EmailsSendCreate, ["emails", "send"], Create);
permission!(EmailsOutboxRead, ["emails", "outbox"], Read);
permission!(
    EmailsSuppressionsUpdate,
    ["emails", "suppressions"],
    Update
);
permission!(EmailsBrandingRead, ["emails", "branding"], Read);
permission!(EmailsBrandingUpdate, ["emails", "branding"], Update);

// admin.rs (spec 009 — static checks converted from manual ensure_permission).
// UsersControllerStatusUpdate/UsersStaffPositionsUpdate above are reused. The
// self-vs-other data-dependent reads elsewhere (e.g. org.rs certifications) are
// unaffected — these are the coarse static gates only.
permission!(AccessSelfRead, ["access", "self"], Read);
permission!(AccessUsersRead, ["access", "users"], Read);
permission!(AccessUsersUpdate, ["access", "users"], Update);
permission!(AccessCatalogRead, ["access", "catalog"], Read);
permission!(AuditLogsRead, ["audit", "logs"], Read);
permission!(
    UsersVatusaRefreshRequest,
    ["users", "vatusa_refresh"],
    Request
);
permission!(
    UsersVisitorApplicationsRead,
    ["users", "visitor_applications"],
    Read
);
permission!(
    UsersVisitorApplicationsDecide,
    ["users", "visitor_applications"],
    Decide
);
permission!(
    UsersDirectoryPrivateRead,
    ["users", "directory_private"],
    Read
);
permission!(UsersFlagsRead, ["users", "flags"], Read);
permission!(UsersFlagsUpdate, ["users", "flags"], Update);
permission!(
    UsersOperatingInitialsUpdate,
    ["users", "operating_initials"],
    Update
);

// auth.rs (spec 009 — static checks converted from manual ensure_permission).
// AuthProfileRead/AuthProfileUpdate above are reused. vatsim_login/vatsim_callback
// stay permission-free (OAuth flows); service_account_me keeps its existing no-gate
// shape — none get a marker.
permission!(AuthTeamspeakUidsRead, ["auth", "teamspeak_uids"], Read);
permission!(
    AuthTeamspeakUidsCreate,
    ["auth", "teamspeak_uids"],
    Create
);
permission!(
    AuthTeamspeakUidsDelete,
    ["auth", "teamspeak_uids"],
    Delete
);
permission!(AuthSessionsDelete, ["auth", "sessions"], Delete);
// Authenticated user impersonation (spec 012). Seeded to NO role — SERVER_ADMIN
// holds it implicitly via the effective-permissions cross-join, and no facility
// role should (checklist #9). Deliberately NOT added to the assignable catalog in
// acl.rs::default_permission_names, and filtered out of the access-editor catalog.
permission!(AuthImpersonateCreate, ["auth", "impersonate"], Create);

// User session manager (post-parity backlog). Like impersonate: seeded to no role
// (SERVER_ADMIN-only via the effective-permissions cross-join), kept out of the
// assignable catalog. Reveals login IPs/times and can force-logout a user.
permission!(UsersSessionsRead, ["users", "sessions"], Read);
permission!(UsersSessionsDelete, ["users", "sessions"], Delete);

// spec 010 — IP rate-limit bypass. `PermissionAction` has no `Bypass` variant, so
// this uses `Update`. It is a SINGLE segment `["system_rate_limit"]`, NOT
// `["system", "rate_limit"]`: `system.read` (SystemRead, org.rs) already makes
// `system` a leaf, and `insert_permission_path` (src/auth/acl.rs) silently drops a
// permission whose ancestor segment is already a leaf — the same leaf-vs-parent
// collision migration 0048 fixed for feedback.items_self. Underscore-joining keeps
// it a collision-free sibling while preserving the "system" framing.
// Required for BOTH session users and service accounts to skip throttling — a valid
// API key alone does not bypass. Seeded but granted to no role by default (an admin
// must grant it to specific accounts).
permission!(SystemRateLimitBypass, ["system_rate_limit"], Update);
