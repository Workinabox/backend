use std::time::Duration;

use crate::messaging::Messaging;
use crate::outbox::Outbox;

/// How many events to take per pass. Small enough that one bad batch is cheap to retry,
/// large enough that a burst drains in a few passes.
const BATCH: i64 = 100;

/// Publish everything waiting in the outbox, oldest first. Returns how many were sent.
///
/// **At-least-once.** An event is deleted only after the broker has accepted it, so a crash
/// between publish and delete resends rather than loses. Consumers must tolerate a repeat,
/// which is the cheaper half of the trade — the alternative loses events outright.
///
/// A publish failure stops the pass rather than skipping the event, so ordering holds: an
/// observer must never see `task.completed` without the `task.started` that preceded it.
pub async fn publish_pending<O: Outbox, M: Messaging>(
    outbox: &O,
    messaging: &M,
) -> anyhow::Result<usize> {
    let pending = outbox.pending(BATCH).await?;
    if pending.is_empty() {
        return Ok(0);
    }

    let mut published = Vec::with_capacity(pending.len());
    for entry in pending {
        let payload = serde_json::to_vec(&entry.event)?;
        match messaging.publish(&entry.event.name, payload).await {
            Ok(()) => published.push(entry.id),
            Err(error) => {
                // Stop here: the next pass retries this event before any that follow it.
                tracing::warn!("outbox publish failed for {}: {error}", entry.event.name);
                break;
            }
        }
    }

    let count = published.len();
    outbox.mark_published(&published).await?;
    Ok(count)
}

/// Drain the outbox forever, waiting `interval` between passes.
///
/// Polling rather than being notified: the writer is a database transaction, and having it
/// signal this task would put the publisher back inside the commit it was separated from.
pub async fn run_publisher<O: Outbox, M: Messaging>(outbox: O, messaging: M, interval: Duration) {
    loop {
        match publish_pending(&outbox, &messaging).await {
            Ok(count) if count > 0 => tracing::debug!("published {count} event(s)"),
            Ok(_) => {}
            // A failing publisher must not take the backend down with it; the events stay
            // in the outbox and the next pass tries again.
            Err(error) => tracing::warn!("outbox publisher pass failed: {error}"),
        }
        tokio::time::sleep(interval).await;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use wiab_core::event::DomainEvent;
    use wiab_core::repository::RepoError;

    use super::*;
    use crate::messaging::MessagingError;
    use crate::outbox::PendingEvent;

    #[derive(Default)]
    struct FakeOutbox {
        pending: Mutex<Vec<PendingEvent>>,
        removed: Mutex<Vec<i64>>,
    }

    impl FakeOutbox {
        fn with(names: &[&str]) -> Self {
            let pending = names
                .iter()
                .enumerate()
                .map(|(index, name)| PendingEvent {
                    id: index as i64 + 1,
                    event: DomainEvent::new(*name, "T-1", serde_json::json!({})),
                })
                .collect();
            Self {
                pending: Mutex::new(pending),
                removed: Mutex::new(Vec::new()),
            }
        }
    }

    impl Outbox for FakeOutbox {
        async fn pending(&self, limit: i64) -> Result<Vec<PendingEvent>, RepoError> {
            Ok(self
                .pending
                .lock()
                .expect("test lock poisoned")
                .iter()
                .take(limit as usize)
                .cloned()
                .collect())
        }

        async fn mark_published(&self, ids: &[i64]) -> Result<(), RepoError> {
            self.removed
                .lock()
                .expect("test lock poisoned")
                .extend_from_slice(ids);
            self.pending
                .lock()
                .expect("test lock poisoned")
                .retain(|entry| !ids.contains(&entry.id));
            Ok(())
        }
    }

    #[derive(Default)]
    struct FakeMessaging {
        sent: Mutex<Vec<String>>,
        /// Refuse everything from this subject onwards.
        fails_from: Option<String>,
    }

    impl Messaging for FakeMessaging {
        async fn publish(&self, subject: &str, _payload: Vec<u8>) -> Result<(), MessagingError> {
            if self.fails_from.as_deref() == Some(subject) {
                return Err(MessagingError::Backend("broker down".to_owned()));
            }
            self.sent
                .lock()
                .expect("test lock poisoned")
                .push(subject.to_owned());
            Ok(())
        }
    }

    #[tokio::test]
    async fn publishes_everything_waiting_and_forgets_it() {
        let outbox = FakeOutbox::with(&["task.started", "task.completed"]);
        let messaging = FakeMessaging::default();

        let count = publish_pending(&outbox, &messaging).await.unwrap();

        assert_eq!(count, 2);
        assert_eq!(
            messaging.sent.lock().unwrap().as_slice(),
            ["task.started", "task.completed"]
        );
        assert_eq!(outbox.removed.lock().unwrap().as_slice(), [1, 2]);
        assert!(outbox.pending.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn an_empty_outbox_does_nothing() {
        let outbox = FakeOutbox::default();
        let messaging = FakeMessaging::default();
        assert_eq!(publish_pending(&outbox, &messaging).await.unwrap(), 0);
        assert!(outbox.removed.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_failure_stops_the_pass_rather_than_skipping_the_event() {
        // Ordering has to hold: nobody may see task.completed without task.started.
        let outbox = FakeOutbox::with(&["task.started", "task.blocked", "task.completed"]);
        let messaging = FakeMessaging {
            fails_from: Some("task.blocked".to_owned()),
            ..FakeMessaging::default()
        };

        let count = publish_pending(&outbox, &messaging).await.unwrap();

        assert_eq!(count, 1);
        assert_eq!(messaging.sent.lock().unwrap().as_slice(), ["task.started"]);
        // The failed event and everything after it are still waiting.
        let waiting: Vec<String> = outbox
            .pending
            .lock()
            .unwrap()
            .iter()
            .map(|entry| entry.event.name.clone())
            .collect();
        assert_eq!(waiting, ["task.blocked", "task.completed"]);
    }

    #[tokio::test]
    async fn an_event_is_forgotten_only_after_the_broker_accepted_it() {
        // The other order would lose events on a crash; this one repeats them, which
        // consumers can defend against.
        let outbox = FakeOutbox::with(&["task.started"]);
        let messaging = FakeMessaging {
            fails_from: Some("task.started".to_owned()),
            ..FakeMessaging::default()
        };

        assert_eq!(publish_pending(&outbox, &messaging).await.unwrap(), 0);
        assert!(outbox.removed.lock().unwrap().is_empty());
        assert_eq!(outbox.pending.lock().unwrap().len(), 1);
    }
}
