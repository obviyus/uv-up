use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use pep440_rs::Operator;
use pep508_rs::{Requirement, VersionOrUrl};
use toml_edit::{DocumentMut, Item, Value};
use walkdir::WalkDir;

use crate::model::{
    DependencyEntry, DependencyKind, DependencySection, DependencyStatus, ParsedRequirement,
    Project, SpecStrategy, UpdateSummary,
};

const SKIP_DIRS: &[&str] = &[".git", ".venv", "venv", "node_modules", ".tox", "target"];

pub fn load_projects(root: &Path) -> Result<Vec<Project>> {
    let mut projects = Vec::new();

    for entry in WalkDir::new(root)
        .into_iter()
        .filter_entry(|entry| !should_skip(entry.path()))
    {
        let entry = entry?;
        if !entry.file_type().is_file() || entry.file_name() != "pyproject.toml" {
            continue;
        }

        let file_path = entry.into_path();
        let content = fs::read_to_string(&file_path)
            .with_context(|| format!("failed to read {}", file_path.display()))?;
        let document = content
            .parse::<DocumentMut>()
            .with_context(|| format!("failed to parse {}", file_path.display()))?;
        projects.push(parse_project(file_path, document));
    }

    projects.sort_by(|left, right| left.file_path.cmp(&right.file_path));
    Ok(projects)
}

pub fn update_project(project: &Project, selected: &BTreeSet<usize>) -> Result<UpdateSummary> {
    update_project_with_lock(project, selected, run_uv_lock)
}

fn update_project_with_lock<F>(
    project: &Project,
    selected: &BTreeSet<usize>,
    run_lock: F,
) -> Result<UpdateSummary>
where
    F: FnOnce(&Path) -> Result<()>,
{
    let original = fs::read_to_string(&project.file_path)
        .with_context(|| format!("failed to read {}", project.file_path.display()))?;
    let mut document = original
        .parse::<DocumentMut>()
        .with_context(|| format!("failed to parse {}", project.file_path.display()))?;

    let mut updated = Vec::new();
    for index in selected {
        let dependency = project
            .dependencies
            .get(*index)
            .with_context(|| format!("invalid dependency index {index}"))?;

        if !dependency.selectable() {
            continue;
        }

        let latest = dependency
            .latest_version
            .as_ref()
            .with_context(|| format!("missing latest version for {}", dependency.display_name))?
            .to_string();
        let strategy = match &dependency.kind {
            DependencyKind::Supported { strategy, .. } => strategy.clone(),
            DependencyKind::Unsupported { reason, .. } => anyhow::bail!(
                "cannot update unsupported dependency {}: {reason}",
                dependency.display_name
            ),
        };

        let current = dependency.current_version_text();
        let new_requirement = build_requirement_string(dependency, &latest, &strategy);
        replace_requirement_value(&mut document, dependency, &new_requirement)?;
        updated.push((dependency.display_name.clone(), current, latest));
    }

    if updated.is_empty() {
        anyhow::bail!("no updateable dependencies selected");
    }

    fs::write(&project.file_path, document.to_string())
        .with_context(|| format!("failed to write {}", project.file_path.display()))?;

    if let Err(err) = run_lock(&project.file_path) {
        fs::write(&project.file_path, original).with_context(|| {
            format!(
                "failed to restore {} after uv lock failure",
                project.file_path.display()
            )
        })?;
        return Err(err);
    }

    Ok(UpdateSummary {
        project_name: project.name.clone(),
        file_path: project.file_path.clone(),
        updated,
    })
}

