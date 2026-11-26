#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
pnpm --dir native-host build
pnpm --dir native-host test:screen
pnpm --dir native-host test:workspace
pnpm --dir native-host test:navigation
pnpm --dir native-host test:interaction
pnpm --dir native-host test:data
pnpm --dir native-host test:editor
pnpm --dir native-host test:views
pnpm --dir native-host test:autosave
pnpm --dir native-host test:chapters
pnpm --dir native-host test:cli-script
pnpm --dir native-host test:connect
node --test native-host/scripts/contracts.test.mjs
pnpm --dir native-host exec node --import tsx --test scripts/chapter03-04.test.mjs
pnpm --dir native-host exec node --import tsx --test scripts/chapter06.test.mjs
pnpm --dir native-host exec node --import tsx --test scripts/chapter08.test.mjs
pnpm --dir native-host exec node --import tsx --test scripts/chapter09.test.mjs
pnpm --dir native-host exec node --import tsx --test scripts/chapter10.test.mjs
pnpm --dir native-host exec node --import tsx --test scripts/agent-session-chip.test.mjs
pnpm --dir native-host exec node --import tsx --test scripts/code-syntax.test.mjs
pnpm --dir native-host exec node --import tsx --test scripts/repo-links.test.mjs
pnpm --dir native-host exec node --import tsx --test scripts/resources-ui.test.mjs
pnpm --dir native-host exec node --import tsx --test scripts/tasks-ui.test.mjs
pnpm --dir native-host exec node --import tsx --test scripts/rail-tabs.test.mjs
pnpm --dir native-host exec node --import tsx --test scripts/typography.test.mjs
pnpm --dir native-host exec node --import tsx --test scripts/settings.test.mjs
pnpm --dir native-host exec node --import tsx --test scripts/sub-tabs-ui.test.mjs
pnpm --dir native-host exec node --import tsx --test scripts/architecture-model.test.mjs scripts/architecture-layout.test.mjs scripts/architecture-icons.test.mjs
pnpm --dir native-host exec node --import tsx --test scripts/notifications-ui.test.mjs
pnpm --dir native-host exec node --import tsx --test scripts/runner-ui.test.mjs
pnpm --dir native-host exec node --import tsx --test scripts/approvals-ui.test.mjs
pnpm --dir native-host exec node --import tsx --test scripts/part4-ask-run.test.mjs
pnpm --dir native-host exec node --import tsx --test scripts/engine-route.test.mjs
pnpm --dir native-host exec node --import tsx --test scripts/client-store-chassis.test.mjs
pnpm --dir native-host exec node --import tsx --test scripts/host-commands.test.mjs
pnpm --dir native-host exec node --import tsx --test scripts/popover-dismiss.test.mjs
pnpm --dir native-host exec node --import tsx --test scripts/response-types.test.mjs
pnpm --dir native-host exec node --import tsx --test scripts/settings-types.test.mjs
pnpm --dir native-host exec node --import tsx --test scripts/skin.test.mjs
pnpm --dir native-host exec node --import tsx --test scripts/workspace-surface.test.mjs
cargo test --manifest-path native-host/src-tauri/Cargo.toml --locked
