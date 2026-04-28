use axum::{
    Router, middleware,
    routing::{get, patch, post},
};
use tower_http::cors::{Any, CorsLayer};

use crate::{
    auth::middleware::{require_staff, resolve_current_user},
    docs,
    handlers::{
        admin, auth, dev, docs as docs_handlers, events, feedback, files, health, stats, training,
        users,
    },
    state::AppState,
};

pub fn build_router(state: AppState) -> Router {
    let admin_routes = Router::new()
        .route("/acl", get(admin::acl_debug))
        .route("/audit", get(admin::list_audit_logs))
        .route("/access/catalog", get(admin::get_access_catalog))
        .route(
            "/users/{cid}/access",
            get(admin::get_user_access).post(admin::update_user_access),
        )
        .route(
            "/users/{cid}/controller-status",
            patch(admin::set_user_controller_status),
        )
        .route_layer(middleware::from_fn_with_state(state.clone(), require_staff));

    let user_routes = Router::new()
        .route("/visit-artcc", post(users::visit_artcc))
        .route("/", get(users::list_users))
        .route("/{cid}/feedback", get(users::get_user_feedback))
        .route("/{cid}", get(users::get_user));

    let event_routes = Router::new()
        .route("/", get(events::list_events).post(events::create_event))
        .route(
            "/{event_id}",
            get(events::get_event)
                .patch(events::update_event)
                .delete(events::delete_event),
        )
        .route(
            "/{event_id}/positions",
            get(events::list_event_positions).post(events::create_event_position),
        )
        .route(
            "/{event_id}/positions/{position_id}",
            patch(events::assign_event_position).delete(events::delete_event_position),
        )
        .route(
            "/{event_id}/positions/publish",
            post(events::publish_event_positions),
        );

    let training_routes = Router::new()
        .route(
            "/assignments",
            get(training::list_assignments).post(training::create_assignment),
        )
        .route(
            "/lessons",
            get(training::list_lessons).post(training::create_lesson),
        )
        .route(
            "/lessons/{lesson_id}",
            patch(training::update_lesson).delete(training::delete_lesson),
        )
        .route(
            "/sessions",
            get(training::list_training_sessions).post(training::create_training_session),
        )
        .route(
            "/sessions/{session_id}",
            get(training::get_training_session)
                .patch(training::update_training_session)
                .delete(training::delete_training_session),
        )
        .route(
            "/assignment-requests",
            get(training::list_assignment_requests).post(training::create_assignment_request),
        )
        .route(
            "/assignment-requests/{request_id}",
            patch(training::decide_assignment_request),
        )
        .route(
            "/assignment-requests/{request_id}/interest",
            post(training::add_assignment_request_interest)
                .delete(training::remove_assignment_request_interest),
        )
        .route(
            "/trainer-release-requests",
            get(training::list_release_requests).post(training::create_release_request),
        )
        .route(
            "/trainer-release-requests/{request_id}",
            patch(training::decide_release_request),
        );

    let feedback_routes = Router::new()
        .route(
            "/",
            get(feedback::list_feedback).post(feedback::create_feedback),
        )
        .route("/{feedback_id}", patch(feedback::decide_feedback));

    let file_routes = Router::new()
        .route("/", get(files::list_files).post(files::upload_file))
        .route(
            "/{file_id}",
            get(files::get_file_metadata)
                .patch(files::update_file_metadata)
                .delete(files::delete_file),
        )
        .route(
            "/{file_id}/content",
            get(files::download_file_content).put(files::replace_file_content),
        )
        .route("/{file_id}/signed-url", get(files::get_signed_download_url));

    let mut api = Router::new()
        .route("/me", get(auth::me))
        .route("/auth/service-account/me", get(auth::service_account_me))
        .route("/auth/vatsim/login", get(auth::vatsim_login))
        .route("/auth/vatsim/callback", get(auth::vatsim_callback))
        .route("/auth/logout", post(auth::logout))
        .route("/admin/files/audit", get(files::list_file_audit_logs))
        .route("/stats/artcc", get(stats::get_artcc_stats))
        .route(
            "/stats/controller-events",
            get(stats::list_controller_events),
        )
        .route(
            "/stats/controller/{cid}/history",
            get(stats::get_controller_history),
        )
        .route(
            "/stats/controller/{cid}/totals",
            get(stats::get_controller_totals),
        )
        .nest("/admin", admin_routes)
        .nest("/events", event_routes)
        .nest("/feedback", feedback_routes)
        .nest("/files", file_routes)
        .nest("/training", training_routes)
        .nest("/user", user_routes);

    if api_dev_mode_enabled() {
        api = api
            .route("/auth/login/as/{cid}", get(auth::login_as_cid))
            .route("/dev/seed", post(dev::seed_data));
    }

    Router::new()
        .route("/health", get(health::health))
        .route("/ready", get(health::ready))
        .route("/cdn/{file_id}", get(files::cdn_download_file))
        .route("/docs", get(docs_handlers::docs_index))
        .route("/docs/{section}/{page}", get(docs_handlers::docs_page))
        .route("/docs/health", get(docs_handlers::docs_health))
        .nest("/api/v1", api)
        .merge(docs::build_docs_router())
        .layer(middleware::from_fn_with_state(
            state.clone(),
            crate::logging::log_requests,
        ))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            resolve_current_user,
        ))
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .with_state(state)
}

fn api_dev_mode_enabled() -> bool {
    env_flag_enabled("API_DEV_MODE") || env_flag_enabled("VATSIM_DEV_MODE")
}

fn env_flag_enabled(name: &str) -> bool {
    std::env::var(name)
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}
