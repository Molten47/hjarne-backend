use std::net::SocketAddr;

use axum::{
    middleware::from_fn_with_state,
    routing::{get, patch, post},
    Router,
};
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod config;
mod middleware;
mod routes;
mod state;
mod notifications;

use config::Config;
use state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = Config::from_env()?;
    info!("configuration loaded");

    let db = db::create_pool(&config.database_url).await?;
    info!("database pool ready");

    let cache = cache::CacheClient::new(&config.redis_url).await?;
    info!("cache client ready");

    let state = AppState::new(db, cache, config.clone());

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // all authenticated staff — read-only staff list included
    let protected = Router::new()
        // auth
        .route("/api/v1/auth/me", get(routes::auth::me))
        .route("/api/v1/auth/logout", post(routes::auth::logout))
        .route("/api/v1/auth/change-password", post(routes::auth::change_password))
        // patients
        .route("/api/v1/patients", get(routes::patients::list_patients))
        .route("/api/v1/patients", post(routes::patients::create_patient))
        .route("/api/v1/patients/:id", get(routes::patients::get_patient))
        // staff — read only
        .route("/api/v1/staff", get(routes::staff::list_staff))
        // appointments
        .route("/api/v1/appointments", get(routes::appointments::list_appointments))
        .route("/api/v1/appointments", post(routes::appointments::create_appointment))
        .route("/api/v1/appointments/:id/status", patch(routes::appointments::update_appointment_status))
        // cases
        .route("/api/v1/cases", get(routes::cases::list_cases).post(routes::cases::open_case))
        .route("/api/v1/cases/:id", get(routes::cases::get_case))
        .route("/api/v1/cases/:id/status", patch(routes::cases::update_case_status))
        .route("/api/v1/cases/:id/diagnoses", post(routes::cases::add_diagnosis))
        .route("/api/v1/cases/:id/assignments", get(routes::cases::list_assignments))
        .route("/api/v1/cases/:id/assignments", post(routes::cases::assign_staff))
        // clinical communications
        .route("/api/v1/cases/:id/communications", get(routes::clinical_comms::list_comms).post(routes::clinical_comms::send_comm))
        .route("/api/v1/communications/:id/attachment", post(routes::clinical_comms::upload_attachment))
        .route("/api/v1/communications/:id/read", patch(routes::clinical_comms::mark_comm_read))
        // prescriptions
        .route("/api/v1/cases/:id/prescriptions", post(routes::prescriptions::create_prescription))
        .route("/api/v1/prescriptions/queue", get(routes::prescriptions::get_pharmacy_queue))
        .route("/api/v1/prescriptions/:id/dispense", patch(routes::prescriptions::dispense_prescription))
        .route("/api/v1/drugs/stock", get(routes::prescriptions::get_drug_stock))
        // notifications
      // notifications
        .route("/api/v1/notifications", get(routes::notifications::list_notifications))
        .route("/api/v1/notifications/read", post(routes::notifications::mark_read))
        // portal — authenticated patient routes
        .route("/api/v1/portal/me",           get(routes::portal::portal_me))
        .route("/api/v1/portal/appointments", get(routes::portal::portal_appointments))
        .route("/api/v1/portal/cases",        get(routes::portal::portal_cases))
        .route("/api/v1/portal/cases/:id/diagnoses", get(routes::portal::portal_diagnoses))
        .route("/api/v1/portal/complaints",   post(routes::portal::submit_complaint))
       .route("/api/v1/portal/messages",     get(routes::portal::portal_messages)
                                              .post(routes::portal::send_portal_message))
        .route("/api/v1/portal/profile",      get(routes::portal::get_portal_profile)
                                              .patch(routes::portal::update_portal_profile))
        // staff → patient invite
        .route("/api/v1/patients/:id/invite", post(routes::portal::send_invite))
        .route("/api/v1/patients/:id/messages", get(routes::portal::staff_get_patient_messages)
                                               .post(routes::portal::staff_send_portal_message))
        .route_layer(from_fn_with_state(
            state.clone(),
            middleware::require_auth,
        ));

    // admin-only mutations — require_admin runs after require_auth
    let admin_only = Router::new()
        .route("/api/v1/staff", post(routes::staff::create_staff))
        .route("/api/v1/staff/:id/auth", post(routes::staff::create_staff_auth))
        .route_layer(from_fn_with_state(
            state.clone(),
            middleware::require_admin,
        ))
        .route_layer(from_fn_with_state(
            state.clone(),
            middleware::require_auth,
        ));

    // public — no auth required
    let public = Router::new()
        .route("/api/v1/auth/login", post(routes::auth::login))
        .route("/api/v1/portal/setup", post(routes::portal::setup_account))
        .route("/api/v1/portal/login", post(routes::portal::portal_login))
        .route("/api/v1/auth/refresh", post(routes::auth::refresh))
        .route("/api/v1/ws", get(routes::ws::ws_handler))
        .route("/health", get(routes::health::health_check))
        .route("/api/v1/communications/:id/attachment/serve", get(routes::clinical_comms::serve_attachment));

    let app = Router::new()
        .merge(protected)
        .merge(admin_only)
        .merge(public)
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = config.socket_addr().parse()?;
    info!("hjarne-api listening on {addr}");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
