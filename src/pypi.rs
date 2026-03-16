use std::collections::HashMap;
use std::time::Duration;

use anyhow::{Context, Result};
use pep440_rs::Version;
use serde::Deserialize;

use crate::model::{ChangeKind, DependencyEntry, DependencyKind, DependencyStatus};

const PYPI_API_BASE_URL: &str = "https://pypi.org/pypi";
const REQUEST_RETRY_ATTEMPTS: usize = 3;

#[derive(Debug, Deserialize)]
struct PypiResponse {
    info: Option<PypiInfo>,
}

#[derive(Debug, Deserialize)]
struct PypiInfo {
    version: Option<String>,
}

pub struct PypiClient {
    client: reqwest::blocking::Client,
    cache: HashMap<String, Option<String>>,
}

impl PypiClient {
    pub fn new() -> Result<Self> {
        let client = reqwest::blocking::Client::builder()
            .user_agent("uv-up/1.1.0 (+https://github.com/obviyus/uv-up)")
            .timeout(Duration::from_secs(10))
            .build()
            .context("failed to build HTTP client")?;
        Ok(Self {
            client,
            cache: HashMap::new(),
        })
    }

    pub fn clear_project(&mut self, package_names: impl IntoIterator<Item = String>) {
        for name in package_names {
            self.cache.remove(&name);
        }
    }

    pub fn hydrate_project(&mut self, dependencies: &mut [DependencyEntry]) {
        for dependency in dependencies {
            self.hydrate_dependency(dependency);
        }
    }

    fn hydrate_dependency(&mut self, dependency: &mut DependencyEntry) {
        let (current_version, package_name) = match &dependency.kind {
            DependencyKind::Supported {
                current_version, ..
            } => (current_version.clone(), dependency.name.clone()),
            DependencyKind::Unsupported { reason, .. } => {
                dependency.status = DependencyStatus::Unsupported(reason.clone());
                return;
            }
        };

        let latest_text = match self.fetch_latest_version(&package_name) {
            Ok(value) => value,
            Err(err) => {
                dependency.status = DependencyStatus::Unsupported(err.to_string());
                return;
            }
        };

        let Some(latest_text) = latest_text else {
            dependency.latest_version_text = None;
            dependency.latest_version = None;
            dependency.status = DependencyStatus::NotFound;
            return;
        };

        let latest_version = match latest_text.parse::<Version>() {
            Ok(version) => version,
            Err(_) => {
                dependency.latest_version_text = Some(latest_text.clone());
                dependency.latest_version = None;
                dependency.status = DependencyStatus::Unsupported(format!(
                    "PyPI latest version is not valid PEP 440: {latest_text}"
                ));
                return;
            }
        };

        dependency.latest_version_text = Some(latest_text);
        dependency.latest_version = Some(latest_version.clone());
        dependency.status = if latest_version > current_version {
            DependencyStatus::UpdateAvailable(change_kind(&current_version, &latest_version))
        } else {
            DependencyStatus::UpToDate
        };
    }

    fn fetch_latest_version(&mut self, package_name: &str) -> Result<Option<String>> {
        if let Some(cached) = self.cache.get(package_name) {
            return Ok(cached.clone());
        }

        for attempt in 0..REQUEST_RETRY_ATTEMPTS {
            let response = self
                .client
                .get(format!("{PYPI_API_BASE_URL}/{package_name}/json"))
                .header(reqwest::header::ACCEPT, "application/json")
                .send();

            match response {
                Ok(response) if response.status() == reqwest::StatusCode::NOT_FOUND => {
                    self.cache.insert(package_name.to_string(), None);
                    return Ok(None);
                }
                Ok(response) if response.status().is_success() => {
                    let payload: PypiResponse = response.json().with_context(|| {
                        format!("failed to parse PyPI payload for {package_name}")
                    })?;
                    let version = payload.info.and_then(|info| info.version);
                    self.cache.insert(package_name.to_string(), version.clone());
                    return Ok(version);
                }
                Ok(response)
                    if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS
                        || response.status().is_server_error() =>
                {
                    std::thread::sleep(Duration::from_millis(300 * (1_u64 << attempt)));
                }
                Ok(response) => {
                    anyhow::bail!(
                        "PyPI request failed for {package_name}: HTTP {}",
                        response.status()
                    );
                }
                Err(err) if attempt + 1 < REQUEST_RETRY_ATTEMPTS => {
                    std::thread::sleep(Duration::from_millis(300 * (1_u64 << attempt)));
                    if attempt + 1 == REQUEST_RETRY_ATTEMPTS {
                        return Err(err).with_context(|| format!("failed to fetch {package_name}"));
                    }
                }
                Err(err) => {
                    return Err(err).with_context(|| format!("failed to fetch {package_name}"));
                }
            }
        }

        Ok(None)
    }
}

fn change_kind(current: &Version, latest: &Version) -> ChangeKind {
    let current_release = current.release();
    let latest_release = latest.release();

    let current_major = *current_release.first().unwrap_or(&0);
    let latest_major = *latest_release.first().unwrap_or(&0);
    if latest_major > current_major {
        return ChangeKind::Major;
    }

    let current_minor = *current_release.get(1).unwrap_or(&0);
    let latest_minor = *latest_release.get(1).unwrap_or(&0);
    if latest_minor > current_minor {
        return ChangeKind::Minor;
    }

    if latest > current {
        ChangeKind::Patch
    } else {
        ChangeKind::None
    }
}
