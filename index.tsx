#!/usr/bin/env bun
import React, { useState, useEffect, useCallback } from "react";
import { render, Text, Box, useInput, useApp } from "ink";
import { readdir } from "node:fs/promises";

interface Dependency {
	name: string;
	currentVersion: string;
	latestVersion?: string | null;
	selected: boolean;
	hasUpdate: boolean;
	loading: boolean;
}

interface ProjectInfo {
	name: string;
	dependencies: Dependency[];
	filePath: string;
}

const fetchLatestVersion = async (
	packageName: string,
): Promise<string | null> => {
	try {
		const response = await fetch(`https://pypi.org/pypi/${packageName}/json`);
		if (!response.ok) return null;

		const data: any = await response.json();
		return data.info?.version || null;
	} catch (error) {
		return null;
	}
};

const compareVersions = (current: string, latest: string): boolean => {
	// Clean current version by removing operators and whitespace
	const cleanCurrent = current.replace(/[>=<~!]/g, "").trim();
	try {
		// Use Bun's built-in semver comparison
		return Bun.semver.order(cleanCurrent, latest) === -1;
	} catch {
		// Fallback to string comparison if semver parsing fails
		return cleanCurrent !== latest;
	}
};

const getVersionChangeType = (
	current: string,
	latest: string,
): "major" | "minor" | "patch" | "none" => {
	const cleanCurrent = current.replace(/[>=<~!]/g, "").trim();
	try {
		const currentParts = cleanCurrent.split(".").map((v) => parseInt(v) || 0);
		const latestParts = latest.split(".").map((v) => parseInt(v) || 0);

		const currentMajor = currentParts[0] || 0;
		const currentMinor = currentParts[1] || 0;
		const latestMajor = latestParts[0] || 0;
		const latestMinor = latestParts[1] || 0;

		if (latestMajor > currentMajor) return "major";
		if (latestMinor > currentMinor) return "minor";
		if (Bun.semver.order(cleanCurrent, latest) === -1) return "patch";
		return "none";
	} catch {
		return "minor"; // Default fallback
	}
};

