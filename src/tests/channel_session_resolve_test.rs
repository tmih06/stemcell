//! Tests for the shared channel session resolver (Discord/Slack/WhatsApp port
//! of the Telegram suffix-lookup pattern). Issue #121.

use crate::channels::session_resolve::{
    chat_id_suffix, resolve_or_create_channel_session, session_idle_expired,
};
use crate::db::Database;
use crate::services::{ServiceContext, SessionService};

async fn fresh_session_service() -> SessionService {
    let db = Database::connect_in_memory()
        .await
        .expect("in-memory db connect");
    db.run_migrations().await.expect("run migrations");
    SessionService::new(ServiceContext::new(db.pool().clone()))
}

#[test]
fn chat_id_suffix_format() {
    assert_eq!(chat_id_suffix("discord-12345"), "[chat:discord-12345]");
    assert_eq!(chat_id_suffix("wa-+15551234567"), "[chat:wa-+15551234567]");
    assert_eq!(chat_id_suffix("slack-dm-U0ABC"), "[chat:slack-dm-U0ABC]");
}

#[test]
fn idle_window_logic_matches_telegram_helper() {
    let recent = chrono::Utc::now() - chrono::Duration::minutes(30);
    let stale = chrono::Utc::now() - chrono::Duration::hours(2);

    assert!(!session_idle_expired(recent, Some(1.0)));
    assert!(session_idle_expired(stale, Some(1.0)));
    // Disabled idle window: never expired.
    assert!(!session_idle_expired(stale, None));
}

#[tokio::test]
async fn resolves_existing_session_by_suffix() {
    let svc = fresh_session_service().await;
    let suffix = chat_id_suffix("discord-1");
    let legacy = "Discord: #1".to_string();
    let title = format!("{legacy} {suffix}");

    let created = svc
        .create_session(Some(title.clone()))
        .await
        .expect("create");

    let resolved = resolve_or_create_channel_session(
        &svc, &suffix, &legacy, &title, None, "Discord",
    )
    .await
    .expect("resolve");
    assert_eq!(resolved, created.id);
}

#[tokio::test]
async fn suffix_lookup_survives_auto_rename() {
    let svc = fresh_session_service().await;
    let suffix = chat_id_suffix("slack-C0123");
    let legacy = "Slack: #C0123".to_string();
    let title = format!("{legacy} {suffix}");

    let created = svc
        .create_session(Some(title.clone()))
        .await
        .expect("create");

    // Simulate auto-title rewriting the visible label but preserving the suffix.
    let mut renamed = created.clone();
    renamed.title = Some(format!("Deploy planning {suffix}"));
    svc.update_session(&renamed).await.expect("rename");

    let resolved = resolve_or_create_channel_session(
        &svc, &suffix, &legacy, &title, None, "Slack",
    )
    .await
    .expect("resolve");
    assert_eq!(
        resolved, created.id,
        "auto-rename must not orphan the session"
    );
}

#[tokio::test]
async fn forward_migrates_legacy_pre_suffix_row() {
    let svc = fresh_session_service().await;
    let suffix = chat_id_suffix("wa-+15551234567");
    let legacy = "WhatsApp: +15551234567".to_string();
    let title = format!("{legacy} {suffix}");

    // Pre-fix row had no suffix.
    let created = svc
        .create_session(Some(legacy.clone()))
        .await
        .expect("create legacy");

    let resolved = resolve_or_create_channel_session(
        &svc, &suffix, &legacy, &title, None, "WhatsApp",
    )
    .await
    .expect("resolve");

    assert_eq!(resolved, created.id, "must reuse legacy row, not duplicate");

    let after = svc
        .get_session(resolved)
        .await
        .expect("get")
        .expect("exists");
    assert_eq!(
        after.title.as_deref(),
        Some(title.as_str()),
        "legacy row title forward-migrated to include suffix"
    );

    // Second call resolves via suffix path now that the row was migrated.
    let resolved2 = resolve_or_create_channel_session(
        &svc, &suffix, &legacy, &title, None, "WhatsApp",
    )
    .await
    .expect("resolve again");
    assert_eq!(resolved2, created.id);
}

#[tokio::test]
async fn creates_when_no_match_exists() {
    let svc = fresh_session_service().await;
    let suffix = chat_id_suffix("discord-dm-999");
    let legacy = "Discord: DM Alice (999)".to_string();
    let title = format!("{legacy} {suffix}");

    let resolved = resolve_or_create_channel_session(
        &svc, &suffix, &legacy, &title, None, "Discord",
    )
    .await
    .expect("resolve creates");

    let row = svc
        .get_session(resolved)
        .await
        .expect("get")
        .expect("exists");
    assert_eq!(row.title.as_deref(), Some(title.as_str()));
    assert!(!row.is_archived());
}

#[tokio::test]
async fn idle_session_archives_and_creates_new() {
    use rusqlite::params;
    let svc = fresh_session_service().await;
    let suffix = chat_id_suffix("discord-2");
    let legacy = "Discord: #2".to_string();
    let title = format!("{legacy} {suffix}");

    let created = svc
        .create_session(Some(title.clone()))
        .await
        .expect("create");

    // Backdate updated_at via direct SQL since update_session always stamps now().
    let conn = svc.pool().get().await.expect("conn");
    let session_id_str = created.id.to_string();
    let backdated_ts = (chrono::Utc::now() - chrono::Duration::hours(2)).timestamp();
    conn.interact(move |c| {
        c.execute(
            "UPDATE sessions SET updated_at = ?1 WHERE id = ?2",
            params![backdated_ts, session_id_str],
        )
    })
    .await
    .expect("interact")
    .expect("backdate");

    let resolved = resolve_or_create_channel_session(
        &svc,
        &suffix,
        &legacy,
        &title,
        Some(1.0), // 1h idle window
        "Discord",
    )
    .await
    .expect("resolve idle");

    assert_ne!(resolved, created.id, "idle reset must produce a new row");

    let archived = svc
        .get_session(created.id)
        .await
        .expect("get old")
        .expect("exists");
    assert!(archived.is_archived(), "old row must be archived");
}
