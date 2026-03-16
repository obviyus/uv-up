use std::collections::BTreeSet;
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::{TerminalOptions, Viewport};

use crate::model::{
    AppMode, DependencyEntry, DependencySection, DependencyStatus, Project, UpdateSummary,
};
use crate::pypi::PypiClient;
use crate::pyproject;

pub fn run(projects: Vec<Project>) -> Result<Option<UpdateSummary>> {
    if projects.is_empty() {
        return Ok(None);
    }

    let mut app = App::new(projects)?;
    let mut terminal = ratatui::try_init_with_options(TerminalOptions {
        viewport: Viewport::Inline(20),
    })
    .context("failed to initialize inline terminal viewport; terminal must support cursor position queries")?;
    let result = app.run(&mut terminal);
    ratatui::restore();
    result
}

struct App {
    mode: AppMode,
    projects: Vec<Project>,
    selected_project: usize,
    selected_dependency: usize,
    selected: Vec<BTreeSet<usize>>,
    status_line: Option<String>,
    pypi: PypiClient,
}

impl App {
    fn new(mut projects: Vec<Project>) -> Result<Self> {
        let project_count = projects.len();
        let mut pypi = PypiClient::new()?;
        for project in &mut projects {
            pypi.hydrate_project(&mut project.dependencies);
        }
        let mode = if projects.len() == 1 {
            AppMode::Dependencies
        } else {
            AppMode::Project
        };

        Ok(Self {
            mode,
            projects,
            selected_project: 0,
            selected_dependency: 0,
            selected: vec![BTreeSet::new(); project_count],
            status_line: None,
            pypi,
        })
    }

