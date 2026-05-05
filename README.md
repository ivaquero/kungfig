# KungFig

KungFig is a cross-platform configuration manager written in Rust.

## Features

- Cross-platform support
- Declarative configuration
- Variable interpolation
- Script execution

## Dependencies

- anyhow: ergonomic error handling
- clap: command line interface
- serde: serialization/deserialization
- serde_json: JSON parsing and merge support
- serde_yaml: YAML parsing and merge support
- toml: read TOML files
- directories: read directory paths
- sha2: compute file and tree digests
- walkdir: traverse directories when copying and hashing

## Quick Start

```bash
rtk cargo run -- list
rtk cargo run -- status
rtk cargo run -- show wezterm
```

## Manifest

The starter manifest lives at [kungfig.toml](kungfig.toml).

### Fields

- items

| field  |   description   |
| :----: | :-------------: |
|  name  |    file name    |
|   id   |     file id     |
| source |   source path   |
| target |   target path   |
|  mode  | operation  mode |
| merge  |   merge  mode   |
|  when  |  when to apply  |
| before |  before apply   |
| after  |   after apply   |

- mode

|    mode    |            description            |    support     |
| :--------: | :-------------------------------: | :------------: |
|   `copy`   |  copy source file to target path  |                |
| `symlink`  |   create symlink to source path   |                |
| `hardlink` |  create hardlink to source path   |                |
|  `merge`   | merge source file and target file | JSON/TOML/YAML |
|  `script`  |  pass source path to run script   |                |

### Built-in Variables

- {backup}
- {home}
- {cache}
- {config}
- {localconfig}
- {appdata}
- {documents}
- {downloads}
- {desktop}

You can also define your own variables:

```toml
[vars]
obsidian_vault = "{documents}/Obsidian"
powershell_profile = "{documents}/PowerShell/Microsoft.PowerShell_profile.ps1"
```

### Template

```toml
[[items]]
name = "zshrc"
id = "zs"
source.macos = "{home}/.zshrc"
target = "{backup}/shell/zshrc.sh"
mode.macos = "symlink"

[[items]]
name = "gitconfig"
id = "g"
source = "{home}/.gitconfig"
target = "{backup}/git/gitconfig"
mode = "copy"

[[items]]
name = "vscode-settings"
id = "vs"
source.macos = "{home}/Library/Application Support/Code/User/settings.json"
source.windows = "{appdata}/Code/User/settings.json"
target = "{backup}/vscode/settings.json"
mode = "copy"
merge = "json"
```

## Modules

### Root Module

- `main.rs`: Entry point of program
- `lib.rs`: Unified interface for CLI commands
- `cli.rs`: Define CLI commands

### core module

Core engine for configuration management.

- `plan.rs`: Generate configuration actions
- `apply.rs`: Apply configuration actions
- `status.rs`: Check configuration status
- `engine.rs`: Main configuration engine

### config module

Configuration parser and validator.

### fs module

File system operations.

### state module

Data storage and retrieval.
