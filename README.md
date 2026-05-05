# KungFig

KungFig is a cross-platform configuration manager written in Rust.

## Features

- Cross-platform support
- Declarative configuration
- Variable interpolation
- Plan / diff / apply workflow
- Backup and rollback support
- SQLite state tracking

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

```bash
cargo run -- init
cargo run -- plan
cargo run -- diff
cargo run -- apply
cargo run -- status
cargo run -- rollback
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

- mode

|   mode    |              description               |
| :-------: | :------------------------------------: |
|  `copy`   |    copy source file to target path     |
| `symlink` | create a target symlink to source path |

### Built-in Variables

- {home}
- {cache}
- {config}
- {data}
- {appdata}
- {localappdata}
- {documents}

Current path expansion is built-in only.

### Template

```toml
[[items]]
name = "gitconfig"
source = "dotfiles/gitconfig"
target = "{home}/.gitconfig"
mode = "copy"

[[items]]
name = "vscode-settings"
source = "apps/vscode/settings.json"
target.macos = "{home}/Library/Application Support/Code/User/settings.json"
target.linux = "{config}/Code/User/settings.json"
target.windows = "{appdata}/Code/User/settings.json"
mode = "copy"
```

## Modules

### Root Module

- `main.rs`: Entry point of program
- `lib.rs`: Unified interface for CLI commands
- `cli.rs`: Define `init / plan / diff / apply / status / rollback`
- `config.rs`: Parse and validate `kungfig.toml`
- `path.rs`: Expand variables and perform path/file helpers
- `plan.rs`: Resolve items and build execution plans
- `diff.rs`: Render file diffs
- `backup.rs`: Create backups before overwrite
- `apply.rs`: Apply plan actions and restore backups
- `state.rs`: Persist managed files and backups in SQLite
- `error.rs`: Shared custom error types
