//! Regression tests: `migrate()` must fail fast (not hang) when another
//! session holds sqlx's migration advisory lock, and must never leak the lock
//! itself (2026-10-08/09: a leaked lock wedged api startup forever).
//!
//! Run with the backend app containers STOPPED (plane-db stays up):
//!   docker compose -f docker-compose-local.yml stop api worker beat-worker
//!   DATABASE_URL=postgres://plane:plane@localhost:5432/plane \
//!     cargo test -p common --test migrate_lock_test

use std::sync::{Mutex, OnceLock};
use std::time::Duration;

/// The two tests below must never overlap: the contention test holds sqlx's
/// migration advisory lock while the leak test asserts no session holds one.
/// A process-wide mutex keeps them serial however cargo schedules threads.
fn serial() -> &'static Mutex<()> {
    static SERIAL: OnceLock<Mutex<()>> = OnceLock::new();
    SERIAL.get_or_init(|| Mutex::new(()))
}

/// sqlx 0.7.4's migration lock key: `0x3d32ad9e * CRC32_ISO_HDLC(db_name)`
/// (`sqlx-postgres-0.7.4/src/migrate.rs::generate_lock_id`).
fn sqlx_migration_lock_id(database_name: &str) -> i64 {
    const CRC_IEEE: crc::Crc<u32> = crc::Crc::<u32>::new(&crc::CRC_32_ISO_HDLC);
    0x3d32ad9e * (CRC_IEEE.checksum(database_name.as_bytes()) as i64)
}

#[tokio::test]
async fn migrate_fails_fast_when_lock_is_held() {
    let _guard = serial().lock().unwrap();
    let cfg = common::config::AppConfig::from_env();
    let holder = common::db::create_pool(&cfg).await;
    let db_name: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&holder)
        .await
        .unwrap();
    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(sqlx_migration_lock_id(&db_name))
        .execute(&holder)
        .await
        .unwrap();

    let started = std::time::Instant::now();
    let outcome = tokio::time::timeout(
        Duration::from_secs(25),
        common::db::migrate_with_statement_timeout(&cfg.database_url, "5s"),
    )
    .await;
    let elapsed = started.elapsed();

    sqlx::query("SELECT pg_advisory_unlock($1)")
        .bind(sqlx_migration_lock_id(&db_name))
        .execute(&holder)
        .await
        .unwrap();

    let result = outcome.expect("migrate() hung waiting for the migration advisory lock");
    assert!(result.is_err(), "migrate() should fail while the lock is held");
    assert!(
        elapsed < Duration::from_secs(25),
        "migrate() took too long to fail ({elapsed:?})"
    );
}

#[tokio::test]
async fn migrate_leaves_no_advisory_lock_behind() {
    let _guard = serial().lock().unwrap();
    let cfg = common::config::AppConfig::from_env();
    tokio::time::timeout(
        Duration::from_secs(120),
        common::db::migrate(&cfg.database_url),
    )
    .await
    .expect("migrate() hung")
    .expect("migrate() failed on a healthy DB");
    let pool = common::db::create_pool(&cfg).await;
    let held: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_locks WHERE locktype = 'advisory' AND granted AND pid <> pg_backend_pid()",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(held, 0, "migrate() leaked a postgres advisory lock");
}
