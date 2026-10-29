import '.just/common.just'
import '.just/release.just'

ci:
    pnpm check
    pnpm lint
    pnpm test
    pnpm build
    cargo fmt --all --check
    cargo clippy --workspace --all-targets --locked -- -D warnings
    cargo test --workspace --locked

# Start the disposable Postgres and SSH server from compose.yaml.
db-up:
    docker compose up -d --wait

db-down:
    docker compose down

# Run all Rust tests, including the DB integration tests, against compose.yaml.
[unix]
test-it: db-up
    #!/usr/bin/env sh
    set -eu
    # Ports come from compose instead of being hardcoded so an override file
    # that maps other host ports still works.
    pg="$(docker compose port postgres 5432)"
    ssh="$(docker compose port ssh 2222)"
    export POSTGRES_HOST="${pg%:*}" POSTGRES_PORT="${pg##*:}"
    export POSTGRES_USER=guckpg POSTGRES_PASSWORD=guckpg POSTGRES_DB=guckpg
    export SSH_HOST="${ssh%:*}" SSH_PORT="${ssh##*:}" SSH_USER=guckpg SSH_PASSWORD=guckpg
    # The SSH server reaches Postgres by its compose service name.
    export SSH_PG_HOST=postgres
    # Fail instead of skip when a target is missing, so a green run means the
    # DB tests really ran.
    export GUCKPG_REQUIRE_DB=1
    cargo test --workspace --locked