const App: React.FC = () => {
	const [projects, setProjects] = useState<ProjectInfo[]>([]);
	const [selectedProjectIndex, setSelectedProjectIndex] = useState(0);
	const [selectedDepIndex, setSelectedDepIndex] = useState(0);
	const [mode, setMode] = useState<"project" | "dependencies" | "confirm">(
		"project",
	);
	const [loading, setLoading] = useState(true);
	const { exit } = useApp();

	const fetchVersionsForProject = useCallback(async (project: ProjectInfo) => {
		const promises = project.dependencies.map(async (dep, index) => {
			const latestVersion = await fetchLatestVersion(dep.name);

			setProjects((prev) =>
				prev.map((p) =>
					p.filePath === project.filePath
						? {
								...p,
								dependencies: p.dependencies.map((d, i) =>
									i === index
										? {
												...d,
												latestVersion,
												hasUpdate: latestVersion
													? compareVersions(d.currentVersion, latestVersion)
													: false,
												loading: false,
											}
										: d,
								),
							}
						: p,
				),
			);
		});

		await Promise.all(promises);
	}, []);

	const fetchVersionsForAllProjects = useCallback(
		async (projectsData: ProjectInfo[]) => {
			for (const project of projectsData) {
				await fetchVersionsForProject(project);
			}
		},
		[fetchVersionsForProject],
	);

	const loadProjects = useCallback(async () => {
		try {
			const files = await readdir(".", { recursive: true });
			const pyprojectFiles = files.filter((file: string) =>
				file.endsWith("pyproject.toml"),
			);

			const projectsData: ProjectInfo[] = [];

			for (const file of pyprojectFiles) {
				try {
					const toml = await import(`./${file}`, { with: { type: "toml" } });

					if (toml.default?.project?.dependencies) {
						const deps: Dependency[] = toml.default.project.dependencies.map(
							(dep: string) => {
								// Parse package name, removing extras in square brackets
								const match = dep.match(
									/^([^>=<~!\[]+)(\[.*?\])?([>=<~!].+)?$/,
								);
								const name = match?.[1]?.trim() || dep;
								const version = match?.[3]?.replace(/[>=<~!]/, "") || "latest";

								return {
									name,
									currentVersion: version,
									latestVersion: undefined,
									selected: false,
									hasUpdate: false,
									loading: true,
								};
							},
						);

						projectsData.push({
							name: toml.default.project?.name || file,
							dependencies: deps,
							filePath: file,
						});
					}
				} catch (err) {
					// Skip files that can't be parsed
				}
			}

			setProjects(projectsData);
			setLoading(false);

			// Fetch version information for all dependencies
			fetchVersionsForAllProjects(projectsData);
		} catch (error) {
			setLoading(false);
		}
	}, [fetchVersionsForAllProjects]);

	// Load projects on startup
	useEffect(() => {
		loadProjects();
	}, [loadProjects]);

	useInput((input, key) => {
		if (key.escape || input === "q") {
			exit();
			return;
		}

		if (mode === "project") {
			if (key.upArrow) {
				setSelectedProjectIndex((prev) => Math.max(0, prev - 1));
			} else if (key.downArrow) {
				setSelectedProjectIndex((prev) =>
					Math.min(projects.length - 1, prev + 1),
				);
			} else if (key.return) {
				if (projects[selectedProjectIndex]) {
					setMode("dependencies");
					setSelectedDepIndex(0);
				}
			}
		} else if (mode === "dependencies") {
			const currentProject = projects[selectedProjectIndex];
			if (!currentProject) return;

			if (key.upArrow) {
				setSelectedDepIndex((prev) => Math.max(0, prev - 1));
			} else if (key.downArrow) {
				setSelectedDepIndex((prev) =>
					Math.min(currentProject.dependencies.length - 1, prev + 1),
				);
			} else if (input === " ") {
				// Toggle selection
				const newProjects = [...projects];
				const targetProject = newProjects[selectedProjectIndex];
				const targetDep = targetProject?.dependencies[selectedDepIndex];
				if (targetProject && targetDep) {
					targetDep.selected = !targetDep.selected;
					setProjects(newProjects);
				}
			} else if (key.return) {
				setMode("confirm");
			} else if (key.leftArrow) {
				setMode("project");
			}
		} else if (mode === "confirm") {
			if (input === "y") {
				updateDependencies();
			} else if (input === "n" || key.leftArrow) {
				setMode("dependencies");
			}
		}
	});

	const updateDependencies = async () => {
		const currentProject = projects[selectedProjectIndex];
		if (!currentProject) return;

		const selectedDeps = currentProject.dependencies.filter(
			(dep) => dep.selected,
		);

		if (selectedDeps.length === 0) {
			setMode("dependencies");
			return;
		}

		try {
			// Read the current TOML file
			const content = await Bun.file(currentProject.filePath).text();
			let updatedContent = content;

			// Update each selected dependency to their latest version
			for (const dep of selectedDeps) {
				if (dep.latestVersion && dep.hasUpdate) {
					// Find and replace the dependency line with the latest version
					const depRegex = new RegExp(`"${dep.name}[^"]*"`, "g");
					updatedContent = updatedContent.replace(
						depRegex,
						`"${dep.name}>=${dep.latestVersion}"`,
					);
				}
			}

			// Write the updated content back
			await Bun.write(currentProject.filePath, updatedContent);

			// Show success and exit
			console.log(
				`✅ Updated ${selectedDeps.length} dependencies in ${currentProject.name}`,
			);
			selectedDeps.forEach((dep) => {
				if (dep.latestVersion) {
					console.log(
						`  • ${dep.name}: ${dep.currentVersion} → ${dep.latestVersion}`,
					);
				}
			});
			exit();
		} catch (error) {
			console.error("❌ Failed to update dependencies:", error);
			exit();
		}
	};

	if (loading) {
		return (
			<Box flexDirection="column" padding={1}>
				<Text color="blue">🔍 Scanning for Python projects...</Text>
			</Box>
		);
	}

	if (projects.length === 0) {
		return (
			<Box flexDirection="column" padding={1}>
				<Text color="red">
					❌ No pyproject.toml files found in current directory
				</Text>
				<Text color="gray">
					Make sure you're in a directory containing Python projects
				</Text>
			</Box>
		);
	}

	if (mode === "project") {
		return (
			<Box flexDirection="column" padding={1}>
				<Text color="green" bold>
					🐍 UV-UP - Python Dependency Updater
				</Text>
				<Text color="gray">Select a project to update:</Text>
				<Text></Text>

				{projects.map((project, index) => {
					const updatesAvailable = project.dependencies.filter(
						(dep) => dep.hasUpdate,
					).length;
					const stillLoading = project.dependencies.some((dep) => dep.loading);

					return (
						<Box key={project.filePath} marginLeft={2}>
							<Text color={index === selectedProjectIndex ? "blue" : "white"}>
								{index === selectedProjectIndex ? "▶ " : "  "}
								{project.name} ({project.dependencies.length} dependencies
								{stillLoading ? (
									<Text color="gray"> - checking...</Text>
								) : updatesAvailable > 0 ? (
									<Text color="green">
										{" "}
										- {updatesAvailable} updates available
									</Text>
								) : (
									<Text color="gray"> - up to date</Text>
								)}
								)
							</Text>
						</Box>
					);
				})}

				<Text></Text>
				<Text color="gray">Use ↑↓ to navigate, Enter to select, q to quit</Text>
			</Box>
		);
	}

	if (mode === "dependencies") {
		const currentProject = projects[selectedProjectIndex];
		if (!currentProject) return null;

		return (
			<Box flexDirection="column" padding={1}>
				<Text color="green" bold>
					📦 {currentProject.name}
				</Text>
				<Text color="gray">Select dependencies to update:</Text>
				<Text></Text>

				{currentProject.dependencies.map((dep, index) => (
					<Box key={`${dep.name}-${index}`} marginLeft={2}>
						<Text color={index === selectedDepIndex ? "blue" : "white"}>
							{index === selectedDepIndex ? "▶ " : "  "}
							{dep.selected ? "✓ " : "☐ "}
							{dep.name}
							{dep.loading ? (
								<Text color="gray"> (checking...)</Text>
							) : dep.latestVersion ? (
								<Text>
									<Text color="gray"> ({dep.currentVersion}</Text>
									{dep.hasUpdate ? (
										(() => {
											const changeType = getVersionChangeType(
												dep.currentVersion,
												dep.latestVersion,
											);
											const color = changeType === "major" ? "red" : "green";
											return <Text color={color}> → {dep.latestVersion}</Text>;
										})()
									) : (
										<Text color="gray"> ✓)</Text>
									)}
								</Text>
							) : (
								<Text color="red"> (not found)</Text>
							)}
						</Text>
					</Box>
				))}

				<Text></Text>
				<Text color="gray">
					Use ↑↓ to navigate, Space to select, Enter to continue, ← to go back
				</Text>
			</Box>
		);
	}

	if (mode === "confirm") {
		const currentProject = projects[selectedProjectIndex];
		if (!currentProject) return null;

		const selectedDeps = currentProject.dependencies.filter(
			(dep) => dep.selected,
		);

		return (
			<Box flexDirection="column" padding={1}>
				<Text color="yellow" bold>
					⚠️ Confirm Updates
				</Text>
				<Text>
					About to update {selectedDeps.length} dependencies in{" "}
					{currentProject.name}:
				</Text>
				<Text></Text>

				{selectedDeps.map((dep, index) => {
					const changeType = dep.latestVersion
						? getVersionChangeType(dep.currentVersion, dep.latestVersion)
						: "minor";
					const color = changeType === "major" ? "red" : "cyan";

					return (
						<Box key={`${dep.name}-confirm-${index}`} marginLeft={2}>
							<Text color={color}>
								• {dep.name}: {dep.currentVersion} →{" "}
								{dep.latestVersion || "unknown"}
							</Text>
						</Box>
					);
				})}

				<Text></Text>
				<Text color="gray">Continue? (y/n)</Text>
			</Box>
		);
	}

	return null;
};

render(<App />);
