pub mod add;
pub mod apply;
pub mod backup;
pub mod cli;
pub mod config;
pub mod diff;
pub mod doctor;
pub mod edit;
pub mod error;
pub mod path;
pub mod plan;
pub mod state;
pub mod template;

pub mod recipe {
    use std::env;
    use std::fs;
    use std::path::{Path, PathBuf};

    use anyhow::{Context, Result, bail};
    use serde::Deserialize;

    use crate::add::append_manifest_items;
    use crate::config::{Config, Item, validate_config};

    #[derive(Debug, Clone)]
    pub struct RecipeSummary {
        pub name: String,
        pub item_count: usize,
    }

    #[derive(Debug, Clone)]
    pub struct Recipe {
        pub name: String,
        pub items: Vec<Item>,
        pub raw_text: String,
    }

    #[derive(Debug, Clone)]
    pub struct AddAppResult {
        pub recipe_name: String,
        pub item_names: Vec<String>,
        pub manifest: PathBuf,
    }

    #[derive(Debug, Deserialize)]
    struct RecipeFile {
        name: Option<String>,
        #[serde(default)]
        items: Vec<Item>,
    }

    pub fn manifest_path_for_repo(path: &Path) -> Result<PathBuf> {
        if path.is_absolute() {
            return Ok(path.to_path_buf());
        }

        Ok(env::current_dir()?.join(path))
    }

    pub fn list_recipes(manifest_path: &Path) -> Result<Vec<RecipeSummary>> {
        let recipes_dir = recipe_dir(manifest_path);
        if !recipes_dir.exists() {
            return Ok(Vec::new());
        }

        let mut recipes = Vec::new();
        for entry in fs::read_dir(&recipes_dir)
            .with_context(|| format!("failed to read {}", recipes_dir.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if !path.is_file() || path.extension().and_then(|ext| ext.to_str()) != Some("toml") {
                continue;
            }

            let id = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .ok_or_else(|| anyhow::anyhow!("invalid recipe filename: {}", path.display()))?;
            let recipe = load_recipe(manifest_path, id)?;
            recipes.push(RecipeSummary {
                name: id.to_string(),
                item_count: recipe.items.len(),
            });
        }

        recipes.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(recipes)
    }

    pub fn load_recipe(manifest_path: &Path, recipe_name: &str) -> Result<Recipe> {
        let recipe_path = recipe_dir(manifest_path).join(format!("{recipe_name}.toml"));
        if !recipe_path.exists() {
            bail!("recipe `{recipe_name}` not found");
        }

        let raw_text = fs::read_to_string(&recipe_path)
            .with_context(|| format!("failed to read {}", recipe_path.display()))?;
        let parsed: RecipeFile = toml::from_str(&raw_text)
            .with_context(|| format!("failed to parse {}", recipe_path.display()))?;

        let recipe_display_name = parsed
            .name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .unwrap_or(recipe_name)
            .to_string();

        let config = Config {
            items: parsed.items.clone(),
        };
        validate_config(&config)?;

        Ok(Recipe {
            name: recipe_display_name,
            items: parsed.items,
            raw_text,
        })
    }

    pub fn add_app(
        manifest_path: &Path,
        config: &Config,
        recipe_name: &str,
    ) -> Result<AddAppResult> {
        let recipe = load_recipe(manifest_path, recipe_name)?;
        for item in &recipe.items {
            if config.contains_identifier(item.name.as_str()) {
                bail!(
                    "item `{}` already exists; cannot add recipe `{recipe_name}`",
                    item.name
                );
            }
            if let Some(alias) = item.alias.as_deref() {
                if config.contains_identifier(alias) {
                    bail!("item alias `{alias}` already exists; cannot add recipe `{recipe_name}`");
                }
            }
        }

        append_manifest_items(manifest_path, &recipe.items)?;

        Ok(AddAppResult {
            recipe_name: recipe.name,
            item_names: recipe.items.into_iter().map(|item| item.name).collect(),
            manifest: manifest_path.to_path_buf(),
        })
    }

    fn recipe_dir(manifest_path: &Path) -> PathBuf {
        manifest_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("recipes")
    }
}

pub fn run() -> i32 {
    match cli::run() {
        Ok(code) => code,
        Err(err) => {
            eprintln!("error: {err}");
            1
        }
    }
}
