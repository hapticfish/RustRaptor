
---

## 2 │ Backend-specific `AGENTS.md`  ➜ `/RustRaptor/rust-backend/AGENTS.md`

```markdown
# AGENTS.md — Rust Backend (Actix-Web)

## 2  Build / Test Override
```bash
# deterministic build for CI & agents
cargo clean                       # start from a fresh target dir
cargo check --workspace --locked  # fast sanity pass
cargo clippy --all-targets --all-features --deny warnings
cargo test   --workspace --all-features --locked
cargo tarpaulin --out Xml --fail-under 90
