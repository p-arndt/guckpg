//! Shared test fixtures. A single place to build `ConnectionConfig` keeps tests
//! compiling when the struct grows a field, and a single place to read the
//! integration-test environment keeps every DB test gated the same way.

use crate::config::{ConnectionConfig, SslMode};

/// A plain local connection; tests override the fields they care about.
pub fn connection(id: &str) -> ConnectionConfig {
    ConnectionConfig {
        id: id.into(),
        name: id.into(),
        host: "localhost".into(),
        port: 5432,
        user: "postgres".into(),
        database: "postgres".into(),
        ssl_mode: SslMode::Prefer,
        ssl_root_cert: None,
        read_only: false,
        color: None,
        ssh: None,
        folder: None,
    }
}

pub fn env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

/// With `GUCKPG_REQUIRE_DB=1` a missing test target fails the test instead of
/// skipping it, so CI cannot report green while running none of the DB tests.
pub fn skip_or_fail(what: &str) {
    if env("GUCKPG_REQUIRE_DB").as_deref() == Some("1") {
        panic!("GUCKPG_REQUIRE_DB=1 but {what} is not set; run `just test-it`");
    }
    eprintln!("SKIPPED: {what} not set (run `just test-it` to run the DB tests)");
}

/// The disposable Postgres from `compose.yaml`, read from `POSTGRES_HOST`,
/// `POSTGRES_PORT`, `POSTGRES_USER`, `POSTGRES_PASSWORD` and `POSTGRES_DB`.
/// Returns the config and password, or `None` (after [`skip_or_fail`]) when
/// any of them is missing.
pub fn test_db(id: &str) -> Option<(ConnectionConfig, String)> {
    let target = (|| {
        let config = ConnectionConfig {
            host: env("POSTGRES_HOST")?,
            port: env("POSTGRES_PORT")?.parse().ok()?,
            user: env("POSTGRES_USER")?,
            database: env("POSTGRES_DB")?,
            ..connection(id)
        };
        Some((config, env("POSTGRES_PASSWORD")?))
    })();
    if target.is_none() {
        skip_or_fail("POSTGRES_HOST/PORT/USER/PASSWORD/DB");
    }
    target
}
