use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Serialize;

use crate::path::current_platform;

#[derive(Debug, Clone, Serialize)]
pub struct TemplateContext {
    pub os: String,
    pub arch: String,
    pub hostname: String,
    pub username: String,
    pub profile: String,
    pub vars: BTreeMap<String, String>,
}

pub fn render_template_source(source: &Path) -> Result<String> {
    let text = fs::read_to_string(source)
        .with_context(|| format!("failed to read template {}", source.display()))?;
    render_template_text(&text)
}

pub fn render_template_text(text: &str) -> Result<String> {
    let env = minijinja::Environment::new();
    env.render_str(text, template_context())
        .context("failed to render template")
}

fn template_context() -> TemplateContext {
    TemplateContext {
        os: current_platform().to_string(),
        arch: env::consts::ARCH.to_string(),
        hostname: env::var("HOSTNAME")
            .or_else(|_| env::var("COMPUTERNAME"))
            .unwrap_or_default(),
        username: env::var("USER")
            .or_else(|_| env::var("USERNAME"))
            .unwrap_or_default(),
        profile: env::var("KUNGFIG_PROFILE").unwrap_or_else(|_| "default".to_string()),
        vars: template_vars(),
    }
}

fn template_vars() -> BTreeMap<String, String> {
    let mut vars = BTreeMap::new();

    for (key, value) in env::vars() {
        if let Some(stripped) = key.strip_prefix("KUNGFIG_VAR_") {
            vars.insert(stripped.to_ascii_lowercase(), value);
        }
    }

    vars
}
