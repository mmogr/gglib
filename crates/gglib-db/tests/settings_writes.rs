//! Two writers on one settings file, each through a pool of its own, as the
//! daemon and `gglib config settings set` are.
//!
//! `SettingsService::update` used to read the whole record, merge, and save it
//! whole, so a write that landed between another writer's read and its save
//! was overwritten by a record read before it (#1034). Each test holds the
//! first writer between its read and its write, gives the second writer time
//! to write if nothing stops it, and then lets the first finish.

use std::sync::{Arc, Barrier};
use std::time::Duration;

use gglib_core::services::SettingsService;
use gglib_core::{Device, Settings, SettingsRepository, SettingsUpdate};
use gglib_db::{SqliteSettingsRepository, setup_database};

fn device(id: &str) -> Device {
    Device {
        id: id.to_owned(),
        label: None,
        joined_at: 1,
        redeemed_at: None,
        last_seen: None,
    }
}

/// Two repositories over one database file, each with a pool of its own.
async fn two_writers() -> (
    tempfile::TempDir,
    Arc<SqliteSettingsRepository>,
    Arc<SqliteSettingsRepository>,
) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("gglib.db");
    let first = setup_database(&path).await.expect("the first pool");
    let second = setup_database(&path).await.expect("the second pool");
    (
        dir,
        Arc::new(SqliteSettingsRepository::new(first)),
        Arc::new(SqliteSettingsRepository::new(second)),
    )
}

/// Holds a change between its read and its write until the test lets it go.
struct Hold {
    entered: Arc<Barrier>,
    released: Arc<Barrier>,
}

impl Hold {
    fn new() -> Self {
        Self {
            entered: Arc::new(Barrier::new(2)),
            released: Arc::new(Barrier::new(2)),
        }
    }

    /// Called from inside the held change.
    fn stop_here(&self) {
        self.entered.wait();
        self.released.wait();
    }

    async fn wait_until_stopped(&self) {
        let entered = Arc::clone(&self.entered);
        tokio::task::spawn_blocking(move || entered.wait())
            .await
            .expect("the held change reached its stop");
    }

    async fn release(&self) {
        let released = Arc::clone(&self.released);
        tokio::task::spawn_blocking(move || released.wait())
            .await
            .expect("the held change was let go");
    }
}

/// Long enough for the second writer to finish its write, if nothing stops it.
const SECOND_WRITER_WINDOW: Duration = Duration::from_millis(300);

/// #1034: a write to one field lands across `invite` recording a device, and
/// the device's roster row survives it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_write_to_another_field_does_not_drop_a_roster_row_written_meanwhile() {
    let (_dir, first, second) = two_writers().await;
    let hold = Arc::new(Hold::new());

    let settings_set = tokio::spawn({
        let hold = Arc::clone(&hold);
        async move {
            first
                .modify(&|settings: &mut Settings| {
                    hold.stop_here();
                    settings.proxy_port = Some(9191);
                    Ok(())
                })
                .await
        }
    });
    hold.wait_until_stopped().await;

    let invite = tokio::spawn({
        let service = SettingsService::new(Arc::clone(&second) as Arc<dyn SettingsRepository>);
        async move {
            service
                .update(SettingsUpdate {
                    remote_devices: Some(Some(vec![device("dev-0a1b2c3d")])),
                    ..SettingsUpdate::default()
                })
                .await
        }
    });
    tokio::time::sleep(SECOND_WRITER_WINDOW).await;
    hold.release().await;

    settings_set
        .await
        .expect("joined")
        .expect("the field is written");
    invite
        .await
        .expect("joined")
        .expect("the roster is written");

    let stored = second.load().await.expect("load");
    assert_eq!(stored.proxy_port, Some(9191));
    assert_eq!(
        stored.remote_devices,
        Some(vec![device("dev-0a1b2c3d")]),
        "the roster row the other writer landed is still there"
    );
}

/// Two writers changing the same field: the second reads what the first wrote
/// rather than a copy from before it, so both changes land.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_writers_adding_to_the_roster_both_land() {
    let (_dir, first, second) = two_writers().await;
    let hold = Arc::new(Hold::new());

    let add_first = tokio::spawn({
        let hold = Arc::clone(&hold);
        async move {
            first
                .modify(&|settings: &mut Settings| {
                    hold.stop_here();
                    settings
                        .remote_devices
                        .get_or_insert_with(Vec::new)
                        .push(device("dev-0a1b2c3d"));
                    Ok(())
                })
                .await
        }
    });
    hold.wait_until_stopped().await;

    let add_second = tokio::spawn({
        let second = Arc::clone(&second);
        async move {
            second
                .modify(&|settings: &mut Settings| {
                    settings
                        .remote_devices
                        .get_or_insert_with(Vec::new)
                        .push(device("dev-11112222"));
                    Ok(())
                })
                .await
        }
    });
    tokio::time::sleep(SECOND_WRITER_WINDOW).await;
    hold.release().await;

    add_first.await.expect("joined").expect("first write");
    add_second.await.expect("joined").expect("second write");

    let ids: Vec<String> = second
        .load()
        .await
        .expect("load")
        .remote_devices
        .unwrap_or_default()
        .into_iter()
        .map(|d| d.id)
        .collect();
    assert_eq!(ids, ["dev-0a1b2c3d", "dev-11112222"]);
}
