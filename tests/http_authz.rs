//! Authorization tests driven through the real router.
//!
//! `http_api.rs`'s own unit tests cover the routing predicates (`csrf_exempt`,
//! `is_public_route`) in isolation. These exercise `http_router` end to end, so a handler that
//! forgets its guard fails here instead of shipping — the regression test for C1/C2 in
//! `docs/SECURITY_REVIEW_OPUS48.md`.

use std::net::SocketAddr;

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Method, Request, StatusCode, header};
use tower::ServiceExt;
use wiab::config::AppConfig;
use wiab::{Cli, bootstrap};
use wiab_app::{CreateRepoRequest, CreateUserRequest, IssueTokenRequest};
use wiab_core::access::{Role, Scope};
use wiab_core::organization::OrganizationId;
use wiab_inf::AppState;

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

/// Builds the same `AppState` the binary serves, over in-memory persistence. Bootstrap has
/// already seeded organization `O-1` with project `P-1` and an Owner user by the time this
/// returns.
async fn test_state() -> AppState {
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
    bootstrap::build_app_state(&config, None)
        .await
        .expect("app state")
}

async fn test_router() -> Router {
    wiab_inf::http_router(test_state().await)
}

/// A repo of each visibility, plus a member of `O-1` and a stranger — the two callers the
/// visibility rule has to tell apart.
struct RepoFixture {
    router: Router,
    private_repo: String,
    public_repo: String,
    member_token: String,
    stranger_token: String,
    /// Holds Write on `O-1`, for the endpoints that mutate rather than read.
    owner_token: String,
}

async fn repo_fixture() -> RepoFixture {
    let state = test_state().await;

    let repo = |name: &'static str, visibility: &'static str| {
        let repos = state.repo_service.clone();
        async move {
            repos
                .create_repo(
                    "P-1",
                    CreateRepoRequest {
                        name: name.to_owned(),
                        description: String::new(),
                        visibility: Some(visibility.to_owned()),
                    },
                )
                .await
                .expect("create repo")
                .expect("seed project exists")
                .id
        }
    };
    let private_repo = repo("secrets", "private").await;
    let public_repo = repo("open", "public").await;

    let user = |name: &'static str, role: Option<Role>| {
        let users = state.user_service.clone();
        let access = state.access_service.clone();
        async move {
            let user = users
                .create_user(CreateUserRequest {
                    kind: "human".to_owned(),
                    name: name.to_owned(),
                    email: Some(format!("{name}@example.test")),
                })
                .await
                .expect("create user");
            if let Some(role) = role {
                access
                    .grant_direct(
                        user.id.parse().expect("user id"),
                        Scope::Org(OrganizationId::from_number(1)),
                        role,
                    )
                    .await
                    .expect("grant role");
            }
            users
                .issue_token(
                    &user.id,
                    IssueTokenRequest {
                        label: "test".to_owned(),
                        read_only: false,
                        repos: None,
                        orgs: None,
                        expires_at: None,
                    },
                )
                .await
                .expect("issue token")
                .expect("user exists")
                .plaintext
        }
    };
    let member_token = user("member", Some(Role::Read)).await;
    let stranger_token = user("stranger", None).await;
    let owner_token = user("owner", Some(Role::Owner)).await;

    RepoFixture {
        router: wiab_inf::http_router(state),
        private_repo,
        public_repo,
        member_token,
        stranger_token,
        owner_token,
    }
}

async fn send(router: &Router, method: Method, path: &str) -> axum::http::Response<Body> {
    send_as(router, method, path, None).await
}

async fn send_as(
    router: &Router,
    method: Method,
    path: &str,
    token: Option<&str>,
) -> axum::http::Response<Body> {
    let mut request = Request::builder().method(method).uri(path);
    if let Some(token) = token {
        request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    let mut request = request.body(Body::empty()).expect("valid request");
    // What `into_make_service_with_connect_info` supplies in the real server. The rate limiter
    // needs a client address to key on, and without one it cannot answer at all.
    request
        .extensions_mut()
        .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 50000))));
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

/// The browse endpoints offer the same contents `git clone` does, so they must apply the same
/// visibility rule. Before this, any authenticated caller could read any private repo in any
/// organization (C2).
#[tokio::test]
async fn private_repo_contents_are_closed_to_non_members() {
    let fixture = repo_fixture().await;
    let paths = [
        format!("/repos/{}", fixture.private_repo),
        format!("/repos/{}/branches", fixture.private_repo),
        format!("/repos/{}/branches/main/files", fixture.private_repo),
        format!(
            "/repos/{}/branches/main/files/raw?path=.env",
            fixture.private_repo
        ),
        format!("/repos/{}/branches/main/commits", fixture.private_repo),
    ];
    for path in &paths {
        let response = send_as(
            &fixture.router,
            Method::GET,
            path,
            Some(&fixture.stranger_token),
        )
        .await;
        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "GET {path} must be closed to a caller with no role"
        );
    }
}

