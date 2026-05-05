# KungFig

KungFig is a cross-platform configuration manager written in Rust.

## Features

- Cross-platform support
- Declarative configuration
- Variable interpolation
- Tags and `when` conditions
- Plan / diff / apply / sync workflow
- Backup and rollback support
- SQLite state tracking
- Template rendering
- Recipe import and app bootstrap

## Dependencies

- clap: command line interface
- serde: serialization/deserialization
- toml: read TOML files
- directories: read directory paths
- similar: render text diffs
- sha2: compute file and tree digests
- hex: encode hashes
- rusqlite: persist managed files and backups
- anyhow: ergonomic error handling
- thiserror: typed internal errors

## Quick Start

- `init`

Create a starter manifest in the current directory, or overwrite an existing one with `--force`.

```bash
cargo run -- init
cargo run -- init --force
```

- `plan`

Preview the actions KungFig would take, and narrow the output to tagged items when needed.

```bash
cargo run -- plan
cargo run -- plan --tag git
```

- `diff`

Inspect full per-item diffs, or focus on a single managed item by name.

```bash
cargo run -- diff
cargo run -- diff gitconfig
```

- `apply`

Materialize sources into their targets, or do a safe dry run before changing anything.

```bash
cargo run -- apply
cargo run -- apply --dry-run
```

- `status`

Check which items are synced, modified, or pending, optionally filtered by tag.

```bash
cargo run -- status
cargo run -- status --tag core
```

- `rollback`

Restore the latest backup for managed items, which is especially useful after a bad edit.

```bash
cargo run -- rollback
cargo run -- rollback --tag git
```

- `sync`

Pull the latest repository changes, then apply the manifest and report git status.

```bash
cargo run -- sync
```

- `doctor`

Run a quick health check for the manifest, source tree, targets, git, and local state paths.

```bash
cargo run -- doctor
```

## Workflow

### Command Entry

```mermaid
flowchart TD
    U[User] --> CLI[KungFig CLI]
    CLI --> INIT[init]
    CLI --> PLAN[plan]
    CLI --> DIFF[diff]
    CLI --> APPLY[apply]
    CLI --> STATUS[status]
    CLI --> ROLLBACK[rollback]
    CLI --> SYNC[sync]
    CLI --> DOCTOR[doctor]
    CLI --> RECIPE[recipe / add-app]
    INIT --> WRITE[write starter kungfig.toml]
```

### Manifest Execution

```mermaid
flowchart TD
    LOAD[load_config / validate_config] --> FILTER[filter by tag / when]
    FILTER --> RESOLVE[resolve_items]
    RESOLVE --> BUILD[build_plan]

    BUILD --> PLANOUT[plan output]
    BUILD --> DIFFITEM[diff_item]
    BUILD --> PREVIEW[preview_plan]
    BUILD --> APPLYPLAN[apply_plan]
    BUILD --> COLLECT[collect_status]
    BUILD --> ROLLBACKOP[rollback_items]

    APPLYPLAN --> BACKUP[backup_target]
    APPLYPLAN --> STORE[StateStore]
    BACKUP --> STORE
    COLLECT --> STORE
    ROLLBACKOP --> STORE
```

### Sync And Ops

```mermaid
flowchart TD
    SYNC[sync] --> PULL[git pull]
    PULL -->|success| LOAD[load_config]
    PULL -->|fail| STOP[exit early]
    LOAD --> APPLYPLAN[apply]
    APPLYPLAN --> GSTATUS[git status --short]

    DOCTOR[doctor] --> CHECKS[manifest / config / repo / targets / git / state / backup]
    RECIPE[recipe / add-app] --> RECIPEPATH[manifest_path_for_repo]
    RECIPEPATH --> RECIPEOPS[list_recipes / load_recipe / add_app]
```

## Manifest

The starter manifest lives at [kungfig.toml](kungfig.toml).

### Fields

- items

| field  |                description                |
| :----: | :---------------------------------------: |
|  name  |                 file name                 |
| source |                source path                |
| target | target path, or per-platform target table |
|  mode  |              operation  mode              |
|  tags  |            optional item tags             |
|  when  |        optional platform condition        |

- mode

|    mode    |               description                |
| :--------: | :--------------------------------------: |
|   `copy`   |     copy source file to target path      |
| `symlink`  |  create a target symlink to source path  |
| `template` | render source as a template before write |

- tags filter items with `--tag` on `plan`, `diff`, `apply`, `status`, and `rollback`.
- when supports expressions like `when = "os == 'linux'"` or `when = "os != 'windows'"`.
- `sync` runs `git pull`, then `apply`, then reports `git status`.

### Built-in Variables

- {home}
- {cache}
- {config}
- {data}
- {appdata}
- {localappdata}
- {documents}

Current path expansion is built-in only.

### Example

```toml
[[items]]
name = "gitconfig"
source = "dotfiles/gitconfig"
target = "{home}/.gitconfig"
mode = "copy"
tags = ["git", "core"]

[[items]]
name = "vscode-settings"
source = "apps/vscode/settings.json"
target.macos = "{home}/Library/Application Support/Code/User/settings.json"
target.linux = "{config}/Code/User/settings.json"
target.windows = "{appdata}/Code/User/settings.json"
mode = "copy"
when = "os == 'macos'"

[[items]]
name = "wezterm-config"
source = "apps/wezterm/config.lua"
target = "{config}/wezterm/wezterm.lua"
mode = "template"
```

## Modules

### Root Module

- `main.rs`: Entry point of program
- `lib.rs`: Unified interface for CLI commands
- `cli.rs`: Define `init / plan / diff / apply / status / rollback / sync / doctor`
- `config.rs`: Parse and validate `kungfig.toml`
- `path.rs`: Expand variables and perform path/file helpers
- `plan.rs`: Resolve items and build execution plans
- `diff.rs`: Render file diffs
- `backup.rs`: Create backups before overwrite
- `apply.rs`: Apply plan actions and restore backups
- `state.rs`: Persist managed files and backups in SQLite
- `error.rs`: Shared custom error types

## Build

Build the binary locally:

```bash
cargo build
```

Build an optimized release binary:

```bash
cargo build --release
```

Run the full test suite while developing:

```bash
cargo test
```

If you prefer using the executable directly, Cargo will place it under `target/debug/` or `target/release/` depending on the build mode.
