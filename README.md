# RustRaptor

RustRaptor is an algorithmic trading framework built primarily in **Rust** with a companion Node.js Discord bot. The backend exposes a REST API via **Actix-Web**, stores data in **Postgres**, uses **Redis** for caching and jobs, and ships metrics/logs to **Prometheus** and **Loki**. Docker images and compose files are provided for local development.

## Table of Contents
- [Repository Layout](#repository-layout)
- [Requirements](#requirements)
- [Quick Start](#quick-start)
- [Running Tests](#running-tests)
- [Deployment](#deployment)
- [Contributing](#contributing)
- [License](#license)

## Repository Layout
| Path | Description |
|------|-------------|
| `rust-backend/` | Actix-Web API, workers and trading strategy engine |
| `discord-bot/` | Node.js Discord command interface |
| `infra/` | Docker Compose files for Grafana/Prometheus/Loki |
| `.github/` | Continuous integration workflows |
| `docker-compose.yml` | Compose stack for Postgres/Redis/backends |

## Requirements
- **Rust** 1.86 or newer
- **Node.js** 20 with [`pnpm`](https://pnpm.io/)
- **Docker** & **Docker Compose**

## Quick Start
1. Copy the sample environment file and edit secrets:
   ```bash
   cp rust-backend/.env.example rust-backend/.env
   ```
2. Build and launch the stack:
   ```bash
   docker-compose up --build
   ```
3. (Optional) start observability tools:
   ```bash
   docker compose -f infra/docker-compose.observability.yml up
   ```
4. The API will be reachable on <http://localhost:8080>.

## Running Tests
Backend tests (from `rust-backend/`):
```bash
cargo test --workspace --all-targets
```

Discord bot tests (from `discord-bot/`):
```bash
pnpm install --offline
pnpm run lint
pnpm run vitest --coverage
```

## Deployment
Dockerfiles are located in `rust-backend/` and `discord-bot/`. Build them directly or use the compose stack above to run everything locally.

## Contributing
Run the tests and linters listed above before opening a pull request. Additional development notes and commands live in the various `AGENTS.md` files.

## License
Copyright (c) 2025 HapticFish

All rights reserved.

This software and all associated files are the confidential and proprietary property of HapticFish and may not be used, copied, modified, distributed, or disclosed in whole or in part without express written permission.

Unauthorized access, duplication, or redistribution is strictly prohibited.

Use of this software is restricted to approved and authenticated users under the terms of a separate licensing agreement or contract.

To request access or licensing information, please contact:
rustraptortrades@gmail.com