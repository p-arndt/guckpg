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
