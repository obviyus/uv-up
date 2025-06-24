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

// Helper function to pad strings for table alignment
const padString = (
	str: string,
	length: number,
	align: "left" | "right" = "left",
): string => {
	if (align === "right") {
		return str.padStart(length);
	}
	return str.padEnd(length);
};

// Helper function to truncate long package names
const truncatePackageName = (name: string, maxLength: number): string => {
	if (name.length <= maxLength) return name;
	return `${name.substring(0, maxLength - 3)}...`;
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
				<Text color="cyan" bold>
					🔍 Scanning for Python projects...
				</Text>
			</Box>
		);
	}

	if (projects.length === 0) {
		return (
			<Box flexDirection="column" padding={1}>
				<Text color="red" bold>
					❌ No pyproject.toml files found
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
				<Box marginBottom={1}>
					<Text color="magenta" bold>
						🐍 UV-UP
					</Text>
					<Text color="gray"> - Python Dependency Updater</Text>
				</Box>

				<Box marginBottom={1}>
					<Text color="white" bold>
						Select a project:
					</Text>
				</Box>

				{projects.map((project, index) => {
					const updatesAvailable = project.dependencies.filter(
						(dep) => dep.hasUpdate,
					).length;
					const stillLoading = project.dependencies.some((dep) => dep.loading);
					const isSelected = index === selectedProjectIndex;

					return (
						<Box key={project.filePath} marginLeft={1} marginBottom={0}>
							<Text color={isSelected ? "cyan" : "white"} bold={isSelected}>
								{isSelected ? "▶ " : "  "}
								{project.name}
							</Text>
							<Text color="gray"> ({project.dependencies.length} deps</Text>
							{stillLoading ? (
								<Text color="yellow"> • checking...</Text>
							) : updatesAvailable > 0 ? (
								<Text color="green" bold>
									{" "}
									• {updatesAvailable} updates available
								</Text>
							) : (
								<Text color="gray"> • up to date</Text>
							)}
							<Text color="gray">)</Text>
						</Box>
					);
				})}

				<Box
					marginTop={1}
					paddingTop={1}
					borderStyle="single"
					borderColor="gray"
				>
					<Text color="gray">↑↓ navigate • Enter select • q quit</Text>
				</Box>
			</Box>
		);
	}

	if (mode === "dependencies") {
		const currentProject = projects[selectedProjectIndex];
		if (!currentProject) return null;

		// Calculate column widths based on content
		const maxNameLength = Math.min(
			Math.max(...currentProject.dependencies.map((d) => d.name.length), 8),
			25, // Cap at 25 chars
		);
		const maxCurrentLength = Math.max(
			...currentProject.dependencies.map((d) => d.currentVersion.length),
			7, // "Current" header length
		);
		const maxLatestLength = Math.max(
			...currentProject.dependencies.map((d) => d.latestVersion?.length || 0),
			6, // "Latest" header length
		);

		const selectedCount = currentProject.dependencies.filter(
			(d) => d.selected,
		).length;

		return (
			<Box flexDirection="column" padding={1}>
				<Box marginBottom={1}>
					<Text color="cyan" bold>
						📦 {currentProject.name}
					</Text>
					{selectedCount > 0 && (
						<Text color="green"> ({selectedCount} selected)</Text>
					)}
				</Box>

				{/* Table Header */}
				<Box marginBottom={1}>
					<Text color="white" bold>
						{padString("", 4)}{" "}
						{/* Space for selection indicator and checkbox */}
						{padString("Package", maxNameLength)} {/* Separator */}
						{padString("Current", maxCurrentLength)} {/* Separator */}
						{padString("Latest", maxLatestLength)} {/* Separator */}
						Status
					</Text>
					<Text color="gray">
						{padString(
							"",
							4 +
								maxNameLength +
								1 +
								maxCurrentLength +
								1 +
								maxLatestLength +
								1,
						)}
						{"─".repeat(20)}
					</Text>
				</Box>

				{/* Table Rows */}
				{currentProject.dependencies.map((dep, index) => {
					const isSelected = index === selectedDepIndex;
					const truncatedName = truncatePackageName(dep.name, maxNameLength);

					let statusText = "";
					let statusColor = "gray";

					if (dep.loading) {
						statusText = "Checking...";
						statusColor = "yellow";
					} else if (!dep.latestVersion) {
						statusText = "Not found";
						statusColor = "red";
					} else if (dep.hasUpdate) {
						const changeType = getVersionChangeType(
							dep.currentVersion,
							dep.latestVersion,
						);
						statusText = changeType.toUpperCase();
						statusColor =
							changeType === "major"
								? "red"
								: changeType === "minor"
									? "yellow"
									: "green";
					} else {
						statusText = "Up to date";
						statusColor = "green";
					}

					return (
						<Box key={`${dep.name}-${index}`} marginBottom={0}>
							<Text
								color={isSelected ? "cyan" : "white"}
								backgroundColor={isSelected ? "blue" : undefined}
							>
								{isSelected ? "▶ " : "  "}
								{dep.selected ? "✓ " : "☐ "}
								{padString(truncatedName, maxNameLength)}{" "}
								<Text color="gray">
									{padString(dep.currentVersion, maxCurrentLength)}
								</Text>{" "}
								<Text color={dep.hasUpdate ? "green" : "gray"}>
									{padString(dep.latestVersion || "─", maxLatestLength)}
								</Text>{" "}
								<Text color={statusColor} bold={dep.hasUpdate}>
									{statusText}
								</Text>
							</Text>
						</Box>
					);
				})}

				<Box
					marginTop={1}
					paddingTop={1}
					borderStyle="single"
					borderColor="gray"
				>
					<Text color="gray">
						↑↓ navigate • Space select • Enter continue • ← back • q quit
					</Text>
				</Box>
			</Box>
		);
	}

	if (mode === "confirm") {
		const currentProject = projects[selectedProjectIndex];
		if (!currentProject) return null;

		const selectedDeps = currentProject.dependencies.filter(
			(dep) => dep.selected,
		);

		// Calculate column widths based on selected dependencies
		const maxNameLength = Math.min(
			Math.max(...selectedDeps.map((d) => d.name.length), 8),
			25, // Cap at 25 chars
		);
		const maxCurrentLength = Math.max(
			...selectedDeps.map((d) => d.currentVersion.length),
			7, // "Current" header length
		);
		const maxLatestLength = Math.max(
			...selectedDeps.map((d) => d.latestVersion?.length || 0),
			6, // "Latest" header length
		);

		return (
			<Box flexDirection="column" padding={1}>
				<Box marginBottom={1}>
					<Text color="yellow" bold>
						⚠️ Confirm Updates
					</Text>
				</Box>

				<Box marginBottom={1}>
					<Text>
						About to update {selectedDeps.length} dependencies in{" "}
						{currentProject.name}:
					</Text>
				</Box>

				{/* Table Header */}
				<Box marginBottom={1}>
					<Text color="white" bold>
						{padString("Package", maxNameLength)} {/* Separator */}
						{padString("Current", maxCurrentLength)} {/* Separator */}
						{padString("Latest", maxLatestLength)} {/* Separator */}
						Change Type
					</Text>
					<Text color="gray">
						{padString(
							"",
							maxNameLength + 1 + maxCurrentLength + 1 + maxLatestLength + 1,
						)}
						{"─".repeat(15)}
					</Text>
				</Box>

				{/* Table Rows */}
				{selectedDeps.map((dep, index) => {
					const changeType = dep.latestVersion
						? getVersionChangeType(dep.currentVersion, dep.latestVersion)
						: "minor";
					const badgeColor =
						changeType === "major"
							? "red"
							: changeType === "minor"
								? "yellow"
								: "green";
					const truncatedName = truncatePackageName(dep.name, maxNameLength);

					return (
						<Box key={`${dep.name}-confirm-${index}`} marginBottom={0}>
							<Text color="white">
								{padString(truncatedName, maxNameLength)}{" "}
								{padString(dep.currentVersion, maxCurrentLength)}{" "}
								<Text color="green" bold>
									{padString(dep.latestVersion || "unknown", maxLatestLength)}
								</Text>{" "}
								<Text color={badgeColor} bold>
									{changeType.toUpperCase()}
								</Text>
							</Text>
						</Box>
					);
				})}

				<Box marginBottom={1}></Box>

				<Box borderStyle="single" borderColor="yellow" padding={1}>
					<Text color="yellow" bold>
						Continue with updates? (y/n)
					</Text>
				</Box>
			</Box>
		);
	}

	return null;
};

render(<App />);
