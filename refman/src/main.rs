use std::fs;
use std::io;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem},
    Terminal,
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize)]
struct Reference {
    title: String,
    authors: Vec<String>,
    year: u32,
    doi: String,
}

#[derive(Default)]
struct App {
    projects: Vec<PathBuf>,
    selected: usize,
    references: Vec<Reference>,
}

impl App {
    fn new() -> Self {
        let projects = fs::read_dir("projects")
            .unwrap_or_else(|_| fs::read_dir(".").unwrap())
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().map(|e| e == "json").unwrap_or(false))
            .collect();
        Self {
            projects,
            selected: 0,
            references: vec![],
        }
    }

    fn load_references(&mut self) {
        if let Some(project_path) = self.projects.get(self.selected) {
            if let Ok(data) = fs::read_to_string(project_path) {
                if let Ok(json) = serde_json::from_str::<Vec<Value>>(&data) {
                    self.references = json
                        .iter()
                        .filter_map(|v| serde_json::from_value::<Reference>(v.clone()).ok())
                        .collect();
                }
            }
        }
    }

    fn next(&mut self) {
        if !self.projects.is_empty() {
            self.selected = (self.selected + 1) % self.projects.len();
            self.load_references();
        }
    }

    fn previous(&mut self) {
        if !self.projects.is_empty() {
            if self.selected == 0 {
                self.selected = self.projects.len() - 1;
            } else {
                self.selected -= 1;
            }
            self.load_references();
        }
    }
}

fn main() -> Result<()> {
    let mut stdout = io::stdout();
    crossterm::terminal::enable_raw_mode()?;
    let backend = CrosstermBackend::new(&mut stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let mut app = App::new();
    app.load_references();

    loop {
        terminal.draw(|f| {
            let size = f.size();
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(30), Constraint::Percentage(70)].as_ref())
                .split(size);

            // Left panel: projects
            let project_items: Vec<ListItem> = app
                .projects
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    let style = if i == app.selected {
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    };
                    ListItem::new(p.file_name().unwrap().to_string_lossy().to_string()).style(style)
                })
                .collect();

            let projects = List::new(project_items)
                .block(Block::default().title("Projects").borders(Borders::ALL));
            f.render_widget(projects, chunks[0]);

            // Right panel: references
            let ref_items: Vec<ListItem> = app
                .references
                .iter()
                .map(|r| {
                    let line = format!("{} ({})", r.title, r.year);
                    ListItem::new(line)
                })
                .collect();

            let refs = List::new(ref_items)
                .block(Block::default().title("References").borders(Borders::ALL));
            f.render_widget(refs, chunks[1]);
        })?;

        if event::poll(Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Down => app.next(),
                    KeyCode::Up => app.previous(),
                    _ => {}
                }
            }
        }
    }

    crossterm::terminal::disable_raw_mode()?;
    Ok(())
}
