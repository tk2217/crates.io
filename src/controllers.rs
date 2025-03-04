mod cargo_prelude {
    pub use super::prelude::*;
    pub use crate::util::errors::cargo_err;
}

mod frontend_prelude {
    pub use super::prelude::*;
    pub use crate::util::errors::{bad_request, server_error};
}

mod prelude {
    pub use super::helpers::ok_true;
    pub use diesel::prelude::*;

    pub use conduit::RequestExt;
    pub use conduit_router::RequestParams;
    pub use http::{header, StatusCode};

    pub use super::conduit_axum::conduit_compat;
    pub use crate::middleware::app::RequestApp;
    pub use crate::util::errors::{cargo_err, AppError, AppResult}; // TODO: Remove cargo_err from here
    pub use crate::util::{AppResponse, EndpointResult};

    use indexmap::IndexMap;
    use serde::Serialize;

    pub trait RequestUtils {
        fn redirect(&self, url: String) -> AppResponse;

        fn json<T: Serialize>(&self, t: &T) -> AppResponse;
        fn query(&self) -> IndexMap<String, String>;
        fn wants_json(&self) -> bool;
        fn query_with_params(&self, params: IndexMap<String, String>) -> String;
    }

    impl<'a> RequestUtils for dyn RequestExt + 'a {
        fn json<T: Serialize>(&self, t: &T) -> AppResponse {
            crate::util::json_response(t)
        }

        fn query(&self) -> IndexMap<String, String> {
            url::form_urlencoded::parse(self.query_string().unwrap_or("").as_bytes())
                .into_owned()
                .collect()
        }

        fn redirect(&self, url: String) -> AppResponse {
            http::Response::builder()
                .status(StatusCode::FOUND)
                .header(header::LOCATION, url)
                .body(conduit::Body::empty())
                .unwrap() // Should not panic unless url contains "\r\n"
        }

        fn wants_json(&self) -> bool {
            self.headers()
                .get_all(header::ACCEPT)
                .iter()
                .any(|val| val.to_str().unwrap_or_default().contains("json"))
        }

        fn query_with_params(&self, new_params: IndexMap<String, String>) -> String {
            let mut params = self.query();
            params.extend(new_params);
            let query_string = url::form_urlencoded::Serializer::new(String::new())
                .extend_pairs(params)
                .finish();
            format!("?{query_string}")
        }
    }
}

pub mod helpers;
pub mod util;

pub mod category;
mod conduit_axum;
pub mod crate_owner_invitation;
pub mod git;
pub mod github;
pub mod keyword;
pub mod krate;
pub mod metrics;
pub mod site_metadata;
pub mod team;
pub mod token;
pub mod user;
pub mod version;
