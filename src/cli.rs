use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, Output};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};

use crate::add::add_item;
use crate::apply::{apply_plan, preview_plan, rollback_items};
use crate::config::{Config, load_config, load_config_or_empty};
use crate::diff::diff_item;
use crate::doctor::run_doctor;
use crate::edit::edit_target;
use crate::plan::{Action, build_plan, resolve_items};
use crate::recipe::{add_app, list_recipes, load_recipe, manifest_path_for_repo};
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
    Plan {
        #[arg(long)]
        tag: Option<String>,
    },
    Diff {
        name: Option<String>,
        #[arg(long)]
        summary: bool,
        #[arg(long)]
        tag: Option<String>,
    },
    Apply {
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        tag: Option<String>,
    },
    Status {
        #[arg(long)]
        tag: Option<String>,
    },
    Rollback {
        #[arg(long)]
        tag: Option<String>,
    },
    Doctor,
    Sync,
    Add {
        path: String,
        #[arg(long)]
        name: String,
    },
    Edit {
        name: Option<String>,
    },
    Recipe {
        #[command(subcommand)]
        command: RecipeCommand,
    },
    AddApp {
        name: String,
    },
}

#[derive(Subcommand, Debug)]
enum RecipeCommand {
    List,
    Show { name: String },
}

pub fn run() -> Result<i32> {
    let cli = Cli::parse();

    match cli.command {
        Command::Init { force } => init_manifest(&cli.manifest, force),
        Command::Plan { tag } => {
            let (manifest_path, config) = load_config(&cli.manifest)?;
            run_plan(&config, &manifest_path, tag.as_deref())
        }
        Command::Diff { name, summary, tag } => {
            let (manifest_path, config) = load_config(&cli.manifest)?;
            run_diff(
                &config,
                &manifest_path,
                name.as_deref(),
                summary,
                tag.as_deref(),
            )
        }
        Command::Apply { dry_run, tag } => {
            let (manifest_path, config) = load_config(&cli.manifest)?;
            run_apply(&config, &manifest_path, dry_run, tag.as_deref())
        }
        Command::Status { tag } => {
            let (manifest_path, config) = load_config(&cli.manifest)?;
            run_status(&config, &manifest_path, tag.as_deref())
        }
        Command::Rollback { tag } => {
            let (manifest_path, config) = load_config(&cli.manifest)?;
            run_rollback(&config, &manifest_path, tag.as_deref())
        }
        Command::Doctor => run_doctor_cli(&cli.manifest),
        Command::Sync => run_sync(&cli.manifest),
        Command::Add { path, name } => {
            let (manifest_path, config) = load_config_or_empty(&cli.manifest)?;
            run_add(&config, &manifest_path, &path, &name)
        }
        Command::Edit { name } => {
            let (manifest_path, config) = load_config(&cli.manifest)?;
            run_edit(&config, &manifest_path, name.as_deref())
        }
        Command::Recipe { command } => run_recipe_command(&cli.manifest, command),
        Command::AddApp { name } => run_add_app(&cli.manifest, &name),
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

fn run_plan(config: &Config, manifest_path: &Path, tag: Option<&str>) -> Result<i32> {
    let selected = select_config(config, None, tag)?;
    if selected.items.is_empty() {
        println!("no matching items");
        return Ok(0);
    }

    let store = StateStore::open()?;
    let plan = build_plan(&selected, manifest_path, Some(&store))?;
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

fn run_diff(
    config: &Config,
    manifest_path: &Path,
    name: Option<&str>,
    summary: bool,
    tag: Option<&str>,
) -> Result<i32> {
    let selected = select_config(config, name, tag)?;
    if selected.items.is_empty() {
        println!("no matching items");
        return Ok(0);
    }

    if summary {
        let store = StateStore::open()?;
        let plan = build_plan(&selected, manifest_path, Some(&store))?;
        let mut has_conflict = false;
        let mut printed = false;

        for action in plan.actions {
            match action {
                Action::Create { name, target, .. } => {
                    printed = true;
                    println!("create   {:<18} {}", name, target.display());
                }
                Action::Update {
                    name,
                    target,
                    change_state,
                    ..
                } => {
                    printed = true;
                    println!(
                        "update   {:<18} {} ({})",
                        name,
                        target.display(),
                        change_state
                    );
                }
                Action::Conflict {
                    name,
                    target,
                    reason,
                } => {
                    printed = true;
                    has_conflict = true;
                    println!("conflict {:<18} {} ({reason})", name, target.display());
                }
                Action::Skip { .. } => {}
            }
        }

        if !printed {
            println!("no changes");
        }

        return Ok(if has_conflict { 1 } else { 0 });
    }

    for item in resolve_items(&selected, manifest_path)? {
        println!("== {} ==", item.name);
        println!("source: {}", item.source.display());
        println!("target: {}", item.target.display());
        println!("{}", diff_item(&item)?);
    }

    Ok(0)
}

fn run_apply(
    config: &Config,
    manifest_path: &Path,
    dry_run: bool,
    tag: Option<&str>,
) -> Result<i32> {
    let selected = select_config(config, None, tag)?;
    if selected.items.is_empty() {
        println!("no matching items");
        return Ok(0);
    }

    let store = StateStore::open()?;
    let plan = build_plan(&selected, manifest_path, Some(&store))?;
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

fn run_status(config: &Config, manifest_path: &Path, tag: Option<&str>) -> Result<i32> {
    let selected = select_config(config, None, tag)?;
    if selected.items.is_empty() {
        println!("no matching items");
        return Ok(0);
    }

    let store = StateStore::open()?;
    let statuses = collect_status(&resolve_items(&selected, manifest_path)?, &store)?;
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

fn run_rollback(config: &Config, manifest_path: &Path, tag: Option<&str>) -> Result<i32> {
    let selected = select_config(config, None, tag)?;
    if selected.items.is_empty() {
        println!("no matching items");
        return Ok(0);
    }

    let store = StateStore::open()?;
    let results = rollback_items(&resolve_items(&selected, manifest_path)?, &store)?;
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

fn run_sync(manifest: &Path) -> Result<i32> {
    let manifest_path = resolve_manifest_path(manifest)?;
    let repo_root = manifest_path.parent().unwrap_or_else(|| Path::new("."));

    let pull = run_git(repo_root, &["pull"])?;
    emit_output(&pull);
    if !pull.status.success() {
        return Ok(1);
    }

    let mut has_problem = false;

    match load_config(&manifest_path) {
        Ok((manifest_path, config)) => {
            let apply_code = run_apply(&config, &manifest_path, false, None)?;
            has_problem |= apply_code != 0;
        }
        Err(err) => {
            eprintln!("error: {err}");
            has_problem = true;
        }
    }

    let status = run_git(repo_root, &["status", "--short"])?;
    if status.stdout.is_empty() && status.stderr.is_empty() {
        println!("git status clean");
    } else {
        emit_output(&status);
    }
    has_problem |= !status.status.success();

    Ok(if has_problem { 1 } else { 0 })
}

fn run_add(config: &Config, manifest_path: &Path, path: &str, name: &str) -> Result<i32> {
    let store = StateStore::open()?;
    let result = add_item(manifest_path, config, path, name, &store)?;
    println!(
        "added    {:<18} {} -> {}",
        result.name,
        result.target.display(),
        result.source.display()
    );
    println!("manifest {}", result.manifest.display());
    Ok(0)
}

fn run_edit(config: &Config, manifest_path: &Path, name: Option<&str>) -> Result<i32> {
    let result = edit_target(config, manifest_path, name)?;
    println!("opened   {:<18} {}", result.editor, result.target.display());
    Ok(0)
}

fn run_recipe_command(manifest: &Path, command: RecipeCommand) -> Result<i32> {
    let manifest_path = manifest_path_for_repo(manifest)?;

    match command {
        RecipeCommand::List => {
            let recipes = list_recipes(&manifest_path)?;
            if recipes.is_empty() {
                println!("no recipes found");
            } else {
                for recipe in recipes {
                    println!("{:<18} {} items", recipe.name, recipe.item_count);
                }
            }
            Ok(0)
        }
        RecipeCommand::Show { name } => {
            let recipe = load_recipe(&manifest_path, &name)?;
            print!("{}", recipe.raw_text.trim_end());
            println!();
            Ok(0)
        }
    }
}

fn run_add_app(manifest: &Path, recipe_name: &str) -> Result<i32> {
    let manifest_path = manifest_path_for_repo(manifest)?;
    let (_, config) = load_config_or_empty(&manifest_path)?;
    let result = add_app(&manifest_path, &config, recipe_name)?;
    println!(
        "added recipe {:<11} {} items",
        result.recipe_name,
        result.item_names.len()
    );
    for item_name in result.item_names {
        println!("item     {item_name}");
    }
    println!("manifest {}", result.manifest.display());
    Ok(0)
}

fn run_git(repo_root: &Path, args: &[&str]) -> Result<Output> {
    ProcessCommand::new("git")
        .args(args)
        .current_dir(repo_root)
        .output()
        .with_context(|| format!("failed to run git {}", args.join(" ")))
}

fn emit_output(output: &Output) {
    if !output.stdout.is_empty() {
        print!("{}", String::from_utf8_lossy(&output.stdout));
        if !String::from_utf8_lossy(&output.stdout).ends_with('\n') {
            println!();
        }
    }

    if !output.stderr.is_empty() {
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
        if !String::from_utf8_lossy(&output.stderr).ends_with('\n') {
            eprintln!();
        }
    }
}

fn resolve_manifest_path(manifest: &Path) -> Result<PathBuf> {
    if manifest.is_absolute() {
        return Ok(manifest.to_path_buf());
    }

    Ok(std::env::current_dir()?.join(manifest))
}

fn select_config(config: &Config, name: Option<&str>, tag: Option<&str>) -> Result<Config> {
    if let Some(name) = name {
        let exists = config.items.iter().any(|item| item.name == name);
        if !exists {
            bail!("item `{name}` not found");
        }
    }

    config.filtered(name, tag)
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