fn parse_project(file_path: PathBuf, document: DocumentMut) -> Project {
    let source_overrides = collect_source_overrides(&document);
    let project_table = document
        .as_table()
        .get("project")
        .and_then(Item::as_table_like);
    let name = project_table
        .and_then(|project| project.get("name"))
        .and_then(|item| item.as_value())
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .unwrap_or_else(|| file_path.display().to_string());

    let mut dependencies = Vec::new();
    if let Some(project_table) = project_table {
        if let Some(item) = project_table.get("dependencies") {
            collect_array_dependencies(
                item,
                DependencySection::Project,
                &source_overrides,
                &mut dependencies,
            );
        }

        if let Some(optional) = project_table
            .get("optional-dependencies")
            .and_then(Item::as_table_like)
        {
            for (extra_name, item) in optional.iter() {
                collect_array_dependencies(
                    item,
                    DependencySection::Optional(extra_name.to_string()),
                    &source_overrides,
                    &mut dependencies,
                );
            }
        }
    }

    if let Some(groups) = document
        .as_table()
        .get("dependency-groups")
        .and_then(Item::as_table_like)
    {
        for (group_name, item) in groups.iter() {
            collect_array_dependencies(
                item,
                DependencySection::Group(group_name.to_string()),
                &source_overrides,
                &mut dependencies,
            );
        }
    }

    Project {
        name,
        file_path,
        dependencies,
    }
}

fn collect_array_dependencies(
    item: &Item,
    section: DependencySection,
    source_overrides: &HashSet<String>,
    out: &mut Vec<DependencyEntry>,
) {
    let Some(array) = item.as_value().and_then(Value::as_array) else {
        return;
    };

    for (index, value) in array.iter().enumerate() {
        let Some(raw) = value.as_str() else {
            continue;
        };
        out.push(parse_dependency_entry(
            raw.to_string(),
            section.clone(),
            index,
            source_overrides,
        ));
    }
}

fn parse_dependency_entry(
    raw: String,
    section: DependencySection,
    item_index: usize,
    source_overrides: &HashSet<String>,
) -> DependencyEntry {
    match raw.parse::<ParsedRequirement>() {
        Ok(requirement) => {
            let name = requirement.name.to_string();
            let display_name = if requirement.extras.is_empty() {
                name.clone()
            } else {
                format!(
                    "{}[{}]",
                    name,
                    requirement
                        .extras
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(",")
                )
            };

            let name_key = normalize_package_name(&name);
            let kind = if source_overrides.contains(&name_key) {
                DependencyKind::Unsupported {
                    requirement: Some(requirement),
                    reason: "managed by tool.uv.sources".to_string(),
                }
            } else {
                classify_requirement(requirement)
            };
            let status = match &kind {
                DependencyKind::Unsupported { reason, .. } => {
                    DependencyStatus::Unsupported(reason.clone())
                }
                DependencyKind::Supported { .. } => DependencyStatus::UpToDate,
            };

            DependencyEntry {
                name,
                display_name,
                section,
                item_index,
                kind,
                latest_version: None,
                latest_version_text: None,
                status,
            }
        }
        Err(err) => DependencyEntry {
            name: raw.clone(),
            display_name: raw.clone(),
            section,
            item_index,
            kind: DependencyKind::Unsupported {
                requirement: None,
                reason: format!("PEP 508 parse failed: {err}"),
            },
            latest_version: None,
            latest_version_text: None,
            status: DependencyStatus::Unsupported(format!("PEP 508 parse failed: {err}")),
        },
    }
}

fn classify_requirement(requirement: Requirement<pep508_rs::VerbatimUrl>) -> DependencyKind {
    let Some(version_or_url) = &requirement.version_or_url else {
        return DependencyKind::Unsupported {
            requirement: Some(requirement),
            reason: "unpinned requirement".to_string(),
        };
    };

    let VersionOrUrl::VersionSpecifier(specifiers) = version_or_url else {
        return DependencyKind::Unsupported {
            requirement: Some(requirement),
            reason: "direct URL requirement".to_string(),
        };
    };

    if specifiers.len() != 1 {
        return DependencyKind::Unsupported {
            requirement: Some(requirement),
            reason: "multi-clause version specifier".to_string(),
        };
    }

    let specifier = specifiers.first().expect("len checked");
    let operator = *specifier.operator();
    let current_version = specifier.version().clone();
    let strategy = match operator {
        Operator::Equal => SpecStrategy::Exact,
        Operator::TildeEqual => SpecStrategy::Compatible,
        Operator::GreaterThanEqual => SpecStrategy::Minimum,
        _ => {
            return DependencyKind::Unsupported {
                requirement: Some(requirement),
                reason: format!("unsupported operator {operator}"),
            };
        }
    };

    DependencyKind::Supported {
        current_version,
        strategy,
        requirement,
    }
}

