//! Authorization tests driven through the real router.
//!
//! `http_api.rs`'s own unit tests cover the routing predicates (`csrf_exempt`,
//! `is_public_route`) in isolation. These exercise `http_router` end to end, so a handler that
//! forgets its guard fails here instead of shipping — the regression test for C1/C2 in
//! `docs/SECURITY_REVIEW_OPUS48.md`.

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use tower::ServiceExt;
use wiab::config::AppConfig;
use wiab::{Cli, bootstrap};

/// Every route the authentication middleware must gate, with a path shaped like the real
/// thing. The ids need not exist: the gate rejects before any handler resolves them.
///
/// This table is maintained by hand — axum's `Router` does not expose its routes, so a new
/// route is not automatically covered. `every_listed_route_is_reachable` at least proves no
/// entry has rotted into a 404, and `guarded_routes_reject_anonymous_callers` proves the ones
/// listed are closed.
const GUARDED_ROUTES: &[(Method, &str)] = &[
    (Method::GET, "/organizations"),
    (Method::POST, "/organizations"),
    (Method::GET, "/organizations/O-1"),
    (Method::PUT, "/organizations/O-1"),
    (Method::GET, "/organizations/O-1/projects"),
    (Method::POST, "/organizations/O-1/projects"),
    (Method::GET, "/organizations/O-1/agents"),
    (Method::POST, "/organizations/O-1/agents"),
    (Method::GET, "/organizations/O-1/teams"),
    (Method::POST, "/organizations/O-1/teams"),
    (Method::GET, "/organizations/O-1/meetings"),
    (Method::POST, "/organizations/O-1/meetings"),
    (Method::GET, "/projects/P-1"),
    (Method::PUT, "/projects/P-1"),
    (Method::GET, "/projects/P-1/works"),
    (Method::POST, "/projects/P-1/works"),
    (Method::GET, "/projects/P-1/boards"),
    (Method::POST, "/projects/P-1/boards"),
    (Method::GET, "/projects/P-1/repos"),
    (Method::POST, "/projects/P-1/repos"),
    (Method::GET, "/projects/P-1/pipelines"),
    (Method::POST, "/projects/P-1/pipelines"),
    (Method::GET, "/agents/A-1"),
    (Method::PUT, "/agents/A-1"),
    (Method::POST, "/agents/A-1/activate"),
    (Method::POST, "/agents/A-1/deactivate"),
    (Method::GET, "/boards/B-1"),
    (Method::PUT, "/boards/B-1"),
    (Method::GET, "/boards/B-1/tasks"),
    (Method::POST, "/boards/B-1/tasks"),
    (Method::POST, "/boards/B-1/tasks/claim"),
    (Method::GET, "/tasks/T-1"),
    (Method::POST, "/tasks/T-1/start"),
    (Method::POST, "/tasks/T-1/block"),
    (Method::POST, "/tasks/T-1/resume"),
    (Method::POST, "/tasks/T-1/escalate"),
    (Method::POST, "/tasks/T-1/complete"),
    (Method::POST, "/tasks/T-1/fail"),
    (Method::GET, "/teams/TM-1"),
    (Method::GET, "/teams/TM-1/task"),
    (Method::POST, "/teams/TM-1/start"),
    (Method::POST, "/teams/TM-1/pause"),
    (Method::POST, "/teams/TM-1/resume"),
    (Method::POST, "/teams/TM-1/stop"),
    (Method::GET, "/repos/R-1"),
    (Method::PUT, "/repos/R-1"),
    (Method::GET, "/repos/R-1/branches"),
    (Method::GET, "/repos/R-1/branches/main/files"),
    (Method::GET, "/repos/R-1/branches/main/files/raw"),
    (Method::GET, "/repos/R-1/branches/main/commits"),
    (Method::POST, "/repos/R-1/commits"),
    (Method::PUT, "/repos/R-1/visibility"),
    (Method::GET, "/repos/R-1/pull-requests"),
    (Method::POST, "/repos/R-1/pull-requests"),
    (Method::GET, "/pull-requests/PR-1"),
    (Method::POST, "/pull-requests/PR-1/close"),
    (Method::POST, "/pull-requests/PR-1/merge"),
    (Method::GET, "/users"),
    (Method::POST, "/users"),
    (Method::GET, "/users/U-1"),
    (Method::POST, "/users/U-1/ssh-keys"),
    (Method::DELETE, "/users/U-1/ssh-keys/K-1"),
    (Method::POST, "/users/U-1/tokens"),
    (Method::DELETE, "/users/U-1/tokens/TK-1"),
    (Method::POST, "/users/invite"),
    (Method::POST, "/users/U-1/deactivate"),
    (Method::POST, "/users/U-1/activate"),
    (Method::GET, "/role-assignments"),
    (Method::POST, "/role-assignments"),
    (Method::DELETE, "/role-assignments/RA-1"),
    (Method::GET, "/pipelines/PL-1"),
    (Method::PUT, "/pipelines/PL-1"),
    (Method::GET, "/works/W-1"),
    (Method::PUT, "/works/W-1"),
    (Method::POST, "/works/W-1/dones"),
    (Method::POST, "/works/W-1/dones/D-1/fulfill"),
    (Method::POST, "/works/W-1/dones/D-1/unfulfill"),
    (Method::GET, "/signal"),
];

/// Builds the same router the binary serves, over in-memory persistence.
async fn test_router() -> Router {
    let cli = Cli {
        persistence: "memory".to_owned(),
        database_url: String::new(),
    };
    let mut config = AppConfig::load(&cli).expect("test configuration");
    // Own the git root so a concurrent dev server (or another test binary) cannot collide.
    config.serve.git_root = tempfile::tempdir()
        .expect("temp git root")
        .keep()
        .join("git");
    let state = bootstrap::build_app_state(&config, None)
        .await
        .expect("app state");
    wiab_inf::http_router(state)
}

async fn send(router: &Router, method: Method, path: &str) -> axum::http::Response<Body> {
    let request = Request::builder()
        .method(method)
        .uri(path)
        .body(Body::empty())
        .expect("valid request");
    router
        .clone()
        .oneshot(request)
        .await
        .expect("router responds")
}

#[tokio::test]
async fn guarded_routes_reject_anonymous_callers() {
    let router = test_router().await;
    for (method, path) in GUARDED_ROUTES {
        let response = send(&router, method.clone(), path).await;
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "{method} {path} must require authentication"
        );
    }
}

#[tokio::test]
async fn every_listed_route_is_reachable() {
    let router = test_router().await;
    for (method, path) in GUARDED_ROUTES {
        let response = send(&router, method.clone(), path).await;
        assert_ne!(
            response.status(),
            StatusCode::NOT_FOUND,
            "{method} {path} is not a route — the table is stale"
        );
        assert_ne!(
            response.status(),
            StatusCode::METHOD_NOT_ALLOWED,
            "{method} {path} is not served for this method — the table is stale"
        );
    }
}

#[tokio::test]
async fn health_is_public() {
    let router = test_router().await;
    let response = send(&router, Method::GET, "/health").await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn git_smart_http_authenticates_itself() {
    let router = test_router().await;
    let response = send(
        &router,
        Method::GET,
        "/repos/R-1.git/info/refs?service=git-upload-pack",
    )
    .await;
    // Still 401 for an unknown/private repo, but from `authorize_git` — which offers the Basic
    // challenge `git` needs — not from the middleware, which would pre-empt the transport.
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response
            .headers()
            .get(header::WWW_AUTHENTICATE)
            .and_then(|value| value.to_str().ok()),
        Some("Basic realm=\"wiab-git\""),
    );
}
