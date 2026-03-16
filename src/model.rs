use std::collections::BTreeSet;
use std::path::PathBuf;

use pep440_rs::Version;
use pep508_rs::Requirement;

pub type ParsedRequirement = Requirement<pep508_rs::VerbatimUrl>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppMode {
    Project,
    Dependencies,
    Confirm,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DependencySection {
    Project,
    Optional(String),
    Group(String),
}

impl DependencySection {
    pub fn label(&self) -> String {
        match self {
            Self::Project => "project".to_string(),
            Self::Optional(name) => format!("extra:{name}"),
            Self::Group(name) => format!("group:{name}"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpecStrategy {
    Exact,
    Compatible,
    Minimum,
}

impl SpecStrategy {
    pub fn operator(&self) -> &'static str {
        match self {
            Self::Exact => "==",
            Self::Compatible => "~=",
            Self::Minimum => ">=",
        }
    }
}

#[derive(Clone, Debug)]
pub enum DependencyKind {
    Supported {
        requirement: ParsedRequirement,
        current_version: Version,
        strategy: SpecStrategy,
    },
    Unsupported {
        requirement: Option<ParsedRequirement>,
        reason: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChangeKind {
    Major,
    Minor,
    Patch,
    None,
}

impl ChangeKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Major => "MAJOR",
            Self::Minor => "MINOR",
            Self::Patch => "PATCH",
            Self::None => "NONE",
        }
    }
}

#[derive(Clone, Debug)]
pub enum DependencyStatus {
    UpdateAvailable(ChangeKind),
    UpToDate,
    NotFound,
    Unsupported(String),
}

impl DependencyStatus {
    pub fn label(&self) -> &str {
        match self {
            Self::UpdateAvailable(kind) => kind.label(),
            Self::UpToDate => "UP TO DATE",
            Self::NotFound => "NOT FOUND",
            Self::Unsupported(_) => "UNSUPPORTED",
        }
    }

    pub fn detail(&self) -> Option<&str> {
        match self {
            Self::Unsupported(reason) => Some(reason.as_str()),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct DependencyEntry {
    pub name: String,
    pub display_name: String,
    pub section: DependencySection,
    pub item_index: usize,
    pub kind: DependencyKind,
    pub latest_version: Option<Version>,
    pub latest_version_text: Option<String>,
    pub status: DependencyStatus,
}

impl DependencyEntry {
    pub fn current_version_text(&self) -> String {
        match &self.kind {
            DependencyKind::Supported {
                current_version, ..
            } => current_version.to_string(),
            DependencyKind::Unsupported { requirement, .. } => requirement
                .as_ref()
                .and_then(|req| match &req.version_or_url {
                    Some(pep508_rs::VersionOrUrl::VersionSpecifier(specs)) => {
                        Some(specs.to_string())
                    }
                    _ => None,
                })
                .unwrap_or_else(|| "n/a".to_string()),
        }
    }

    pub fn selectable(&self) -> bool {
        matches!(self.status, DependencyStatus::UpdateAvailable(_))
    }
}

#[derive(Clone, Debug)]
pub struct Project {
    pub name: String,
    pub file_path: PathBuf,
    pub dependencies: Vec<DependencyEntry>,
}

impl Project {
    pub fn selected_count(&self, selected: &BTreeSet<usize>) -> usize {
        selected
            .iter()
            .filter(|index| {
                self.dependencies
                    .get(**index)
                    .is_some_and(DependencyEntry::selectable)
            })
            .count()
    }

    pub fn updates_available(&self) -> usize {
        self.dependencies
            .iter()
            .filter(|dep| dep.selectable())
            .count()
    }
}

#[derive(Clone, Debug)]
pub struct UpdateSummary {
    pub project_name: String,
    pub file_path: PathBuf,
    pub updated: Vec<(String, String, String)>,
}
