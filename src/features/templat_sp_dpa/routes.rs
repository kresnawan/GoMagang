use axum::{
    Router,
    middleware::from_fn_with_state,
    routing::{get, put},
};

use super::handler;
use crate::{features::auth::require_panitia_magang, state::AppState};

pub fn routes(state: &AppState) -> Router<AppState> {
    let simpan =
        put(handler::simpan).route_layer(from_fn_with_state(state.clone(), require_panitia_magang));

    Router::new().route("/templat-sp-dpa", get(handler::find).merge(simpan))
}
