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
permission!(FeedbackItemsSelfRead, ["feedback", "items", "self"], Read);

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
permission!(SystemRead, ["system"], Read);

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
