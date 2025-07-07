# AGENTS.md — RustRaptor Repository Guide
This file applies to *every* sub-directory unless a deeper `AGENTS.md` overrides a
section. 

## 1  Repository Map
| Path             | Purpose                                   |
|------------------|-------------------------------------------|
| rust-backend/    | Actix-Web API, workers, strategy engine   |
| discord-bot/     | (upcoming) JS/TS Discord command UI       |
| infra/           | Terraform, Docker, Kubernetes manifests   |
| .github/         | CI (`ci.yml`), CD (`deploy.yml`), releases|
| postgres-data/   | Dev-only Postgres volume                  |
| redis-data/      | Dev-only Redis volume                     |

## 2  Global Build / Test
```bash
./scripts/bootstrap.sh                      # deps install
cargo test   --workspace --all-targets
pnpm --filter discord-bot test --run vitest # when front-end exists