fn collect_source_overrides(document: &DocumentMut) -> HashSet<String> {
    let mut overrides = HashSet::new();
    if let Some(sources) = document
        .as_table()
        .get("tool")
        .and_then(Item::as_table_like)
        .and_then(|tool| tool.get("uv"))
        .and_then(Item::as_table_like)
        .and_then(|uv| uv.get("sources"))
        .and_then(Item::as_table_like)
    {
        for (name, _) in sources.iter() {
            overrides.insert(normalize_package_name(name));
        }
    }
    overrides
}

fn replace_requirement_value(
    document: &mut DocumentMut,
    dependency: &DependencyEntry,
    new_requirement: &str,
) -> Result<()> {
    let array = match &dependency.section {
        DependencySection::Project => document["project"]["dependencies"]
            .as_value_mut()
            .and_then(Value::as_array_mut)
            .context("project.dependencies is not an array")?,
        DependencySection::Optional(group) => document["project"]["optional-dependencies"][group]
            .as_value_mut()
            .and_then(Value::as_array_mut)
            .with_context(|| format!("project.optional-dependencies.{group} is not an array"))?,
        DependencySection::Group(group) => document["dependency-groups"][group]
            .as_value_mut()
            .and_then(Value::as_array_mut)
            .with_context(|| format!("dependency-groups.{group} is not an array"))?,
    };

    let slot = array
        .get_mut(dependency.item_index)
        .with_context(|| format!("missing array item {}", dependency.item_index))?;
    let quote = detect_quote_style(slot);
    let decor = slot.decor().clone();
    let literal = format_toml_string(new_requirement, quote);
    let mut new_value = literal
        .parse::<Value>()
        .with_context(|| format!("failed to build TOML string {literal}"))?;
    *new_value.decor_mut() = decor;
    *slot = new_value;
    Ok(())
}

fn build_requirement_string(
    dependency: &DependencyEntry,
    latest_version: &str,
    strategy: &SpecStrategy,
) -> String {
    let requirement = match &dependency.kind {
        DependencyKind::Supported { requirement, .. } => requirement,
        DependencyKind::Unsupported { .. } => unreachable!("validated before update"),
    };

    let mut output = dependency.name.clone();
    if !requirement.extras.is_empty() {
        output.push('[');
        output.push_str(
            &requirement
                .extras
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(","),
        );
        output.push(']');
    }
    output.push_str(strategy.operator());
    output.push_str(latest_version);

    if let Some(marker) = requirement.marker.contents() {
        output.push_str(" ; ");
        output.push_str(&marker.to_string());
    }

    output
}

fn run_uv_lock(file_path: &Path) -> Result<()> {
    let project_dir = file_path
        .parent()
        .with_context(|| format!("{} has no parent directory", file_path.display()))?;
    let status = Command::new("uv")
        .arg("lock")
        .current_dir(project_dir)
        .status()
        .context("failed to run `uv lock`")?;

    if !status.success() {
        anyhow::bail!("`uv lock` failed in {}", project_dir.display());
    }
    Ok(())
}

fn should_skip(path: &Path) -> bool {
    path.components().any(|component| {
        let name = component.as_os_str().to_string_lossy();
        SKIP_DIRS.iter().any(|skip| name == *skip)
    })
}

fn detect_quote_style(value: &Value) -> char {
    let repr = value.to_string();
    if repr.starts_with('\'') { '\'' } else { '"' }
}

fn format_toml_string(value: &str, quote: char) -> String {
    match quote {
        '\'' if !value.contains('\'') => format!("'{value}'"),
        _ => {
            let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
            format!("\"{escaped}\"")
        }
    }
}

