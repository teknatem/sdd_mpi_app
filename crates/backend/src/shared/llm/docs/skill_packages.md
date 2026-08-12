# Skill packages

The configured external skills directory is the registry's only runtime source.
On startup the backend creates it when necessary and writes missing embedded
seed skills there without replacing an existing skill with the same id. It
supports both legacy `<id>.md` files and package directories containing
`SKILL.md`.

```text
marketplace-funnel-analysis/
├── SKILL.md
├── references/
├── examples/
├── schemas/
└── scripts/
```

`references/` and `examples/` are indexed as lazy resources. JavaScript files
under `scripts/` are automatically exposed as development tasks using their
file stem as task id and the exported `run` function.

Production tasks should be declared explicitly:

```yaml
---
id: marketplace-funnel-analysis
title: Marketplace funnel analysis
intents: [marketplace_funnel_analysis]
tools: [find_data_sources, preview_data]
tasks:
  - id: calculate-funnel
    title: Calculate funnel
    runtime: javascript
    entrypoint: scripts/calculate-funnel.mjs
    export: run
    mode: stable
    input_schema: schemas/calculate-funnel-input.json
    capabilities: [network:none]
---
```

Task modules use the server QuickJS contract:

```javascript
export async function run(args, host) {
  host.log.info("calculate", args.rows.length);
  return { rows: args.rows.length };
}
```

The runtime has no shell, filesystem, environment or network globals. Database
access is available only through the existing plugin host and declared
`db:read:<scope>` capabilities. Resource and task paths are canonicalized and
must remain inside the package.

Changes become active only after `POST /api/llm-skills/reload` or the
“Перезагрузить skills” button. Reload scans and validates the entire catalog,
then atomically activates a new numbered snapshot. In-flight chat messages keep
their original snapshot, including prompts, resources, schemas and script
sources.

`read_skill_resource` is paged. Start with `offset: 0`; the result contains
`next_offset` and `truncated`. A single call returns at most 32,000 characters.

The reload response contains `generation`, `catalog_digest`, diagnostics and
the added/changed/removed skill IDs. If catalog construction fails critically,
the response status is `rejected` and the previous snapshot remains active.

The repository contains `marketplace-funnel-analysis/` as the reference package
for multi-file skills. Deploy/copy non-seed package directories into the
configured `[llm].skills_path`; package content remains reloadable without
recompiling the backend. No catalog next to the executable is scanned.