    fn run(&mut self, terminal: &mut ratatui::DefaultTerminal) -> Result<Option<UpdateSummary>> {
        loop {
            terminal.draw(|frame| self.render(frame))?;
            if !event::poll(Duration::from_millis(100))? {
                continue;
            }

            let Event::Key(key) = event::read()? else {
                continue;
            };
            if key.kind != KeyEventKind::Press {
                continue;
            }

            match key.code {
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    return Ok(None);
                }
                KeyCode::Esc | KeyCode::Char('q') => return Ok(None),
                KeyCode::Up | KeyCode::Char('k') => self.move_cursor(-1),
                KeyCode::Down | KeyCode::Char('j') => self.move_cursor(1),
                KeyCode::Enter => {
                    if let Some(summary) = self.handle_enter()? {
                        return Ok(Some(summary));
                    }
                }
                KeyCode::Left => self.handle_back(),
                KeyCode::Char(' ') if self.mode == AppMode::Dependencies => self.toggle_current(),
                KeyCode::Char('a') if self.mode == AppMode::Dependencies => {
                    self.select_all_outdated()
                }
                KeyCode::Char('n') if self.mode == AppMode::Dependencies => self.clear_selection(),
                KeyCode::Char('u') if self.mode == AppMode::Dependencies => {
                    self.toggle_only_outdated()
                }
                KeyCode::Char('r') => self.refresh_current()?,
                KeyCode::Char('y') if self.mode == AppMode::Confirm => {
                    return self.confirm_updates().map(Some);
                }
                KeyCode::Char('n') if self.mode == AppMode::Confirm => {
                    self.mode = AppMode::Dependencies;
                }
                _ => {}
            }
        }
    }

    fn render(&self, frame: &mut ratatui::Frame) {
        let areas = Layout::vertical([
            Constraint::Length(2),
            Constraint::Min(1),
            Constraint::Length(2),
        ])
        .split(frame.area());

        let prompt = match self.mode {
            AppMode::Project => Line::from(vec![
                Span::styled(
                    "? ",
                    Style::default()
                        .fg(Color::Blue)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "Select a project",
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "  Enter open, j/k move, r refresh, q quit",
                    Style::default().fg(Color::DarkGray),
                ),
            ]),
            AppMode::Dependencies => Line::from(vec![
                Span::styled(
                    "? ",
                    Style::default()
                        .fg(Color::Blue)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "Select dependencies to update",
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "  Space toggle, Enter confirm, a all, n none, u outdated, r refresh, q quit",
                    Style::default().fg(Color::DarkGray),
                ),
            ]),
            AppMode::Confirm => Line::from(vec![
                Span::styled(
                    "? ",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "Confirm updates",
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "  y apply, n back, q quit",
                    Style::default().fg(Color::DarkGray),
                ),
            ]),
        };
        frame.render_widget(Paragraph::new(prompt), areas[0]);

        match self.mode {
            AppMode::Project => self.render_projects(frame, areas[1]),
            AppMode::Dependencies => self.render_dependencies(frame, areas[1]),
            AppMode::Confirm => self.render_confirm(frame, areas[1]),
        }

        let status = self.status_line.clone().unwrap_or_default();
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("status ", Style::default().fg(Color::DarkGray)),
                Span::raw(status),
            ]))
            .wrap(Wrap { trim: true }),
            areas[2],
        );
    }

    fn render_projects(&self, frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        let mut lines = Vec::new();

        for (index, project) in self.projects.iter().enumerate() {
            let prefix = if index == self.selected_project {
                "❯ "
            } else {
                "  "
            };
            let summary = if project.updates_available() > 0 {
                format!(
                    "{} deps, {} updates",
                    project.dependencies.len(),
                    project.updates_available()
                )
            } else {
                format!("{} deps, up to date", project.dependencies.len())
            };

            lines.push(Line::from(vec![
                Span::styled(
                    format!("{prefix}{}", project.name),
                    Style::default()
                        .fg(if index == self.selected_project {
                            Color::Cyan
                        } else {
                            Color::White
                        })
                        .add_modifier(if index == self.selected_project {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                ),
                Span::styled(
                    format!("  ({summary})"),
                    Style::default().fg(Color::DarkGray),
                ),
            ]));
        }

        frame.render_widget(Paragraph::new(Text::from(lines)), area);
    }

    fn render_dependencies(&self, frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        let Some(project) = self.current_project() else {
            return;
        };

        let selected = self.selected_for_current();
        let name_width = project
            .dependencies
            .iter()
            .map(|dep| dep.display_name.len())
            .max()
            .unwrap_or(7)
            .clamp(7, 30);
        let current_width = project
            .dependencies
            .iter()
            .map(|dep| dep.current_version_text().len())
            .max()
            .unwrap_or(7)
            .max(7);
        let latest_width = project
            .dependencies
            .iter()
            .map(|dep| dep.latest_version_text.as_deref().unwrap_or("—").len())
            .max()
            .unwrap_or(6)
            .max(6);
        let target_width = latest_width.max(6);

        let mut lines = Vec::new();
        lines.push(Line::from(vec![
            Span::styled(project.name.clone(), Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("  {} selected", project.selected_count(selected)),
                Style::default().fg(Color::DarkGray),
            ),
        ]));
        lines.push(Line::raw(""));

        let mut previous_section: Option<&DependencySection> = None;
        for (index, dependency) in project.dependencies.iter().enumerate() {
            if previous_section != Some(&dependency.section) {
                if previous_section.is_some() {
                    lines.push(Line::raw(""));
                }
                lines.push(Line::styled(
                    section_title(&dependency.section),
                    Style::default().add_modifier(Modifier::BOLD),
                ));
                lines.push(Line::styled(
                    format!(
                        "  {:name_width$} {:current_width$} {:target_width$} {:latest_width$}",
                        "dependencies",
                        "Current",
                        "Target",
                        "Latest",
                        name_width = name_width + 4,
                        current_width = current_width,
                        target_width = target_width,
                        latest_width = latest_width
                    ),
                    Style::default().fg(Color::DarkGray),
                ));
                previous_section = Some(&dependency.section);
            }
            let is_cursor = index == self.selected_dependency;
            let is_selected = selected.contains(&index);
            lines.push(render_dependency_line(
                dependency,
                is_cursor,
                is_selected,
                name_width,
                current_width,
                target_width,
                latest_width,
            ));
        }

        frame.render_widget(
            Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false }),
            area,
        );
    }

    fn render_confirm(&self, frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        let Some(project) = self.current_project() else {
            return;
        };
        let selected = self.selected_for_current();
        let chosen: Vec<&DependencyEntry> = selected
            .iter()
            .filter_map(|index| project.dependencies.get(*index))
            .filter(|dependency| dependency.selectable())
            .collect();

        let mut lines = Vec::new();
        lines.push(Line::styled(
            format!("Apply {} updates in {}?", chosen.len(), project.name),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));
        lines.push(Line::raw(""));
        for dependency in chosen {
            lines.push(Line::raw(format!(
                "{}  {} -> {}  {}",
                dependency.display_name,
                dependency.current_version_text(),
                dependency
                    .latest_version_text
                    .as_deref()
                    .unwrap_or("unknown"),
                dependency.section.label()
            )));
        }

        frame.render_widget(
            Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false }),
            area,
        );
    }

    fn move_cursor(&mut self, delta: isize) {
        match self.mode {
            AppMode::Project => {
                self.selected_project =
                    move_index(self.selected_project, self.projects.len(), delta);
            }
            AppMode::Dependencies => {
                let count = self
                    .current_project()
                    .map(|project| project.dependencies.len())
                    .unwrap_or(0);
                self.selected_dependency = move_index(self.selected_dependency, count, delta);
            }
            AppMode::Confirm => {}
        }
    }

    fn handle_enter(&mut self) -> Result<Option<UpdateSummary>> {
        match self.mode {
            AppMode::Project => {
                self.mode = AppMode::Dependencies;
                self.selected_dependency = 0;
                Ok(None)
            }
            AppMode::Dependencies => {
                self.mode = AppMode::Confirm;
                Ok(None)
            }
            AppMode::Confirm => self.confirm_updates().map(Some),
        }
    }

    fn handle_back(&mut self) {
        self.mode = match self.mode {
            AppMode::Project => AppMode::Project,
            AppMode::Dependencies => AppMode::Project,
            AppMode::Confirm => AppMode::Dependencies,
        };
    }

    fn toggle_current(&mut self) {
        let project_index = self.selected_project;
        let Some(project) = self.projects.get(project_index) else {
            return;
        };
        let Some(dependency) = project.dependencies.get(self.selected_dependency) else {
            return;
        };
        if !dependency.selectable() {
            self.status_line = dependency
                .status
                .detail()
                .map(ToString::to_string)
                .or_else(|| {
                    Some("Only outdated supported dependencies can be selected".to_string())
                });
            return;
        }

        let selected = &mut self.selected[project_index];
        if !selected.insert(self.selected_dependency) {
            selected.remove(&self.selected_dependency);
        }
    }

    fn select_all_outdated(&mut self) {
        let project_index = self.selected_project;
        let Some(project) = self.projects.get(project_index) else {
            return;
        };
        let outdated: BTreeSet<usize> = project
            .dependencies
            .iter()
            .enumerate()
            .filter_map(|(index, dependency)| dependency.selectable().then_some(index))
            .collect();
        self.selected[project_index] = outdated;
    }

    fn clear_selection(&mut self) {
        let project_index = self.selected_project;
        if project_index >= self.selected.len() {
            return;
        }
        self.selected[project_index].clear();
    }

    fn toggle_only_outdated(&mut self) {
        let project_index = self.selected_project;
        let Some(project) = self.projects.get(project_index) else {
            return;
        };
        let all_outdated: BTreeSet<usize> = project
            .dependencies
            .iter()
            .enumerate()
            .filter_map(|(index, dependency)| dependency.selectable().then_some(index))
            .collect();

        let selected = &mut self.selected[project_index];
        if *selected == all_outdated {
            selected.clear();
        } else {
            *selected = all_outdated;
        }
    }

    fn refresh_current(&mut self) -> Result<()> {
        let project_index = self.selected_project;
        let Some(project_name) = self
            .projects
            .get(project_index)
            .map(|project| project.name.clone())
        else {
            return Ok(());
        };
        let package_names = self
            .projects
            .get(project_index)
            .into_iter()
            .flat_map(|project| project.dependencies.iter().map(|dep| dep.name.clone()))
            .collect::<Vec<_>>();
        self.pypi.clear_project(package_names);
        if project_index < self.projects.len() {
            let pypi = &mut self.pypi;
            let project = &mut self.projects[project_index];
            pypi.hydrate_project(&mut project.dependencies);
        }
        self.status_line = Some(format!("Refreshed {project_name}"));
        Ok(())
    }

    fn confirm_updates(&mut self) -> Result<UpdateSummary> {
        let project = self
            .current_project()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("no project selected"))?;
        let selected = self.selected_for_current().clone();
        let summary = pyproject::update_project(&project, &selected)?;
        self.status_line = Some(format!(
            "Updated {} dependencies in {}",
            summary.updated.len(),
            summary.project_name
        ));
        Ok(summary)
    }

    fn current_project(&self) -> Option<&Project> {
        self.projects.get(self.selected_project)
    }

    fn selected_for_current(&self) -> &BTreeSet<usize> {
        &self.selected[self.selected_project]
    }
}

