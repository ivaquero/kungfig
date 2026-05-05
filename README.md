# KungFig

KungFig is a cross-platform configuration manager written in Rust.

## Features

- Cross-platform support
- Declarative configuration
- Variable interpolation
- Script execution

## Dependencies

- clap: command line interface
- serde: serialization/deserialization
- toml: read TOML files
- directories: read directory paths
- camino: use UTF-8 encoded paths
- ignore: ignore files and directories when scanning

## Modules

### Root Module

- `main.rs`: Entry point of program
- `lib.rs`: Unified interface for CLI commands
- `cli.rs`: Define CLI commands

### core

Core engine for configuration management.

### config

Configuration parser and validator.

### fs

File system operations.

### state

Data storage and retrieval.

## Manifest

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

### Variables

- {backup}
- {home}
- {cache}
- {config}
- {localconfig}
- {appdata}
- {documents}
- {downloads}
- {desktop}

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
