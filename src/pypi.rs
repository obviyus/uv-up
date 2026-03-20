use std::collections::{HashMap, HashSet};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use pep440_rs::Version;
use serde::Deserialize;

use crate::model::Project;
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
    base_url: String,
    agent: ureq::Agent,
    cache: HashMap<String, Option<String>>,
}

impl PypiClient {
    pub fn new() -> Result<Self> {
        Self::with_base_url(PYPI_API_BASE_URL)
    }

    pub fn with_base_url(base_url: impl Into<String>) -> Result<Self> {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(10)))
            .user_agent("uvlift/1.1.0 (+https://github.com/obviyus/uvlift)")
            .accept("application/json")
            .build();
        Ok(Self {
            base_url: base_url.into(),
            agent: config.into(),
            cache: HashMap::new(),
        })
    }

    pub fn clear_project(&mut self, package_names: impl IntoIterator<Item = String>) {
        for name in package_names {
            self.cache.remove(&name);
        }
    }

    pub fn hydrate_project(&mut self, dependencies: &mut [DependencyEntry]) {
        let latest_versions = self.prefetch_latest_versions(dependencies.iter());
        for dependency in dependencies {
            self.hydrate_dependency(dependency, &latest_versions);
        }
    }

    pub fn hydrate_projects(&mut self, projects: &mut [Project]) {
        let latest_versions = self.prefetch_latest_versions(
            projects
                .iter()
                .flat_map(|project| project.dependencies.iter()),
        );
        for project in projects {
            for dependency in &mut project.dependencies {
                self.hydrate_dependency(dependency, &latest_versions);
            }
        }
    }

    fn hydrate_dependency(
        &self,
        dependency: &mut DependencyEntry,
        latest_versions: &HashMap<String, Result<Option<String>, String>>,
    ) {
        let (current_version, package_name) = match &dependency.kind {
            DependencyKind::Supported {
                current_version, ..
            } => (current_version.clone(), dependency.name.clone()),
            DependencyKind::Unsupported { reason, .. } => {
                dependency.status = DependencyStatus::Unsupported(reason.clone());
                return;
            }
        };

        let latest_text = match latest_versions.get(&package_name) {
            Some(Ok(value)) => value.clone(),
            Some(Err(err)) => {
                dependency.status = DependencyStatus::Unsupported(err.clone());
                return;
            }
            None => None,
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

    fn prefetch_latest_versions<'a>(
        &mut self,
        dependencies: impl IntoIterator<Item = &'a DependencyEntry>,
    ) -> HashMap<String, Result<Option<String>, String>> {
        let package_names = dependencies
            .into_iter()
            .filter_map(|dependency| match dependency.kind {
                DependencyKind::Supported { .. } => Some(dependency.name.clone()),
                DependencyKind::Unsupported { .. } => None,
            })
            .collect::<HashSet<_>>();

        let mut latest_versions = HashMap::with_capacity(package_names.len());
        let mut pending = Vec::new();
        for package_name in package_names {
            if let Some(cached) = self.cache.get(&package_name) {
                latest_versions.insert(package_name, Ok(cached.clone()));
            } else {
                pending.push(package_name);
            }
        }

        for (package_name, result) in self.fetch_latest_versions(&pending) {
            if let Ok(version) = &result {
                self.cache.insert(package_name.clone(), version.clone());
            }
            latest_versions.insert(package_name, result);
        }

        latest_versions
    }

    fn fetch_latest_versions(
        &self,
        package_names: &[String],
    ) -> Vec<(String, Result<Option<String>, String>)> {
        if package_names.is_empty() {
            return Vec::new();
        }

        let worker_cap = if package_names.len() <= 5 { 3 } else { 2 };
        let worker_count = thread::available_parallelism()
            .map(|count| count.get())
            .unwrap_or(1)
            .min(package_names.len())
            .min(worker_cap);
        let chunk_size = package_names.len().div_ceil(worker_count);

        thread::scope(|scope| {
            let mut workers = Vec::with_capacity(worker_count);
            for chunk in package_names.chunks(chunk_size) {
                let agent = self.agent.clone();
                let base_url = self.base_url.clone();
                workers.push(scope.spawn(move || {
                    let mut results = Vec::with_capacity(chunk.len());
                    for package_name in chunk {
                        let result = fetch_latest_version(&agent, &base_url, package_name)
                            .map_err(|err| err.to_string());
                        results.push((package_name.clone(), result));
                    }
                    results
                }));
            }

            workers
                .into_iter()
                .flat_map(|worker| worker.join().expect("PyPI worker panicked"))
                .collect()
        })
    }
}

