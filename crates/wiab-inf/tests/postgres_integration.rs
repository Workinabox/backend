//! End-to-end checks for the PostgreSQL repositories. Ignored by default; run with a live
//! database:
//!
//! ```sh
//! DATABASE_URL=postgres://wiab:wiab@localhost:55432/wiab \
//!   cargo test -p wiab-inf --test postgres_integration -- --ignored
//! ```

use wiab_core::organization::{Organization, OrganizationId, OrganizationRepository};
use wiab_core::project::ProjectId;
use wiab_core::repository::{SaveError, Version};
use wiab_core::user::{SshKey, SshKeyId, User, UserId, UserKind, UserRepository};
use wiab_core::work::{Work, WorkId, WorkRepository};
use wiab_inf::pg_pool;
use wiab_inf::{PostgresOrganizationRepository, PostgresUserRepository, PostgresWorkRepository};

#[tokio::test]
#[ignore = "requires DATABASE_URL pointing at a live Postgres"]
async fn postgres_persistence_end_to_end() {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    // Migrations are idempotent: running twice is a no-op the second time. Both the host's
    // series and authbox's (separate history table) must apply cleanly against a fresh DB.
    let pool = pg_pool::build_pool(&url).await.expect("pool");
    pg_pool::run_migrations(&pool).await.expect("migrate 1");
    pg_pool::run_migrations(&pool)
        .await
        .expect("migrate 2 (idempotent)");
    authbox_inf::run_migrations(&pool)
        .await
        .expect("authbox migrate");
    authbox_inf::run_migrations(&pool)
        .await
        .expect("authbox migrate (idempotent)");

    // Clean slate so the test is repeatable.
    pool.get()
        .await
        .expect("client")
        .batch_execute(
            "TRUNCATE organization, project, agent, board, repo, pipeline, work, work_done, \
             app_user, user_ssh_key, user_access_token, role_assignment",
        )
        .await
        .expect("truncate");

    // --- Organization: insert, read-with-version, optimistic update, stale-conflict. ---
    let orgs = PostgresOrganizationRepository::new(pool.clone());
    let v1 = orgs
        .save(
            Organization::new(OrganizationId::from_number(1), "Acme".into(), String::new())
                .unwrap(),
            Version::NEW,
        )
        .await
        .expect("insert org");

    let (got, got_version) = orgs
        .get(&OrganizationId::from_number(1))
        .await
        .expect("get org")
        .expect("org present");
    assert_eq!(got.name(), "Acme");
    assert_eq!(got_version, v1);

    let mut updated = got;
    updated.update("Acme Inc".into(), "rockets".into()).unwrap();
    let v2 = orgs.save(updated, v1).await.expect("update org");

    // Saving against the now-stale v1 must conflict (the heart of optimistic concurrency).
    let stale =
        Organization::new(OrganizationId::from_number(1), "Nope".into(), String::new()).unwrap();
    assert!(matches!(
        orgs.save(stale, v1).await,
        Err(SaveError::Conflict)
    ));

    let (after_update, after_version) = orgs
        .get(&OrganizationId::from_number(1))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after_update.name(), "Acme Inc");
    assert_eq!(after_version, v2);
    assert_eq!(orgs.list().await.unwrap().len(), 1);

    // --- Work + its `dones` child table: round-trip then mutate-and-persist completion. ---
    let works = PostgresWorkRepository::new(pool.clone());
    let mut work = Work::new(
        WorkId::from_number(1),
        ProjectId::from_number(1),
        "Ship v1".into(),
        String::new(),
    )
    .unwrap();
    let done_id = work.add_done("tests pass".into()).unwrap();
    let work_v1 = works.save(work, Version::NEW).await.expect("insert work");

    let (mut reloaded, reloaded_version) = works
        .get(&WorkId::from_number(1))
        .await
        .unwrap()
        .expect("work present");
    assert_eq!(reloaded.dones().len(), 1, "done survived the child table");
    assert!(!reloaded.is_done());
    assert_eq!(reloaded_version, work_v1);

    reloaded.fulfill_done(&done_id).unwrap();
    works.save(reloaded, work_v1).await.expect("update work");

    let (done_work, _) = works.get(&WorkId::from_number(1)).await.unwrap().unwrap();
    assert!(done_work.is_done(), "fulfilled state persisted");

    // --- User + its `ssh_keys` child table. ---
    let users = PostgresUserRepository::new(pool.clone());
    let mut user = User::new(
        UserId::from_number(1),
        UserKind::Human,
        "Alice".into(),
        Some("alice@example.com".into()),
    )
    .unwrap();
    user.add_ssh_key(
        SshKey::new(
            SshKeyId::new(),
            "laptop".into(),
            "ssh-ed25519 AAAAExample".into(),
            "SHA256:abc".into(),
        )
        .unwrap(),
    );
    users.save(user, Version::NEW).await.expect("insert user");
    let (user_back, _) = users.get(&UserId::from_number(1)).await.unwrap().unwrap();
    assert_eq!(user_back.ssh_keys().len(), 1);
    assert_eq!(user_back.ssh_keys()[0].label(), "laptop");
    assert_eq!(user_back.name(), "Alice");

    // --- Durability across a fresh pool (proxy for a process restart). ---
    drop(orgs);
    drop(pool);
    let pool2 = pg_pool::build_pool(&url).await.expect("reconnect");
    let orgs2 = PostgresOrganizationRepository::new(pool2);
    let (survived, _) = orgs2
        .get(&OrganizationId::from_number(1))
        .await
        .unwrap()
        .expect("org survived reconnect");
    assert_eq!(survived.name(), "Acme Inc");
}
