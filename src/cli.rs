use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use clap::{Parser, Subcommand};

use crate::apply::{apply_plan, preview_plan, rollback_items};
use crate::config::{Config, load_config};
use crate::diff::diff_item;
use crate::doctor::run_doctor;
use crate::plan::{Action, build_plan, resolve_items};
use crate::state::{StateStore, collect_status};

#[derive(Parser, Debug)]
#[command(
    name = "kungfig",
    about = "A minimal cross-platform config management CLI."
)]
pub struct Cli {
    #[arg(short, long, default_value = "kungfig.toml")]
    manifest: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    Init {
        #[arg(long)]
        force: bool,
    },
    Plan,
    Diff,
    Apply {
        #[arg(long)]
        dry_run: bool,
    },
    Status,
    Rollback,
    Doctor,
}

pub fn run() -> Result<i32> {
    let cli = Cli::parse();

    match cli.command {
        Command::Init { force } => init_manifest(&cli.manifest, force),
        Command::Plan => {
            let (manifest_path, config) = load_config(&cli.manifest)?;
            run_plan(&config, &manifest_path)
        }
        Command::Diff => {
            let (manifest_path, config) = load_config(&cli.manifest)?;
            run_diff(&config, &manifest_path)
        }
        Command::Apply { dry_run } => {
            let (manifest_path, config) = load_config(&cli.manifest)?;
            run_apply(&config, &manifest_path, dry_run)
        }
        Command::Status => {
            let (manifest_path, config) = load_config(&cli.manifest)?;
            run_status(&config, &manifest_path)
        }
        Command::Rollback => {
            let (manifest_path, config) = load_config(&cli.manifest)?;
            run_rollback(&config, &manifest_path)
        }
        Command::Doctor => run_doctor_cli(&cli.manifest),
    }
}

fn init_manifest(path: &Path, force: bool) -> Result<i32> {
    if path.exists() && !force {
        bail!(
            "{} already exists. Re-run with --force to overwrite it.",
            path.display()
        );
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, format!("{}\n", DEFAULT_MANIFEST.trim()))?;
    println!("wrote {}", path.display());
    Ok(0)
}

fn run_plan(config: &Config, manifest_path: &Path) -> Result<i32> {
    let store = StateStore::open()?;
    let plan = build_plan(config, manifest_path, Some(&store))?;
    let mut has_conflict = false;

    for action in plan.actions {
        match action {
            Action::Create {
                name,
                source,
                target,
                mode,
            } => println!(
                "create   {:<18} {:<7} {} -> {}",
                name,
                mode,
                source.display(),
                target.display()
            ),
            Action::Update {
                name,
                source,
                target,
                mode,
                change_state,
            } => println!(
                "update   {:<18} {:<7} {} -> {} ({})",
                name,
                mode,
                source.display(),
                target.display(),
                change_state
            ),
            Action::Skip { name, reason } => {
                println!("skip     {:<18} {}", name, reason);
            }
            Action::Conflict {
                name,
                target,
                reason,
            } => {
                has_conflict = true;
                println!("conflict {:<18} {} ({reason})", name, target.display());
            }
        }
    }

    Ok(if has_conflict { 1 } else { 0 })
}

fn run_diff(config: &Config, manifest_path: &Path) -> Result<i32> {
    for item in resolve_items(config, manifest_path)? {
        println!("== {} ==", item.name);
        println!("source: {}", item.source.display());
        println!("target: {}", item.target.display());
        println!("{}", diff_item(&item)?);
    }

    Ok(0)
}

fn run_apply(config: &Config, manifest_path: &Path, dry_run: bool) -> Result<i32> {
    let store = StateStore::open()?;
    let plan = build_plan(config, manifest_path, Some(&store))?;
    let results = if dry_run {
        preview_plan(&plan)
    } else {
        apply_plan(&plan, &store)?
    };
    let mut has_problem = false;

    for result in results {
        println!("{:<9} {:<18} {}", result.status, result.name, result.detail);
        has_problem |= matches!(result.status.as_str(), "error" | "conflict");
    }

    Ok(if has_problem { 1 } else { 0 })
}

fn run_status(config: &Config, manifest_path: &Path) -> Result<i32> {
    let store = StateStore::open()?;
    let statuses = collect_status(&resolve_items(config, manifest_path)?, &store)?;
    let mut has_changes = false;

    for status in statuses {
        println!(
            "{:<14} {:<18} {}",
            status.status, status.name, status.detail
        );
        has_changes |= status.status != "synced";
    }

    Ok(if has_changes { 1 } else { 0 })
}

fn run_rollback(config: &Config, manifest_path: &Path) -> Result<i32> {
    let store = StateStore::open()?;
    let results = rollback_items(&resolve_items(config, manifest_path)?, &store)?;
    let mut has_error = false;

    for result in results {
        println!("{:<9} {:<18} {}", result.status, result.name, result.detail);
        has_error |= result.status == "error";
    }

    Ok(if has_error { 1 } else { 0 })
}

fn run_doctor_cli(manifest: &Path) -> Result<i32> {
    let checks = run_doctor(manifest)?;
    let mut has_error = false;

    for check in checks {
        println!("{:<8} {:<18} {}", check.status, check.name, check.detail);
        has_error |= check.status == "error";
    }

    Ok(if has_error { 1 } else { 0 })
}

const DEFAULT_MANIFEST: &str = r#"
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
"#;