fn move_index(current: usize, len: usize, delta: isize) -> usize {
    if len == 0 {
        return 0;
    }

    if delta.is_negative() {
        current.saturating_sub(delta.unsigned_abs())
    } else {
        (current + delta as usize).min(len.saturating_sub(1))
    }
}

fn render_dependency_line(
    dependency: &DependencyEntry,
    is_cursor: bool,
    is_selected: bool,
    name_width: usize,
    current_width: usize,
    target_width: usize,
    latest_width: usize,
) -> Line<'static> {
    let cursor = if is_cursor { "❯" } else { " " };
    let checkbox = if is_selected { "◼" } else { "◻" };
    let latest = dependency.latest_version_text.as_deref().unwrap_or("—");
    let target = dependency.latest_version_text.as_deref().unwrap_or("—");
    let current = dependency.current_version_text();
    let name = format!("{checkbox} {}", dependency.display_name);

    let default_style = Style::default().bg(if is_cursor {
        Color::DarkGray
    } else {
        Color::Reset
    });
    let latest_color = match &dependency.status {
        DependencyStatus::UpdateAvailable(_) => Color::Green,
        DependencyStatus::NotFound => Color::Red,
        DependencyStatus::Unsupported(_) => Color::Yellow,
        DependencyStatus::UpToDate => Color::DarkGray,
    };

    Line::from(vec![
        Span::styled(format!("{cursor} "), default_style),
        Span::styled(
            format!("{:name_width$}", name, name_width = name_width + 2),
            default_style,
        ),
        Span::styled(
            format!(" {:current_width$}", current, current_width = current_width),
            default_style,
        ),
        Span::styled(
            format!(" {:target_width$}", target, target_width = target_width),
            default_style.fg(if dependency.selectable() {
                Color::Green
            } else {
                Color::DarkGray
            }),
        ),
        Span::styled(
            format!(" {:latest_width$}", latest, latest_width = latest_width),
            default_style.fg(latest_color),
        ),
        Span::styled(
            format!("  {}", dependency.status.label()),
            default_style.fg(match &dependency.status {
                DependencyStatus::UpdateAvailable(_) => Color::Green,
                DependencyStatus::NotFound => Color::Red,
                DependencyStatus::Unsupported(_) => Color::Yellow,
                DependencyStatus::UpToDate => Color::DarkGray,
            }),
        ),
    ])
}

fn section_title(section: &DependencySection) -> String {
    match section {
        DependencySection::Project => "dependencies".to_string(),
        DependencySection::Optional(name) => format!("extra:{name}"),
        DependencySection::Group(name) => format!("group:{name}"),
    }
}
