//! End-to-end checks for the PostgreSQL repositories. Ignored by default; run with a live
//! database:
//!
//! ```sh
//! DATABASE_URL=postgres://wiab:wiab@localhost:55432/wiab \
//!   cargo test -p wiab-inf --test postgres_integration -- --ignored
//! ```

use wiab_app::Outbox;
use wiab_core::board::BoardId;
use wiab_core::organization::{Organization, OrganizationId, OrganizationRepository};
use wiab_core::project::ProjectId;
use wiab_core::repo::RepoId;
use wiab_core::repository::{SaveError, Version};
use wiab_core::task::{Task, TaskId, TaskRepository, TaskState};
use wiab_core::team::{Team, TeamId, TeamRepository, TeamState};
use wiab_core::user::{SshKey, SshKeyId, User, UserId, UserKind, UserRepository};
use wiab_core::vm::{VmId, VmTemplate};
use wiab_core::work::{Work, WorkId, WorkRepository};
use wiab_inf::pg_pool;
use wiab_inf::{
    PostgresOrganizationRepository, PostgresOutbox, PostgresTaskRepository, PostgresTeamRepository,
    PostgresUserRepository, PostgresWorkRepository,
};

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
             app_user, user_ssh_key, user_access_token, role_assignment, team, task, outbox",
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

    // --- Team: the lifecycle columns (state, vm_id) must survive a round-trip. ---
    let teams = PostgresTeamRepository::new(pool.clone());
    let mut team = Team::new(
        TeamId::from_number(1),
        OrganizationId::from_number(1),
        "platform".into(),
        "the platform team".into(),
        BoardId::from_number(1),
        RepoId::from_number(7),
        UserId::from_number(1),
        VmTemplate::new("developer".to_owned()).unwrap(),
    )
    .unwrap();
    let team_v1 = teams
        .save(team.clone(), Version::NEW)
        .await
        .expect("insert");

    team.start().unwrap();
    team.mark_idle(VmId::from_number(5)).unwrap();
    teams.save(team, team_v1).await.expect("update team");

    let (team_back, _) = teams
        .get(&TeamId::from_number(1))
        .await
        .unwrap()
        .expect("team present");
    assert_eq!(team_back.state(), TeamState::Idle);
    assert_eq!(team_back.vm_id(), Some(VmId::from_number(5)));
    assert_eq!(teams.list().await.unwrap().len(), 1);

    // --- Task: the board pull. Two teams claiming at once must not get the same task. ---
    let tasks = PostgresTaskRepository::new(pool.clone());
    let board = BoardId::from_number(1);
    for number in [1, 2] {
        tasks
            .save(
                Task::new(TaskId::from_number(number), board, WorkId::from_number(1)),
                Version::NEW,
            )
            .await
            .expect("insert task");
    }

    let (first, second) = tokio::join!(
        tasks.claim_next(&board, TeamId::from_number(1)),
        tasks.claim_next(&board, TeamId::from_number(2)),
    );
    let claimed: Vec<TaskId> = [first, second]
        .into_iter()
        .map(|outcome| outcome.expect("claim").expect("a task was waiting").0.id())
        .collect();
    assert_eq!(
        claimed
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len(),
        2,
        "concurrent claims took the same task"
    );

    // Both are held now, so a third claim finds nothing.
    assert!(
        tasks
            .claim_next(&board, TeamId::from_number(3))
            .await
            .unwrap()
            .is_none(),
        "an empty board must yield nothing, not a held task"
    );

    // An escalated task goes back on the board and is claimable again.
    let (mut task, version) = tasks.get(&claimed[0]).await.unwrap().expect("task present");
    assert_eq!(task.state(), TaskState::Assigned);
    task.start().unwrap();
    task.escalate("needs a decision".to_owned()).unwrap();
    tasks.save(task, version).await.expect("escalate");

    let (reclaimed, _) = tasks
        .claim_next(&board, TeamId::from_number(3))
        .await
        .unwrap()
        .expect("the escalated task is back on the board");
    assert_eq!(reclaimed.id(), claimed[0]);
    assert_eq!(reclaimed.assignee(), Some(TeamId::from_number(3)));
    assert_eq!(reclaimed.reason(), None, "the stale reason is cleared");

    // --- Outbox: events land with the row that produced them, in one transaction. ---
    let outbox = PostgresOutbox::new(pool.clone());
    let waiting = outbox.pending(100).await.expect("read outbox");
    let names: Vec<&str> = waiting.iter().map(|e| e.event.name.as_str()).collect();
    // The team and task work above drove real transitions, so their events are here in the
    // order they happened.
    assert!(
        names.contains(&"team.starting") && names.contains(&"team.started"),
        "team lifecycle events were not written with the row: {names:?}"
    );
    assert!(
        names.contains(&"task.assigned"),
        "claiming a task wrote no event: {names:?}"
    );
    assert!(
        waiting.windows(2).all(|pair| pair[0].id < pair[1].id),
        "the outbox must come back in the order things happened"
    );

    // A rejected save writes nothing: the transaction that carried the events rolled back.
    let before = outbox.pending(1000).await.unwrap().len();
    let (mut conflicting, _) = teams.get(&TeamId::from_number(1)).await.unwrap().unwrap();
    conflicting.stop();
    assert!(matches!(
        teams.save(conflicting, Version::NEW).await,
        Err(SaveError::Conflict)
    ));
    assert_eq!(
        outbox.pending(1000).await.unwrap().len(),
        before,
        "a conflicting save must not leave its events behind"
    );

    // Publishing forgets them.
    let ids: Vec<i64> = waiting.iter().map(|entry| entry.id).collect();
    outbox.mark_published(&ids).await.expect("mark published");
    let remaining = outbox.pending(100).await.unwrap();
    let left: Vec<&str> = remaining.iter().map(|e| e.event.name.as_str()).collect();
    assert!(
        !left.iter().any(|name| names.contains(name)),
        "published events are still waiting: {left:?}"
    );

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
