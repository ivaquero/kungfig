use std::fs;
use std::path::PathBuf;

use anyhow::{Result, anyhow, bail};
use clap::{Parser, Subcommand};

use crate::core::{
    Manifest, current_platform, get_status, load_manifest, run_action, select_items,
};

#[derive(Parser, Debug)]
#[command(
    name = "kungfig",
    about = "Manage dotfiles and GUI app configs from a TOML manifest."
)]
pub struct Cli {
    #[arg(
        short,
        long,
        default_value = "kungfig.toml",
        help = "Path to the manifest file. Defaults to kungfig.toml."
    )]
    manifest: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    List {
        #[arg(help = "Optional item ids, names, or tags.")]
        selectors: Vec<String>,
    },
    Show {
        #[arg(help = "Item id or exact name.")]
        selector: String,
    },
    Status {
        #[arg(help = "Optional item ids, names, or tags.")]
        selectors: Vec<String>,
    },
    Pull {
        #[arg(help = "Optional item ids, names, or tags.")]
        selectors: Vec<String>,
        #[arg(long, help = "Plan changes without touching the filesystem.")]
        dry_run: bool,
    },
    Push {
        #[arg(help = "Optional item ids, names, or tags.")]
        selectors: Vec<String>,
        #[arg(long, help = "Plan changes without touching the filesystem.")]
        dry_run: bool,
    },
    Init {
        #[arg(long, help = "Overwrite an existing manifest.")]
        force: bool,
    },
}

pub fn run() -> Result<i32> {
    let cli = Cli::parse();

    match cli.command {
        Command::Init { force } => init_manifest(&cli.manifest, force),
        Command::List { selectors } => {
            let manifest = load_manifest(&cli.manifest)?;
            do_list(&manifest, &selectors)
        }
        Command::Show { selector } => {
            let manifest = load_manifest(&cli.manifest)?;
            do_show(&manifest, &selector)
        }
        Command::Status { selectors } => {
            let manifest = load_manifest(&cli.manifest)?;
            do_status(&manifest, &selectors)
        }
        Command::Pull { selectors, dry_run } => {
            let manifest = load_manifest(&cli.manifest)?;
            do_action(&manifest, "pull", &selectors, dry_run)
        }
        Command::Push { selectors, dry_run } => {
            let manifest = load_manifest(&cli.manifest)?;
            do_action(&manifest, "push", &selectors, dry_run)
        }
    }
}

fn do_list(manifest: &Manifest, selectors: &[String]) -> Result<i32> {
    let items = select_items(manifest, selectors)?;
    println!("manifest: {}", manifest.path.display());
    println!("platform: {}", current_platform());
    if !manifest.variables.is_empty() {
        println!();
    }

    for item in items {
        let live_path = item
            .live_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "-".to_string());
        let store_path = item
            .store_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "-".to_string());
        println!(
            "{:<12} {:<8} {} -> {}",
            item.item_id, item.mode, live_path, store_path
        );
    }

    Ok(0)
}

fn do_show(manifest: &Manifest, selector: &str) -> Result<i32> {
    let mut items = manifest
        .items
        .iter()
        .filter(|item| item.item_id == selector || item.name == selector);

    let item = items
        .next()
        .ok_or_else(|| anyhow!("No item matched selector: {selector}"))?;
    if items.next().is_some() {
        bail!("Selector is ambiguous: {selector}");
    }

    let status = get_status(item);
    println!("id:          {}", item.item_id);
    println!("name:        {}", item.name);
    println!("mode:        {}", item.mode);
    println!("merge:       {}", item.merge.as_deref().unwrap_or("-"));
    println!(
        "live_path:   {}",
        item.live_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "-".to_string())
    );
    println!(
        "store_path:  {}",
        item.store_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "-".to_string())
    );
    println!("tags:        {}", join_or_dash(&item.tags));
    println!("when:        {}", join_or_dash(&item.when));
    println!("before:      {}", join_or_dash(&item.before));
    println!("after:       {}", join_or_dash(&item.after));
    println!(
        "description: {}",
        item.description.as_deref().unwrap_or("-")
    );
    println!("status:      {} ({})", status.status, status.detail);

    Ok(0)
}

fn do_status(manifest: &Manifest, selectors: &[String]) -> Result<i32> {
    let items = select_items(manifest, selectors)?;
    println!("manifest: {}", manifest.path.display());
    println!();

    let mut has_drift = false;
    for item in items {
        let status = get_status(item);
        println!(
            "{:<13} {:<12} {}",
            status.status, item.item_id, status.detail
        );
        has_drift |= matches!(
            status.status.as_str(),
            "drift" | "missing" | "missing-live" | "missing-store"
        );
    }

    Ok(if has_drift { 1 } else { 0 })
}

fn do_action(
    manifest: &Manifest,
    action: &str,
    selectors: &[String],
    dry_run: bool,
) -> Result<i32> {
    let results = run_action(manifest, action, selectors, dry_run)?;
    let mut has_error = false;

    for result in results {
        println!(
            "{:<13} {:<12} {}",
            result.status, result.item.item_id, result.detail
        );
        has_error |= result.status == "error";
    }

    Ok(if has_error { 1 } else { 0 })
}

fn init_manifest(path: &PathBuf, force: bool) -> Result<i32> {
    if path.exists() && !force {
        bail!(
            "{} already exists. Re-run with --force to overwrite it.",
            path.display()
        );
    }

    fs::write(path, format!("{}\n", DEFAULT_MANIFEST.trim()))?;
    println!("wrote {}", path.display());
    Ok(0)
}

fn join_or_dash(items: &[String]) -> String {
    if items.is_empty() {
        "-".to_string()
    } else {
        items.join(", ")
    }
}

const DEFAULT_MANIFEST: &str = r#"
[[items]]
name = "zshrc"
id = "zsh"
source.macos = "{home}/.zshrc"
source.linux = "{home}/.zshrc"
target = "{backup}/unix/zshrc"
mode = "symlink"
tags = ["shell", "dotfile"]
description = "Pull as a plain file, push as a symlink back to the live path."

[[items]]
name = "vscode-settings"
id = "vscode"
source.macos = "{appdata}/Code/User/settings.json"
source.linux = "{config}/Code/User/settings.json"
source.windows = "{appdata}/Code/User/settings.json"
target = "{backup}/vscode/settings.json"
mode = "copy"
merge = "json"
tags = ["gui", "editor"]

[[items]]
name = "wezterm"
id = "wezterm"
source.macos = "{config}/wezterm/wezterm.lua"
source.linux = "{config}/wezterm/wezterm.lua"
target = "{backup}/unix/wezterm.lua"
mode = "symlink"
tags = ["terminal", "dotfile"]
"#;
