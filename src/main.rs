mod model;
mod pypi;
mod pyproject;
mod tui;

use std::path::PathBuf;

use anyhow::Result;

fn main() -> Result<()> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let projects = pyproject::load_projects(&cwd)?;

    if projects.is_empty() {
        println!("No pyproject.toml files with supported dependency arrays found.");
        return Ok(());
    }

    if let Some(summary) = tui::run(projects)? {
        println!(
            "Updated {} dependencies in {} ({})",
            summary.updated.len(),
            summary.project_name,
            summary.file_path.display()
        );
        for (name, from, to) in summary.updated {
            println!("  {name}: {from} -> {to}");
        }
    }

    Ok(())
}
