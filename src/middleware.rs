mod prelude {
    pub use crate::middleware::log_request::CustomMetadataRequestExt;
    pub use conduit::{box_error, Body, Handler, RequestExt};
    pub use conduit_middleware::{AfterResult, AroundMiddleware, BeforeResult, Middleware};
    pub use http::{header, Response, StatusCode};
}

use self::app::AppMiddleware;
use self::known_error_to_json::KnownErrorToJson;

pub mod app;
mod balance_capacity;
mod block_traffic;
mod debug;
mod ember_html;
mod head;
mod known_error_to_json;
pub mod log_request;
pub mod normalize_path;
mod require_user_agent;
mod sentry;
pub mod session;
mod static_or_continue;
mod update_metrics;

use conduit_middleware::MiddlewareBuilder;
use conduit_router::RouteBuilder;

use ::sentry::integrations::tower as sentry_tower;
use axum::error_handling::HandleErrorLayer;
use axum::middleware::{from_fn, from_fn_with_state};
use axum::Router;

use crate::app::AppState;
use crate::Env;

pub fn apply_axum_middleware(state: AppState, router: Router) -> Router {
    type Request = http::Request<axum::body::Body>;

    let env = state.config.env();

    let capacity = state.config.db.primary.pool_size;
    if capacity >= 10 {
        info!(?capacity, "Enabling BalanceCapacity middleware");
    } else {
        info!("BalanceCapacity middleware not enabled. DB_PRIMARY_POOL_SIZE is too low.");
    }

    let middleware = tower::ServiceBuilder::new()
        .layer(sentry_tower::NewSentryLayer::<Request>::new_from_top())
        .layer(sentry_tower::SentryHttpLayer::with_transaction())
        .layer(from_fn(self::sentry::set_transaction))
        .layer(from_fn(log_request::log_requests))
        .layer(from_fn_with_state(
            state.clone(),
            update_metrics::update_metrics,
        ))
        // The following layer is unfortunately necessary for `option_layer()` to work
        .layer(HandleErrorLayer::new(dummy_error_handler))
        // Optionally print debug information for each request
        // To enable, set the environment variable: `RUST_LOG=cargo_registry::middleware=debug`
        .option_layer((env == Env::Development).then(|| from_fn(debug::debug_requests)))
        .layer(from_fn_with_state(state.clone(), session::attach_session))
        .layer(from_fn_with_state(
            state.clone(),
            require_user_agent::require_user_agent,
        ))
        .layer(from_fn_with_state(
            state.clone(),
            block_traffic::block_traffic,
        ))
        .layer(from_fn_with_state(
            state.clone(),
            block_traffic::block_routes,
        ))
        .layer(from_fn(head::support_head_requests))
        .layer(HandleErrorLayer::new(dummy_error_handler))
        .option_layer(
            (env == Env::Development).then(|| from_fn(static_or_continue::serve_local_uploads)),
        )
        // Serve the static files in the *dist* directory, which are the frontend assets.
        // Not needed for the backend tests.
        .layer(HandleErrorLayer::new(dummy_error_handler))
        .option_layer((env != Env::Test).then(|| from_fn(static_or_continue::serve_dist)))
        .layer(HandleErrorLayer::new(dummy_error_handler))
        .option_layer(
            (env != Env::Test).then(|| from_fn_with_state(state.clone(), ember_html::serve_html)),
        )
        // This is currently the final middleware to run. If a middleware layer requires a database
        // connection, it should be run after this middleware so that the potential pool usage can be
        // tracked here.
        //
        // In production we currently have 2 equally sized pools (primary and a read-only replica).
        // Because such a large portion of production traffic is for download requests (which update
        // download counts), we consider only the primary pool here.
        .layer(HandleErrorLayer::new(dummy_error_handler))
        .option_layer(
            (capacity >= 10).then(|| from_fn_with_state(state, balance_capacity::balance_capacity)),
        );

    router.layer(middleware)
}

/// This function is only necessary because `.option_layer()` changes the error type
/// and we need to change it back. Since the axum middleware has no way of returning
/// an actual error this function should never actually be called.
async fn dummy_error_handler(_err: axum::BoxError) -> http::StatusCode {
    http::StatusCode::INTERNAL_SERVER_ERROR
}

pub fn build_middleware(app: AppState, endpoints: RouteBuilder) -> MiddlewareBuilder {
    let mut m = MiddlewareBuilder::new(endpoints);

    m.add(AppMiddleware::new(app));
    m.add(KnownErrorToJson);

    m
}