pub fn normalize_package_name(name: &str) -> String {
    name.chars()
        .map(|ch| match ch {
            '_' | '.' => '-',
            other => other.to_ascii_lowercase(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ChangeKind;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static TEST_COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn parse_project_from_str(content: &str) -> Project {
        parse_project(
            PathBuf::from("pyproject.toml"),
            content.parse::<DocumentMut>().unwrap(),
        )
    }

    fn dependency_index(project: &Project, section: &DependencySection, name: &str) -> usize {
        project
            .dependencies
            .iter()
            .position(|dependency| {
                dependency.section == *section && dependency.display_name == name
            })
            .unwrap()
    }

    fn make_update_available(project: &mut Project, index: usize, latest: &str) {
        let dependency = &mut project.dependencies[index];
        dependency.latest_version = Some(latest.parse().unwrap());
        dependency.latest_version_text = Some(latest.to_string());
        dependency.status = DependencyStatus::UpdateAvailable(ChangeKind::Minor);
    }

    fn make_temp_dir(label: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "uv-up-tests-{label}-{}-{}",
            std::process::id(),
            TEST_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        if path.exists() {
            fs::remove_dir_all(&path).unwrap();
        }
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn write_file(root: &Path, relative: &str, content: &str) -> PathBuf {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn preserves_section_and_item_when_updating() {
        let content = r#"
[project]
dependencies = ["httpx>=0.27.0"]

[project.optional-dependencies]
cli = ["httpx>=0.27.0"]
"#;
        let mut document = content.parse::<DocumentMut>().unwrap();
        let project = parse_project(PathBuf::from("pyproject.toml"), document.clone());
        let dependency = project
            .dependencies
            .iter()
            .find(|dependency| dependency.section == DependencySection::Project)
            .unwrap();

        replace_requirement_value(&mut document, dependency, "httpx>=0.28.1").unwrap();

        let rendered = document.to_string();
        assert!(rendered.contains("dependencies = [\"httpx>=0.28.1\"]"));
        assert!(rendered.contains("cli = [\"httpx>=0.27.0\"]"));
    }

    #[test]
    fn preserves_extras_markers_and_single_quotes_on_update() {
        let content = r#"
[project]
dependencies = ['httpx[http2]>=0.27.0 ; python_version < "3.12"']
"#;
        let mut document = content.parse::<DocumentMut>().unwrap();
        let project = parse_project(PathBuf::from("pyproject.toml"), document.clone());
        let dependency = project.dependencies.first().unwrap();

        let updated = build_requirement_string(dependency, "0.28.1", &SpecStrategy::Minimum);
        replace_requirement_value(&mut document, dependency, &updated).unwrap();

        let rendered = document.to_string();
        assert!(rendered.contains("\"httpx[http2]>=0.28.1 ; python_full_version < '3.12'\""));
    }

    #[test]
    fn preserves_single_quotes_when_safe() {
        let content = r#"
[project]
dependencies = ['httpx>=0.27.0']
"#;
        let mut document = content.parse::<DocumentMut>().unwrap();
        let project = parse_project(PathBuf::from("pyproject.toml"), document.clone());
        let dependency = project.dependencies.first().unwrap();

        replace_requirement_value(&mut document, dependency, "httpx>=0.28.1").unwrap();

        let rendered = document.to_string();
        assert!(rendered.contains("'httpx>=0.28.1'"));
    }

    #[test]
    fn updates_optional_dependency_only() {
        let content = r#"
[project]
name = "demo"
dependencies = ["httpx>=0.27.0"]

[project.optional-dependencies]
cli = ["rich>=13.0.0"]
"#;
        let mut document = content.parse::<DocumentMut>().unwrap();
        let project = parse_project(PathBuf::from("pyproject.toml"), document.clone());
        let dependency = &project.dependencies[dependency_index(
            &project,
            &DependencySection::Optional("cli".to_string()),
            "rich",
        )];

        replace_requirement_value(&mut document, dependency, "rich>=14.0.0").unwrap();

        let rendered = document.to_string();
        assert!(rendered.contains("dependencies = [\"httpx>=0.27.0\"]"));
        assert!(rendered.contains("cli = [\"rich>=14.0.0\"]"));
    }

    #[test]
    fn updates_dependency_group_only() {
        let content = r#"
[project]
name = "demo"
dependencies = ["httpx>=0.27.0"]

[dependency-groups]
dev = ["pytest>=8.0.0"]
"#;
        let mut document = content.parse::<DocumentMut>().unwrap();
        let project = parse_project(PathBuf::from("pyproject.toml"), document.clone());
        let dependency = &project.dependencies[dependency_index(
            &project,
            &DependencySection::Group("dev".to_string()),
            "pytest",
        )];

        replace_requirement_value(&mut document, dependency, "pytest>=8.3.0").unwrap();

        let rendered = document.to_string();
        assert!(rendered.contains("dependencies = [\"httpx>=0.27.0\"]"));
        assert!(rendered.contains("dev = [\"pytest>=8.3.0\"]"));
    }

    #[test]
    fn marks_multi_clause_specifiers_unsupported() {
        let dependency = parse_dependency_entry(
            "httpx>=0.27,<1".to_string(),
            DependencySection::Project,
            0,
            &HashSet::new(),
        );

        assert!(matches!(
            dependency.status,
            DependencyStatus::Unsupported(_)
        ));
    }

    #[test]
    fn marks_unpinned_and_direct_url_requirements_unsupported() {
        for raw in [
            "httpx",
            "httpx @ https://example.com/httpx.whl",
            "gitpkg @ git+https://github.com/example/project",
        ] {
            let dependency = parse_dependency_entry(
                raw.to_string(),
                DependencySection::Project,
                0,
                &HashSet::new(),
            );

            assert!(matches!(
                dependency.status,
                DependencyStatus::Unsupported(_)
            ));
        }
    }

    #[test]
    fn marks_unsupported_operators() {
        for raw in [
            "httpx!=0.27.0",
            "httpx<0.27.0",
            "httpx<=0.27.0",
            "httpx>0.27.0",
            "httpx===0.27.0",
        ] {
            let dependency = parse_dependency_entry(
                raw.to_string(),
                DependencySection::Project,
                0,
                &HashSet::new(),
            );

            assert!(matches!(
                dependency.status,
                DependencyStatus::Unsupported(_)
            ));
        }
    }

    #[test]
    fn marks_source_override_unsupported() {
        let mut overrides = HashSet::new();
        overrides.insert("httpx".to_string());

        let dependency = parse_dependency_entry(
            "httpx>=0.27".to_string(),
            DependencySection::Project,
            0,
            &overrides,
        );

        assert!(matches!(
            dependency.status,
            DependencyStatus::Unsupported(_)
        ));
    }

    #[test]
    fn normalizes_source_override_names() {
        let content = r#"
[project]
dependencies = ["foo-bar-baz>=1.0.0"]

[tool.uv.sources]
"foo_bar.baz" = { path = "./vendor/foo" }
"#;
        let project = parse_project_from_str(content);

        assert!(matches!(
            project.dependencies.first().unwrap().status,
            DependencyStatus::Unsupported(_)
        ));
    }

    #[test]
    fn supports_exact_compatible_and_minimum() {
        for raw in ["httpx==0.27.0", "httpx~=0.27.0", "httpx>=0.27.0"] {
            let dependency = parse_dependency_entry(
                raw.to_string(),
                DependencySection::Project,
                0,
                &HashSet::new(),
            );
            assert!(matches!(dependency.kind, DependencyKind::Supported { .. }));
        }
    }

    #[test]
    fn load_projects_scans_multiple_sections_and_skips_vendor_dirs() {
        let root = make_temp_dir("scan");
        write_file(
            &root,
            "apps/api/pyproject.toml",
            r#"
[project]
name = "api"
dependencies = ["httpx>=0.27.0"]

[project.optional-dependencies]
cli = ["rich>=13.0.0"]

[dependency-groups]
dev = ["pytest>=8.0.0"]
"#,
        );
        write_file(
            &root,
            "libs/tool/pyproject.toml",
            r#"
[project]
name = "tool"
dependencies = ["typer>=0.12.0"]
"#,
        );
        write_file(
            &root,
            "node_modules/ignored/pyproject.toml",
            r#"
[project]
name = "ignored"
dependencies = ["bad>=1.0.0"]
"#,
        );
        write_file(
            &root,
            ".venv/ignored/pyproject.toml",
            r#"
[project]
name = "ignored-two"
dependencies = ["bad>=1.0.0"]
"#,
        );

        let projects = load_projects(&root).unwrap();

        assert_eq!(projects.len(), 2);
        let api = projects
            .iter()
            .find(|project| project.name == "api")
            .unwrap();
        assert_eq!(api.dependencies.len(), 3);
        assert!(
            api.dependencies
                .iter()
                .any(|dependency| dependency.section == DependencySection::Project)
        );
        assert!(
            api.dependencies
                .iter()
                .any(|dependency| dependency.section
                    == DependencySection::Optional("cli".to_string()))
        );
        assert!(api
            .dependencies
            .iter()
            .any(|dependency| dependency.section == DependencySection::Group("dev".to_string())));
    }

    #[test]
    fn update_project_runs_lock_and_writes_selected_items() {
        let root = make_temp_dir("update-success");
        let file_path = write_file(
            &root,
            "pyproject.toml",
            r#"
[project]
name = "demo"
dependencies = ["httpx>=0.27.0"]

[project.optional-dependencies]
cli = ["rich>=13.0.0"]
"#,
        );
        let mut project = load_projects(&root).unwrap().pop().unwrap();
        let project_index = dependency_index(&project, &DependencySection::Project, "httpx");
        let optional_index = dependency_index(
            &project,
            &DependencySection::Optional("cli".to_string()),
            "rich",
        );
        make_update_available(&mut project, project_index, "0.28.1");
        make_update_available(&mut project, optional_index, "14.0.0");

        let mut selected = BTreeSet::new();
        selected.insert(project_index);
        selected.insert(optional_index);

        let summary = update_project_with_lock(&project, &selected, |path| {
            assert_eq!(path, file_path.as_path());
            Ok(())
        })
        .unwrap();

        let rendered = fs::read_to_string(&file_path).unwrap();
        assert!(rendered.contains("dependencies = [\"httpx>=0.28.1\"]"));
        assert!(rendered.contains("cli = [\"rich>=14.0.0\"]"));
        assert_eq!(summary.updated.len(), 2);
    }

    #[test]
    fn update_project_rolls_back_when_lock_fails() {
        let root = make_temp_dir("update-rollback");
        let file_path = write_file(
            &root,
            "pyproject.toml",
            r#"
[project]
name = "demo"
dependencies = ["httpx>=0.27.0"]
"#,
        );
        let original = fs::read_to_string(&file_path).unwrap();
        let mut project = load_projects(&root).unwrap().pop().unwrap();
        let dependency_index = dependency_index(&project, &DependencySection::Project, "httpx");
        make_update_available(&mut project, dependency_index, "0.28.1");

        let mut selected = BTreeSet::new();
        selected.insert(dependency_index);

        let error = update_project_with_lock(&project, &selected, |_| {
            anyhow::bail!("lock failed on purpose")
        })
        .unwrap_err();

        assert!(error.to_string().contains("lock failed on purpose"));
        assert_eq!(fs::read_to_string(&file_path).unwrap(), original);
    }

    #[test]
    fn marks_invalid_requirement_syntax_unsupported() {
        let dependency = parse_dependency_entry(
            "!not-a-valid-requirement".to_string(),
            DependencySection::Project,
            0,
            &HashSet::new(),
        );

        assert!(matches!(
            dependency.status,
            DependencyStatus::Unsupported(_)
        ));
    }
}
