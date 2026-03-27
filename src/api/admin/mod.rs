pub mod proxies;
pub mod settings;
pub mod users;

use crate::db::User;
use axum::extract::Extension;

/// Type alias for extracting the current admin user from request extensions.
/// Set by the admin_auth middleware.
pub type CurrentUser = Extension<User>;