#[tokio::test]
async fn private_repo_contents_are_open_to_members() {
    let fixture = repo_fixture().await;
    let response = send_as(
        &fixture.router,
        Method::GET,
        &format!("/repos/{}", fixture.private_repo),
        Some(&fixture.member_token),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
}

/// A public repo stays anonymously cloneable over git, so an authenticated caller with no role
/// must still be able to browse it — the REST door must not be *stricter* than the git one
/// either.
#[tokio::test]
async fn public_repo_metadata_is_readable_without_a_role() {
    let fixture = repo_fixture().await;
    let response = send_as(
        &fixture.router,
        Method::GET,
        &format!("/repos/{}", fixture.public_repo),
        Some(&fixture.stranger_token),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn listing_a_projects_repos_requires_a_role_on_its_organization() {
    let fixture = repo_fixture().await;
    let stranger = send_as(
        &fixture.router,
        Method::GET,
        "/projects/P-1/repos",
        Some(&fixture.stranger_token),
    )
    .await;
    assert_eq!(stranger.status(), StatusCode::FORBIDDEN);

    let member = send_as(
        &fixture.router,
        Method::GET,
        "/projects/P-1/repos",
        Some(&fixture.member_token),
    )
    .await;
    assert_eq!(member.status(), StatusCode::OK);
}

/// Meetings belong to an organization, so listing them is an organization read. Its sibling
/// `create_meeting` was already gated; this one was not, which is how the participant ids that
/// H2 turned on got handed to any authenticated caller.
#[tokio::test]
async fn listing_meetings_requires_a_role_on_the_organization() {
    let fixture = repo_fixture().await;
    let stranger = send_as(
        &fixture.router,
        Method::GET,
        "/organizations/O-1/meetings",
        Some(&fixture.stranger_token),
    )
    .await;
    assert_eq!(stranger.status(), StatusCode::FORBIDDEN);

    let member = send_as(
        &fixture.router,
        Method::GET,
        "/organizations/O-1/meetings",
        Some(&fixture.member_token),
    )
    .await;
    assert_eq!(member.status(), StatusCode::OK);
}

/// Online password guessing was bounded only by the network. The limiter keys on the client
/// address, which behind nginx comes from `X-Forwarded-For` — see `frontend/nginx.conf`, which
/// overwrites rather than appends so the value cannot be chosen by the client.
#[tokio::test]
async fn repeated_logins_from_one_address_are_throttled() {
    let router = test_router().await;
    let attempt = |ip: &'static str| {
        let router = router.clone();
        async move {
            let request = Request::builder()
                .method(Method::POST)
                .uri("/auth/session")
                .header("X-Forwarded-For", ip)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"email":"ada@example.test","password":"not-the-password"}"#,
                ))
                .expect("valid request");
            router
                .oneshot(request)
                .await
                .expect("router responds")
                .status()
        }
    };

    let mut throttled = None;
    for _ in 0..40 {
        let status = attempt("203.0.113.7").await;
        if status == StatusCode::TOO_MANY_REQUESTS {
            throttled = Some(status);
            break;
        }
    }
    assert!(
        throttled.is_some(),
        "a burst of failed logins from one address must eventually be refused"
    );

    // A different client is unaffected — the limit is per address, not global.
    assert_ne!(
        attempt("198.51.100.4").await,
        StatusCode::TOO_MANY_REQUESTS,
        "another address must not inherit the first one's limit"
    );
}

/// A VM template name becomes a filesystem path under Firecracker and a container image
/// reference under Docker. Both are built by interpolation, so an unvalidated name lets an
/// org member mount an arbitrary host file into a guest, or make the backend pull and run an
/// image from a registry of their choosing (M4).
#[tokio::test]
async fn a_hostile_vm_template_is_refused_before_it_reaches_a_runtime() {
    let fixture = repo_fixture().await;
    for hostile in ["../../etc/passwd", "evil.registry.com/x", "base@sha256:abc"] {
        let body = format!(r#"{{"name":"probe","description":"","vm_type":"{hostile}"}}"#);
        let request = Request::builder()
            .method(Method::POST)
            .uri("/organizations/O-1/agents")
            .header(
                header::AUTHORIZATION,
                format!("Bearer {}", fixture.owner_token),
            )
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body))
            .expect("valid request");
        let response = fixture
            .router
            .clone()
            .oneshot(request)
            .await
            .expect("router responds");
        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "vm_type {hostile:?} must be refused"
        );
    }
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
