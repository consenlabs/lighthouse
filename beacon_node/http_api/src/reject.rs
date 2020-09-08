use eth2::types::ErrorMessage;
use std::convert::Infallible;
use warp::{http::StatusCode, reject::Reject};

#[derive(Debug)]
pub struct BeaconChainError(pub beacon_chain::BeaconChainError);

impl Reject for BeaconChainError {}

pub fn beacon_chain_error(e: beacon_chain::BeaconChainError) -> warp::reject::Rejection {
    warp::reject::custom(BeaconChainError(e))
}

#[derive(Debug)]
pub struct CustomNotFound(pub String);

impl Reject for CustomNotFound {}

pub fn custom_not_found(msg: String) -> warp::reject::Rejection {
    warp::reject::custom(CustomNotFound(msg))
}

#[derive(Debug)]
pub struct CustomBadRequest(pub String);

impl Reject for CustomBadRequest {}

pub fn custom_bad_request(msg: String) -> warp::reject::Rejection {
    warp::reject::custom(CustomBadRequest(msg))
}

#[derive(Debug)]
pub struct CustomServerError(pub String);

impl Reject for CustomServerError {}

pub fn custom_server_error(msg: String) -> warp::reject::Rejection {
    warp::reject::custom(CustomServerError(msg))
}

#[derive(Debug)]
pub struct BroadcastWithoutImport(pub String);

impl Reject for BroadcastWithoutImport {}

pub fn broadcast_without_import(msg: String) -> warp::reject::Rejection {
    warp::reject::custom(BroadcastWithoutImport(msg))
}

#[derive(Debug)]
pub struct ObjectInvalid(pub String);

impl Reject for ObjectInvalid {}

pub fn object_invalid(msg: String) -> warp::reject::Rejection {
    warp::reject::custom(ObjectInvalid(msg))
}

// This function receives a `Rejection` and tries to return a custom
// value, otherwise simply passes the rejection along.
pub async fn handle_rejection(err: warp::Rejection) -> Result<impl warp::Reply, Infallible> {
    let code;
    let message;

    if err.is_not_found() {
        code = StatusCode::NOT_FOUND;
        message = "NOT_FOUND".to_string();
    } else if let Some(e) = err.find::<warp::filters::body::BodyDeserializeError>() {
        message = format!("BAD_REQUEST: body deserialize error: {}", e);
        code = StatusCode::BAD_REQUEST;
    } else if let Some(e) = err.find::<warp::reject::InvalidQuery>() {
        code = StatusCode::BAD_REQUEST;
        message = format!("BAD_REQUEST (invalid query): {}", e);
    } else if let Some(e) = err.find::<crate::reject::BeaconChainError>() {
        code = StatusCode::INTERNAL_SERVER_ERROR;
        message = format!("UNHANDLED_ERROR: {:?}", e.0);
    } else if let Some(e) = err.find::<crate::reject::CustomNotFound>() {
        code = StatusCode::NOT_FOUND;
        message = format!("NOT_FOUND: {}", e.0);
    } else if let Some(e) = err.find::<crate::reject::CustomBadRequest>() {
        code = StatusCode::BAD_REQUEST;
        message = format!("BAD_REQUEST: {}", e.0);
    } else if let Some(e) = err.find::<crate::reject::CustomServerError>() {
        code = StatusCode::INTERNAL_SERVER_ERROR;
        message = format!("INTERNAL_SERVER_ERROR: {}", e.0);
    } else if let Some(e) = err.find::<crate::reject::BroadcastWithoutImport>() {
        code = StatusCode::ACCEPTED;
        message = format!(
            "ACCEPTED: the object was broadcast to the network without being \
            fully imported to the local database: {}",
            e.0
        );
    } else if let Some(e) = err.find::<crate::reject::ObjectInvalid>() {
        code = StatusCode::BAD_REQUEST;
        message = format!("BAD_REQUEST: Invalid object: {}", e.0);
    } else if err.find::<warp::reject::MethodNotAllowed>().is_some() {
        code = StatusCode::METHOD_NOT_ALLOWED;
        message = "METHOD_NOT_ALLOWED".to_string();
    } else {
        code = StatusCode::INTERNAL_SERVER_ERROR;
        message = "UNHANDLED_REJECTION".to_string();
    }

    let json = warp::reply::json(&ErrorMessage {
        code: code.as_u16(),
        message,
        stacktraces: vec![],
    });

    Ok(warp::reply::with_status(json, code))
}