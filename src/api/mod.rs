pub mod admin;
pub mod auth;
pub mod client_fetch;
pub mod fetch;
pub mod relay;
pub mod subscription;

use crate::AppState;
use axum::{
    extract::State,
    http::{Request, StatusCode},
    middleware::{self, Next},
    response::Response,
    routing::{delete, get, post, put},
    Router,
};
use axum::extract::DefaultBodyLimit;
use std::sync::Arc;
use tower_http::cors::CorsLayer;

pub fn router(state: Arc<AppState>) -> Router {
    // Auth routes — no auth required
    let auth_routes = Router::new()
        .route("/api/auth/login", get(auth::login))
        .route("/api/auth/callback", get(auth::callback))
        .route("/api/auth/me", get(auth::me))
        .route("/api/auth/logout", post(auth::logout))
        .route("/api/auth/regenerate-key", post(auth::regenerate_key))
        .route("/api/auth/login/password", post(auth::login_password))
        .route("/api/auth/options", get(auth::auth_options))
        .route("/api/auth/register", post(auth::register));

    // Admin routes — protected by session + role
    let admin_routes = Router::new()
        .route("/api/admin/proxies", get(admin::proxies::list_proxies))
        .route("/api/admin/proxies/:id", delete(admin::proxies::delete_proxy))
        .route("/api/admin/proxies/:id/toggle", post(admin::proxies::toggle_proxy))
        .route("/api/admin/proxies/:id/validate", post(admin::proxies::validate_single_proxy))
        .route("/api/admin/proxies/:id/quality", post(admin::proxies::quality_check_single_proxy))
        .route("/api/admin/proxies/cleanup", post(admin::proxies::cleanup_proxies))
        .route("/api/admin/validate", post(admin::proxies::trigger_validation))
        .route("/api/admin/quality-check", post(admin::proxies::trigger_quality_check))
        .route("/api/admin/stats", get(admin::settings::get_stats))
        .route("/api/admin/users", get(admin::users::list_users))
        .route("/api/admin/users/:id", delete(admin::users::delete_user))
        .route("/api/admin/users/:id/ban", post(admin::users::ban_user))
        .route("/api/admin/users/:id/unban", post(admin::users::unban_user))
        .route("/api/admin/users/create", post(admin::users::create_password_user))
        .route("/api/admin/users/:id/password", put(admin::users::reset_user_password))
        .route("/api/admin/users/:id/role", put(admin::users::change_user_role))
        .route("/api/admin/users/:id/username", put(admin::users::update_username))
        .route("/api/admin/settings", get(admin::settings::get_settings).put(admin::settings::update_settings))
        .route(
            "/api/subscriptions",
            get(subscription::list_subscriptions).post(subscription::add_subscription),
        )
        .route(
            "/api/subscriptions/:id",
            delete(subscription::delete_subscription).put(subscription::update_subscription),
        )
        .route(
            "/api/subscriptions/:id/refresh",
            post(subscription::refresh_subscription),
        )
        .route_layer(middleware::from_fn_with_state(state.clone(), admin_auth));

    // Fetch/Relay/Proxies routes — handler-level auth (API key or session)
    let fetch_relay_routes = Router::new()
        .route("/api/fetch", get(fetch::fetch_proxies))
        .route("/api/client/fetch", get(client_fetch::client_fetch_proxies))
        .route("/api/proxies", get(fetch::list_all_proxies))
        .route(
            "/api/relay",
            get(relay::relay_request)
                .post(relay::relay_request),
        )
        .layer(DefaultBodyLimit::max(10 * 1024 * 1024)); // 10 MB

    // Page routes — no auth
    let page_routes = Router::new()
        .route("/", get(user_page))
        .route("/admin", get(admin_page))
        .route("/docs", get(docs_page));

    Router::new()
        .merge(auth_routes)
        .merge(admin_routes)
        .merge(fetch_relay_routes)
        .merge(page_routes)
        .layer(CorsLayer::permissive())
        .with_state(state)
}

async fn admin_auth(
    State(state): State<Arc<AppState>>,
    mut request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // Session-based admin auth: require valid session with admin or super_admin role
    let headers = request.headers().clone();
    let user = auth::extract_session_user(&state, &headers)
        .await
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    if user.role == "user" {
        return Err(StatusCode::FORBIDDEN);
    }

    // Inject user into request extensions for downstream handlers
    request.extensions_mut().insert(user);
    Ok(next.run(request).await)
}

async fn user_page() -> axum::response::Html<&'static str> {
    axum::response::Html(include_str!("../web/user.html"))
}

async fn admin_page() -> axum::response::Html<&'static str> {
    axum::response::Html(include_str!("../web/admin.html"))
}

async fn docs_page() -> axum::response::Html<String> {
    use pulldown_cmark::{Parser, Options, html};
    let readme = include_str!("../../README.md");
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    let parser = Parser::new_ext(readme, options);
    let mut rendered = String::new();
    html::push_html(&mut rendered, parser);
    let template = include_str!("../web/docs.html");
    let page = template.replace("{{CONTENT}}", &rendered);
    axum::response::Html(page)
}
