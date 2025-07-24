import { Box, render, Text, useApp, useInput } from "ink";
import type React from "react";
import { useCallback, useEffect, useState } from "react";

// Types
type VersionChangeType = "major" | "minor" | "patch" | "none";
type AppMode = "project" | "dependencies" | "confirm";
type TextAlign = "left" | "right";

// Interfaces
interface Dependency {
	name: string;
	extras: string;
	currentVersion: string;
	originalConstraint: string;
	constraintOperator: string;
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

interface PyPIResponse {
	info?: {
		version?: string;
	};
}

interface PyProjectToml {
	project?: {
		name?: string;
		dependencies?: string[];
	};
}

// Constants
const MAX_PACKAGE_NAME_LENGTH = 25;
const PYPI_API_BASE_URL = "https://pypi.org/pypi";
const DEFAULT_CONSTRAINT_OPERATOR = "~=";
const DEPENDENCY_REGEX = /^([^>=<~![]+)(\[.*?\])?([>=<~!].+)?$/;
const OPERATOR_REGEX = /^([>=<~!]+)(.+)$/;

const fetchLatestVersion = async (
	packageName: string,
): Promise<string | null> => {
	if (!packageName?.trim()) return null;

	try {
		const response = await fetch(`${PYPI_API_BASE_URL}/${packageName}/json`);
		if (!response.ok) return null;

		const data = (await response.json()) as PyPIResponse;
		return data.info?.version || null;
	} catch {
		return null;
	}
};

const compareVersions = (current: string, latest: string): boolean => {
	if (!current || !latest) return false;

	const cleanCurrent = current.replace(/[>=<~!]/g, "").trim();
	try {
		return Bun.semver.order(cleanCurrent, latest) === -1;
	} catch {
		return cleanCurrent !== latest;
	}
};

const getVersionChangeType = (
	current: string,
	latest: string,
): VersionChangeType => {
	const cleanCurrent = current.replace(/[>=<~!]/g, "").trim();
	try {
		const [currentMajor = 0, currentMinor = 0] = cleanCurrent
			.split(".")
			.map((v) => parseInt(v) || 0);
		const [latestMajor = 0, latestMinor = 0] = latest
			.split(".")
			.map((v) => parseInt(v) || 0);

		if (latestMajor > currentMajor) return "major";
		if (latestMinor > currentMinor) return "minor";
		if (Bun.semver.order(cleanCurrent, latest) === -1) return "patch";
		return "none";
	} catch {
		return "minor";
	}
};

const padString = (
	str: string,
	length: number,
	align: TextAlign = "left",
): string => {
	return align === "right" ? str.padStart(length) : str.padEnd(length);
};

const truncatePackageName = (
	name: string,
	maxLength = MAX_PACKAGE_NAME_LENGTH,
): string => {
	if (!name || name.length <= maxLength) return name;
	return `${name.substring(0, maxLength - 3)}...`;
};

const getStatusDisplay = (dep: Dependency) => {
	if (dep.loading) return { text: "Checking...", color: "yellow" };
	if (!dep.latestVersion) return { text: "Not found", color: "red" };
	if (!dep.hasUpdate) return { text: "Up to date", color: "green" };

	const changeType = getVersionChangeType(
		dep.currentVersion,
		dep.latestVersion,
	);
	const colorMap = {
		major: "red",
		minor: "yellow",
		patch: "green",
		none: "green",
	};
	return { text: changeType.toUpperCase(), color: colorMap[changeType] };
};

const calculateColumnWidths = (dependencies: Dependency[]) => {
	const maxNameLength = Math.min(
		Math.max(...dependencies.map((d) => d.name.length), 8),
		MAX_PACKAGE_NAME_LENGTH,
	);
	const maxCurrentLength = Math.max(
		...dependencies.map((d) => d.currentVersion.length),
		7,
	);
	const maxLatestLength = Math.max(
		...dependencies.map((d) => d.latestVersion?.length || 0),
		6,
	);
	return { maxNameLength, maxCurrentLength, maxLatestLength };
};

// Component for displaying loading state
const LoadingScreen: React.FC = () => (
	<Box flexDirection="column" padding={1}>
		<Text color="cyan" bold>
			🔍 Scanning for Python projects...
		</Text>
	</Box>
);

// Component for displaying no projects found
const NoProjectsScreen: React.FC = () => (
	<Box flexDirection="column" padding={1}>
		<Text color="red" bold>
			❌ No pyproject.toml files found
		</Text>
		<Text color="gray">
			Make sure you're in a directory containing Python projects
		</Text>
	</Box>
);

// Component for displaying keyboard shortcuts
interface KeyboardShortcutsProps {
	mode: AppMode;
}

const KeyboardShortcuts: React.FC<KeyboardShortcutsProps> = ({ mode }) => {
	const shortcuts: Record<AppMode, string> = {
		project: "↑↓ navigate • Enter select • q quit",
		dependencies:
			"↑↓ navigate • Space select • Enter continue • ← back • q quit",
		confirm: "y confirm • n cancel • q quit",
	};

	return (
		<Box marginTop={1} paddingTop={1} borderStyle="single" borderColor="gray">
			<Text color="gray">{shortcuts[mode]}</Text>
		</Box>
	);
};

const App: React.FC = () => {
	const [projects, setProjects] = useState<ProjectInfo[]>([]);
	const [selectedProjectIndex, setSelectedProjectIndex] = useState(0);
	const [selectedDepIndex, setSelectedDepIndex] = useState(0);
	const [mode, setMode] = useState<AppMode>("project");
	const [loading, setLoading] = useState(true);

	const safeSelectedProjectIndex = Math.min(
		selectedProjectIndex,
		Math.max(0, projects.length - 1),
	);
	const currentProject = projects[safeSelectedProjectIndex];
	const safeSelectedDepIndex = currentProject
		? Math.min(
				selectedDepIndex,
				Math.max(0, currentProject.dependencies.length - 1),
			)
		: 0;
	const { exit } = useApp();

	const fetchVersionsForProject = useCallback(async (project: ProjectInfo) => {
		const promises = project.dependencies.map(async (dep, index) => {
			const latestVersion = await fetchLatestVersion(dep.name);
			const hasUpdate = latestVersion
				? compareVersions(dep.currentVersion, latestVersion)
				: false;

			setProjects((prev) =>
				prev.map((p) =>
					p.filePath === project.filePath
						? {
								...p,
								dependencies: p.dependencies.map((d, i) =>
									i === index
										? { ...d, latestVersion, hasUpdate, loading: false }
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

	const parseDependency = useCallback((dep: string): Dependency => {
		const baseMatch = dep.split(";")[0] || dep;
		const match = baseMatch.match(DEPENDENCY_REGEX);
		const name = match?.[1]?.trim() || dep;
		const extras = match?.[2] || "";
		const constraintPart = match?.[3] || "";

		const operatorMatch = constraintPart.match(OPERATOR_REGEX);
		const operator = operatorMatch?.[1] || ">=";
		const version = operatorMatch?.[2]?.trim() || "0.0.0";

		return {
			name,
			extras,
			currentVersion: version,
			originalConstraint: dep,
			constraintOperator: operator,
			latestVersion: undefined,
			selected: false,
			hasUpdate: false,
			loading: true,
		};
	}, []);

	const loadProjects = useCallback(async () => {
		try {
			const files = [...new Bun.Glob("**/*.toml").scanSync(".")];
			const pyprojectFiles = files.filter((file: string) =>
				file.endsWith("pyproject.toml"),
			);

			const projectsData: ProjectInfo[] = [];

			for (const file of pyprojectFiles) {
				try {
					const tomlContent = await Bun.file(file).text();
					const toml = Bun.TOML.parse(tomlContent) as PyProjectToml;
					const dependencies = toml?.project?.dependencies;

					if (dependencies && Array.isArray(dependencies)) {
						const deps = dependencies
							.filter((dep: unknown): dep is string => typeof dep === "string")
							.map(parseDependency);

						projectsData.push({
							name: toml.project?.name || file,
							dependencies: deps,
							filePath: file,
						});
					}
				} catch (err) {
					console.warn(
						`Failed to parse ${file}:`,
						err instanceof Error ? err.message : err,
					);
				}
			}

			setProjects(projectsData);
			setLoading(false);
			fetchVersionsForAllProjects(projectsData);
		} catch (error) {
			console.error("Failed to load projects:", error);
			setLoading(false);
		}
	}, [fetchVersionsForAllProjects, parseDependency]);

	// Load projects on startup
	useEffect(() => {
		loadProjects();
	}, [loadProjects]);

	const handleNavigation = (direction: "up" | "down") => {
		if (mode === "project") {
			setSelectedProjectIndex((prev) =>
				direction === "up"
					? Math.max(0, prev - 1)
					: Math.min(projects.length - 1, prev + 1),
			);
		} else if (mode === "dependencies" && currentProject) {
			setSelectedDepIndex((prev) =>
				direction === "up"
					? Math.max(0, prev - 1)
					: Math.min(currentProject.dependencies.length - 1, prev + 1),
			);
		}
	};

	const toggleDependencySelection = () => {
		const newProjects = [...projects];
		const targetProject = newProjects[safeSelectedProjectIndex];
		const targetDep = targetProject?.dependencies[safeSelectedDepIndex];
		if (targetProject && targetDep) {
			targetDep.selected = !targetDep.selected;
			setProjects(newProjects);
		}
	};

	useInput((input, key) => {
		if (key.escape || input === "q") {
			exit();
			return;
		}

		if (key.upArrow) handleNavigation("up");
		else if (key.downArrow) handleNavigation("down");
		else if (key.return) {
			if (mode === "project" && projects[safeSelectedProjectIndex]) {
				setMode("dependencies");
				setSelectedDepIndex(0);
			} else if (mode === "dependencies") {
				setMode("confirm");
			}
		} else if (input === " " && mode === "dependencies") {
			toggleDependencySelection();
		} else if (key.leftArrow) {
			setMode(mode === "dependencies" ? "project" : "dependencies");
		} else if (mode === "confirm") {
			if (input === "y") updateDependencies();
			else if (input === "n") setMode("dependencies");
		}
	});

	const updateDependencies = useCallback(async () => {
		const selectedProject = projects[safeSelectedProjectIndex];
		if (!selectedProject) return;

		const selectedDeps = selectedProject.dependencies.filter(
			(dep) => dep.selected,
		);

		if (selectedDeps.length === 0) {
			setMode("dependencies");
			return;
		}

		try {
			// Read the current TOML file content
			const content = await Bun.file(selectedProject.filePath).text();
			let updatedContent = content;

			// Update each selected dependency to their latest version
			for (const dep of selectedDeps) {
				if (dep.latestVersion && dep.hasUpdate) {
					// Choose appropriate constraint operator based on original or use compatible release
					let newOperator = DEFAULT_CONSTRAINT_OPERATOR;

					// Preserve strict equality or use compatible release for others
					if (dep.constraintOperator === "==") {
						newOperator = "==";
					} else if (dep.constraintOperator === "~=") {
						newOperator = "~=";
					}
					// For >=, >, <, <=, !=: default to ~= for better semantic versioning

					// More precise replacement that preserves formatting
					const escapedOriginal = dep.originalConstraint.replace(
						/[.*+?^${}()|[\]\\]/g,
						"\\$&",
					);
					const originalRegex = new RegExp(`"${escapedOriginal}"`, "g");

					// Build new constraint string, preserving extras and environment markers
					const envMarkerMatch = dep.originalConstraint.match(/;(.+)$/);
					const envMarker = envMarkerMatch ? `;${envMarkerMatch[1]}` : "";
					const newConstraint = `"${dep.name}${dep.extras}${newOperator}${dep.latestVersion}${envMarker}"`;

					updatedContent = updatedContent.replace(originalRegex, newConstraint);
				}
			}

			// Write the updated content back
			await Bun.write(selectedProject.filePath, updatedContent);

			// Show success and exit
			console.log(
				`✅ Updated ${selectedDeps.length} dependencies in ${selectedProject.name}`,
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
	}, [projects, safeSelectedProjectIndex, exit]);

	if (loading) {
		return <LoadingScreen />;
	}

	if (projects.length === 0) {
		return <NoProjectsScreen />;
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

				<KeyboardShortcuts mode={mode} />
			</Box>
		);
	}

	if (mode === "dependencies") {
		if (!currentProject) return null;

		const { maxNameLength, maxCurrentLength, maxLatestLength } =
			calculateColumnWidths(currentProject.dependencies);
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
					const status = getStatusDisplay(dep);

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
								<Text color={status.color} bold={dep.hasUpdate}>
									{status.text}
								</Text>
							</Text>
						</Box>
					);
				})}

				<KeyboardShortcuts mode={mode} />
			</Box>
		);
	}

	if (mode === "confirm") {
		if (!currentProject) return null;

		const selectedDeps = currentProject.dependencies.filter(
			(dep) => dep.selected,
		);
		const { maxNameLength, maxCurrentLength, maxLatestLength } =
			selectedDeps.length === 0
				? { maxNameLength: 8, maxCurrentLength: 7, maxLatestLength: 6 }
				: calculateColumnWidths(selectedDeps);

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
					const colorMap = {
						major: "red",
						minor: "yellow",
						patch: "green",
						none: "green",
					};
					const truncatedName = truncatePackageName(dep.name, maxNameLength);

					return (
						<Box key={`${dep.name}-confirm-${index}`} marginBottom={0}>
							<Text color="white">
								{padString(truncatedName, maxNameLength)}{" "}
								{padString(dep.currentVersion, maxCurrentLength)}{" "}
								<Text color="green" bold>
									{padString(dep.latestVersion || "unknown", maxLatestLength)}
								</Text>{" "}
								<Text color={colorMap[changeType]} bold>
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
