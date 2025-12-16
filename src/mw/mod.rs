use axum::{
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use axum::extract::State;
use serde_json::json;
use crate::config::SharedConfig;

pub async fn internal_auth(
    State(cfg): State<SharedConfig>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let ok = req
        .headers()
        .get("X-Internal-Auth")
        .and_then(|x| x.to_str().ok())
        .map(|v| v == cfg.auth.token)
        .unwrap_or(false);

    if !ok {
        let json_body = json!({
            "error": "Missing or invalid X-Internal-Auth token."
        })
            .to_string();

        return Ok(Response::builder()
            .status(StatusCode::FORBIDDEN)
            .header("Content-Type", "application/json")
            .body(json_body.into())
            .unwrap());
    }

    let mut res = next.run(req).await;

    res.headers_mut()
        .insert("X-Internal-Auth", "checked".parse().unwrap());

    Ok(res)
}
