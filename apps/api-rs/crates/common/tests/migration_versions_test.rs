//! Migration hygiene: every file under `apps/api-rs/migrations/` must carry a
//! unique version number. A duplicate version makes `migrate()` fail with
//! `VersionMismatch` on every boot (2026-10-08/09: two `0002_*` files wedged
//! api + worker startup on the migration advisory lock).

#[test]
fn migration_versions_are_unique() {
    let migrator = sqlx::migrate!("../../migrations");
    let mut versions: Vec<i64> = migrator.iter().map(|m| m.version).collect();
    let total = versions.len();
    versions.sort_unstable();
    versions.dedup();
    assert_eq!(
        total,
        versions.len(),
        "duplicate migration version numbers in apps/api-rs/migrations"
    );
}