fn fetch_latest_version(
    agent: &ureq::Agent,
    base_url: &str,
    package_name: &str,
) -> Result<Option<String>> {
    for attempt in 0..REQUEST_RETRY_ATTEMPTS {
        let response = agent.get(format!("{base_url}/{package_name}/json")).call();

        match response {
            Ok(mut response) => {
                let payload: PypiResponse = response
                    .body_mut()
                    .read_json()
                    .with_context(|| format!("failed to parse PyPI payload for {package_name}"))?;
                return Ok(payload.info.and_then(|info| info.version));
            }
            Err(ureq::Error::StatusCode(404)) => return Ok(None),
            Err(ureq::Error::StatusCode(code)) if code == 429 || code >= 500 => {
                thread::sleep(Duration::from_millis(300 * (1_u64 << attempt)));
            }
            Err(ureq::Error::StatusCode(code)) => {
                anyhow::bail!("PyPI request failed for {package_name}: HTTP {code}");
            }
            Err(err) if attempt + 1 < REQUEST_RETRY_ATTEMPTS => {
                thread::sleep(Duration::from_millis(300 * (1_u64 << attempt)));
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

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use anyhow::Result;
    use pep440_rs::Version;
    use pep508_rs::Requirement;

    use super::PypiClient;
    use crate::model::{
        DependencyEntry, DependencyKind, DependencySection, DependencyStatus, Project, SpecStrategy,
    };

    #[test]
    fn hydrate_projects_dedupes_requests_and_reuses_cache() -> Result<()> {
        let requests = Arc::new(AtomicUsize::new(0));
        let server = TestServer::start(requests.clone())?;
        let mut client = PypiClient::with_base_url(server.base_url())?;
        let mut projects = vec![
            Project {
                name: "one".to_string(),
                file_path: "one/pyproject.toml".into(),
                dependencies: vec![
                    dependency("httpx", "0.27.0"),
                    dependency("httpx", "0.27.0"),
                    dependency("rich", "13.7.0"),
                ],
            },
            Project {
                name: "two".to_string(),
                file_path: "two/pyproject.toml".into(),
                dependencies: vec![dependency("httpx", "0.27.0")],
            },
        ];

        client.hydrate_projects(&mut projects);
        assert_eq!(requests.load(Ordering::SeqCst), 2);

        client.hydrate_projects(&mut projects);
        assert_eq!(requests.load(Ordering::SeqCst), 2);
        assert!(projects.iter().all(|project| {
            project
                .dependencies
                .iter()
                .all(|dependency| matches!(dependency.status, DependencyStatus::UpdateAvailable(_)))
        }));

        Ok(())
    }

    fn dependency(name: &str, version: &str) -> DependencyEntry {
        let current_version = version.parse::<Version>().expect("valid version");
        let requirement = format!("{name}>={version}")
            .parse::<Requirement<_>>()
            .expect("valid requirement");
        DependencyEntry {
            name: name.to_string(),
            display_name: name.to_string(),
            current_version_text: version.to_string(),
            section: DependencySection::Project,
            item_index: 0,
            kind: DependencyKind::Supported {
                requirement,
                current_version,
                strategy: SpecStrategy::Minimum,
            },
            latest_version: None,
            latest_version_text: None,
            status: DependencyStatus::Unsupported("not hydrated".to_string()),
        }
    }

    struct TestServer {
        base_url: String,
        _thread: std::thread::JoinHandle<()>,
    }

    impl TestServer {
        fn start(requests: Arc<AtomicUsize>) -> Result<Self> {
            let listener = TcpListener::bind("127.0.0.1:0")?;
            let address = listener.local_addr()?;
            let thread = std::thread::spawn(move || {
                for stream in listener.incoming() {
                    let Ok(mut stream) = stream else {
                        break;
                    };
                    let mut buffer = [0_u8; 1024];
                    let Ok(read) = stream.read(&mut buffer) else {
                        continue;
                    };
                    let request = String::from_utf8_lossy(&buffer[..read]);
                    let path = request
                        .lines()
                        .next()
                        .and_then(|line| line.split_whitespace().nth(1))
                        .unwrap_or("/");
                    requests.fetch_add(1, Ordering::SeqCst);
                    let version = match path {
                        "/httpx/json" => Some("0.28.1"),
                        "/rich/json" => Some("13.9.4"),
                        _ => None,
                    };
                    let (status, body) = match version {
                        Some(version) => {
                            ("200 OK", format!(r#"{{"info":{{"version":"{version}"}}}}"#))
                        }
                        None => ("404 Not Found", "{}".to_string()),
                    };
                    let response = format!(
                        "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = stream.write_all(response.as_bytes());
                }
            });

            Ok(Self {
                base_url: format!("http://{address}"),
                _thread: thread,
            })
        }

        fn base_url(&self) -> &str {
            &self.base_url
        }
    }
}
