use biblatex::{Bibliography, Entry};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect, Alignment},
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Clear, Wrap},
    Terminal,
};
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::{fs, io};
// use std::path::Path;
use anyhow::Result;

use bshelf::{
    add_reference,
    add_to_project,
    authors_to_string,
    chunks_to_string,
    date_to_year_string,
    entry_matches,
    load_config,
    publisher_string,
    Config,
    load_projects_map,
    ProjectsMap,
    save_projects_map,
    remove_from_project,
}; // <-- crate name = [package].name in Cargo.toml

// TODO: 
//  - Cannot show more references than size of terminal! need scrolling
//  - Search/filtering
//  - Creating a new project should check if project already exists.
//  - Using direct link to pdf, download the pdf and store it as {doi}.pdf
//

struct App {
    config: Config,
    projects: Vec<String>,
    selected_project: usize,
    references: Vec<Entry>,
    selected_reference: usize,
    mode: Mode,
    // search_mode: bool,
    search_query: String,
    new_project_name: String,
    new_ref: String,
    filtered_refs: Vec<Entry>,
    alert_message: Option<String>,
    alert_timer: Option<std::time::Instant>,
    list_state: ListState,
    moving_target: usize,
}

enum Mode {
    Normal,
    Search,
    NewProject,
    Adding,
    Moving,
}

impl App {
    fn new(config: Config) -> Self {
        let proj_map_path = config.projects_file.to_string_lossy().to_string();
        let mut projects: Vec<String> = load_projects_map(&proj_map_path)
            .map(|map| map.into_keys().collect())
            .unwrap_or_default();

        projects.sort();
        projects.insert(0, "all".to_string());

        App {
            config,
            projects,
            selected_project: 0,
            references: Vec::new(),
            selected_reference: 0,
            mode: Mode::Normal,
            // search_mode: false,
            search_query: String::new(),
            new_project_name: String::new(),
            new_ref: String::new(),
            filtered_refs: Vec::new(),
            alert_message: None,
            alert_timer: None,
            list_state: ListState::default(),
            moving_target: 0,
        }
    }

    fn load_references(&mut self) {
        let all_bib_path = self.config.all_bib.to_string_lossy().to_string();

        let bib = fs::read_to_string(&all_bib_path)
            .ok()
            .and_then(|content| Bibliography::parse(&content).ok())
            .unwrap_or_default();

        let selected = self.projects.get(self.selected_project).map(|s| s.as_str());

        let mut refs: Vec<Entry> = match selected {
            Some("all") => bib.iter().cloned().collect(),  // all entries
            Some(project) => {
                let proj_map_path = self.config.projects_file.to_string_lossy().to_string();
                let map: ProjectsMap = fs::read_to_string(&proj_map_path)
                    .ok()
                    .and_then(|data| serde_json::from_str(&data).ok())
                    .unwrap_or_default();

                let keys = map.get(project).cloned().unwrap_or_default();
                keys.iter().filter_map(|k| bib.get(k)).cloned().collect()
            }
            None => vec![],
        };

        refs.sort_by(|a, b| a.key.cmp(&b.key));
        self.references = refs;
        self.selected_reference = 0;
    }

    fn clear_filtered_refs(&mut self) {
        self.filtered_refs.clear();
    }

    fn enter_search_mode(&mut self) {
        self.mode = Mode::Search;
        // self.search_mode = true;
        self.search_query.clear();
    }

    fn apply_search(&mut self) {
        if self.search_query.is_empty() {
            self.clear_filtered_refs();
            self.mode = Mode::Normal;
            return;
        }

        let query = self.search_query.clone();

        self.filtered_refs = self
            .references
            .iter()
            .filter(|entry| entry_matches(entry, &query))
            .cloned()
            .collect();

        self.selected_reference = 0;
        // self.search_mode = false;
        self.mode = Mode::Normal;
    }

    fn show_alert(&mut self, msg: &str) {
        self.alert_message = Some(msg.to_string());
        self.alert_timer = Some(std::time::Instant::now());
    }

    fn clear_expired_alert(&mut self) {
        if let Some(start) = self.alert_timer {
            if start.elapsed().as_secs() > 3 {
                self.alert_message = None;
                self.alert_timer = None;
            }
        }
    }

    fn suspend_tui(&self) -> io::Result<()> {
        disable_raw_mode()?;
        execute!(io::stdout(), LeaveAlternateScreen)?;
        Ok(())
    }

    fn resume_tui(&self) -> io::Result<()> {
        enable_raw_mode()?;
        execute!(io::stdout(), EnterAlternateScreen)?;
        Ok(())
    }

    fn new_project(&self, project: &str) -> Result<()> {
        if project == "all" {
            anyhow::bail!("'all' is a reserved project name");
        }

        let proj_map_path = self.config.projects_file.to_string_lossy().to_string();
        let mut map = load_projects_map(&proj_map_path)?;
        
        if !map.contains_key(project) {
            map.insert(project.to_string(), vec![]);
            save_projects_map(&proj_map_path, &map)?;
        }
        Ok(())
    }
}

fn main() -> anyhow::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let config = load_config();

    let mut app = App::new(config);
    app.load_references();

    loop {
        // --- draw UI ---
        terminal.draw(|f| {
            // make vertical layout: top = the three panels, bottom = search box (length 3)
            let vchunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(3), Constraint::Length(3)].as_ref())
                .split(f.size());

            // top row: three panels
            let panels = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(20),
                    Constraint::Percentage(30),
                    Constraint::Percentage(50),
                ])
                .split(vchunks[0]);

            // Left panel: projects
            let project_items: Vec<ListItem> = app
                .projects
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    let style = if i == app.selected_project {
                        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    };
                    ListItem::new(Span::styled(p, style))
                })
                .collect();
            let project_list = List::new(project_items)
                .block(Block::default().title("Projects").borders(Borders::ALL));
            f.render_widget(project_list, panels[0]);

            // Middle panel: references - choose filtered list if present

            // let refs_to_show = app.references.clone();
            let refs_to_show = if !app.filtered_refs.is_empty() {
                app.filtered_refs.clone()
            } else {
                app.references.clone()
            };

            let ref_items: Vec<ListItem> = refs_to_show
                .iter()
                .enumerate()
                .map(|(i, r)| {
                    let key = r.key.to_string();
                    let style = if i == app.selected_reference {
                        Style::default().fg(Color::Cyan)
                    } else {
                        Style::default()
                    };
                    ListItem::new(Span::styled(key, style))
                })
                .collect();


            let ref_list = List::new(ref_items)
                .block(Block::default().title("References").borders(Borders::ALL));
            // f.render_widget(ref_list, panels[1]);
            f.render_stateful_widget(ref_list, panels[1], &mut app.list_state);

            // Right panel: details - use refs_to_show and check bounds
            let details = if !refs_to_show.is_empty() && app.selected_reference < refs_to_show.len() {
                let r = &refs_to_show[app.selected_reference];
                format!(
                    "Title:\n{}\nAuthors:\n{}\nYear: {}\nJournal: {}\nDOI: {}\nPublisher: {}\nAbstract:\n {}",
                    r.title().ok().map(chunks_to_string).unwrap_or_else(|| "<no title>".to_string()),
                    r.author().ok().map(authors_to_string).unwrap_or_else(|| "no authors".to_string()),
                    r.date().ok().and_then(date_to_year_string).unwrap_or_else(|| "<no year>".to_string()),
                    r.journal().ok().map(chunks_to_string).unwrap_or_else(|| "<no jounal>".to_string()),
                    r.url().ok().as_deref().unwrap_or(""),
                    r.publisher().ok().map(publisher_string).unwrap_or_else(|| "<no issn>".to_string()),
                    r.abstract_().ok().map(chunks_to_string).unwrap_or_else(|| "<no abstract>".to_string()),
                )
            } else {
                "No reference selected.".to_string()
            };

            f.render_widget(Clear, Block::default().borders(Borders::ALL).inner(panels[2]));

            let ref_para = Paragraph::new(details)
                .wrap(Wrap { trim: false })
                .block(Block::default().title("Details").borders(Borders::ALL));
            f.render_widget(ref_para, panels[2]);

            // Bottom: search box (vchunks[1])
            let search_text = if matches!(app.mode, Mode::Search) {
                format!("/{}", app.search_query)
            } else if !app.filtered_refs.is_empty() {
                // show active filter
                format!("Filter: {}", app.search_query)
            } else {
                String::from("Press / to search")
            };
            let search_para = Paragraph::new(search_text)
                .block(Block::default().title("Search").borders(Borders::ALL));
            f.render_widget(search_para, vchunks[1]);

            if let Some(msg) = &app.alert_message {
                let size = f.size();
                let alert_height = 3;
                let alert_width = msg.len() as u16 + 4; // padding
                let x = (size.width.saturating_sub(alert_width)) / 2;
                let y = size.height.saturating_sub(alert_height) - 1;

                let area = Rect { x, y, width: alert_width, height: alert_height };

                let paragraph = Paragraph::new(Span::styled(
                    msg,
                    Style::default().fg(Color::White).bg(Color::Red),
                ))
                .block(Block::default().borders(Borders::ALL))
                .alignment(Alignment::Center);

                f.render_widget(paragraph, area);
            }

            if matches!(app.mode, Mode::NewProject) {
                let size = f.size();

                let area = Rect {
                    x: size.width / 4,
                    y: size.height / 2 - 2,
                    width: size.width / 2,
                    height: 3,
                };

                f.render_widget(Clear, area);

                let input = Paragraph::new(app.new_project_name.as_str())
                    .block(
                        Block::default()
                            .title("New Project Name")
                            .borders(Borders::ALL),
                    );

                f.render_widget(input, area);
            }

            if matches!(app.mode, Mode::Adding) {
                let size = f.size();

                let area = Rect {
                    x: size.width / 4,
                    y: size.height / 2 - 2,
                    width: size.width / 2,
                    height: 3,
                };

                f.render_widget(Clear, area);

                let input = Paragraph::new(app.new_ref.as_str())
                    .block(
                        Block::default()
                            .title("New reference DOI")
                            .borders(Borders::ALL),
                    );

                f.render_widget(input, area);
            }

            if matches!(app.mode, Mode::Moving) {
                let targets: Vec<String> = app.projects.iter()
                    .filter(|p| p.as_str() != "all")
                    .cloned()
                    .collect();

                let items: Vec<ListItem> = targets.iter().enumerate().map(|(i, p)| {
                    let style = if i == app.moving_target {
                        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    };
                    ListItem::new(p.as_str()).style(style)
                }).collect();

                let size = f.size();
                let height = (targets.len() as u16 + 2).min(size.height / 2);
                let area = Rect {
                    x: size.width / 4,
                    y: size.height / 2 - height / 2,
                    width: size.width / 2,
                    height,
                };

                let block = Block::default()
                    .title(" Copy to project (↑↓ navigate, Enter confirm, Esc cancel) ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Yellow));

                let list = List::new(items).block(block);
                f.render_widget(Clear, area);
                f.render_widget(list, area);
            }

        })?;

        if crossterm::event::poll(std::time::Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') if matches!(app.mode, Mode::Normal) => break,

                    // Typing during search
                    KeyCode::Char(c) if matches!(app.mode, Mode::Search) => {
                        app.search_query.push(c);
                    }

                    // Backspace during search
                    KeyCode::Backspace if matches!(app.mode, Mode::Search) => {
                        app.search_query.pop();
                    }

                    // Cancel search
                    KeyCode::Esc if matches!(app.mode, Mode::Search) => {
                        // app.search_mode = false;
                        app.search_query.clear();
                        app.clear_filtered_refs();
                        app.mode = Mode::Normal;
                    }

                    KeyCode::Esc if matches!(app.mode, Mode::Normal) => {
                        // app.search_mode = false;
                        app.search_query.clear();
                        app.clear_filtered_refs();
                    }

                    KeyCode::Char(c) if matches!(app.mode, Mode::NewProject) => {
                        app.new_project_name.push(c);
                    }
                    
                    KeyCode::Backspace if matches!(app.mode, Mode::NewProject) => {
                        app.new_project_name.pop();
                    }
                    
                    KeyCode::Esc if matches!(app.mode, Mode::NewProject) => {
                        app.mode = Mode::Normal;
                    }

                    KeyCode::Char(c) if matches!(app.mode, Mode::Adding) => {
                        app.new_ref.push(c);
                    }
                    
                    KeyCode::Backspace if matches!(app.mode, Mode::Adding) => {
                        app.new_ref.pop();
                    }

                    KeyCode::Esc if matches!(app.mode, Mode::Adding) => {
                        app.mode = Mode::Normal;
                    }

                    KeyCode::Enter if matches!(app.mode, Mode::NewProject) => {
                        if !app.new_project_name.is_empty() {
                            if !app.projects.contains(&app.new_project_name) && app.new_project_name != "all" {
                                let _ = app.new_project(&app.new_project_name);
                                app.projects.push(app.new_project_name.clone());
                                app.projects.sort();
                                app.selected_project = app.projects.iter().position(|p| p == &app.new_project_name).unwrap_or(0);
                                app.load_references();
                                app.show_alert(&format!("Created new project: {}", app.new_project_name));
                            } else {
                                app.show_alert(&format!("Project {} already exists!", app.new_project_name));
                            }
                        }

                        app.mode = Mode::Normal;
                    }

                    KeyCode::Enter if matches!(app.mode, Mode::Adding) => {
                        if !app.new_ref.is_empty() {
                            let all_bib_path = app.config.all_bib.to_string_lossy().to_string();
                            let proj_map_path = app.config.projects_file.to_string_lossy().to_string();

                            app.suspend_tui().ok();
                            println!("Fetching {}...", app.new_ref);

                            let result = add_reference(&all_bib_path, &app.new_ref);

                            app.resume_tui().ok();
                            terminal.clear().ok();
                            
                            match result {
                                Ok(key) => {
                                    add_to_project(&proj_map_path, &app.projects[app.selected_project], &key)?;
                                    app.load_references();
                                }
                                Err(e) => app.show_alert(&format!("Failed: {e}")),
                            }

                        }

                        app.mode = Mode::Normal;
                    }

                    
                    // Navigation
                    KeyCode::Up | KeyCode::Char('k') if matches!(app.mode, Mode::Normal) => {
                        if app.selected_reference > 0 {
                            app.selected_reference -= 1;
                        }
                    }

                    KeyCode::Down | KeyCode::Char('j') if matches!(app.mode, Mode::Normal) => {
                        let active_refs = if !app.filtered_refs.is_empty() {
                            &app.filtered_refs
                        } else {
                            &app.references
                        };
                        if app.selected_reference + 1 < active_refs.len() {
                            app.selected_reference += 1;
                        }
                    }

                    KeyCode::Left | KeyCode::Char('h') if matches!(app.mode, Mode::Normal) => {
                        if app.selected_project > 0 {
                            app.selected_project -= 1;
                            app.load_references();
                            app.clear_filtered_refs();
                        }
                    }

                    KeyCode::Right | KeyCode::Char('l') if matches!(app.mode, Mode::Normal) => {
                        if app.selected_project + 1 < app.projects.len() {
                            app.selected_project += 1;
                            app.load_references();
                            app.clear_filtered_refs();
                        }
                    }

                    KeyCode::Up | KeyCode::Char('k') if matches!(app.mode, Mode::Moving) => {
                        if app.moving_target > 0 {
                            app.moving_target -= 1;
                        }
                    }

                    KeyCode::Down | KeyCode::Char('j') if matches!(app.mode, Mode::Moving) => {
                        // Exclude "all" (index 0) from targets
                        let targets: Vec<&String> = app.projects.iter().filter(|p| p.as_str() != "all").collect();
                        if app.moving_target + 1 < targets.len() {
                            app.moving_target += 1;
                        }
                    }

                    KeyCode::Esc if matches!(app.mode, Mode::Moving) => {
                        app.mode = Mode::Normal;
                    }
                    
                    KeyCode::Enter if matches!(app.mode, Mode::Moving) => {
                        let targets: Vec<String> = app.projects.iter()
                            .filter(|p| p.as_str() != "all")
                            .cloned()
                            .collect();

                        if let Some(target_project) = targets.get(app.moving_target) {
                            let active_refs = if !app.filtered_refs.is_empty() {
                                &app.filtered_refs
                            } else {
                                &app.references
                            };

                            if let Some(entry) = active_refs.get(app.selected_reference) {
                                let key = entry.key.clone();
                                let proj_map_path = app.config.projects_file.to_string_lossy().to_string();

                                match add_to_project(&proj_map_path, target_project, &key) {
                                    Ok(_) => app.show_alert(&format!("Copied '{}' to '{}'", key, target_project)),
                                    Err(e) => app.show_alert(&format!("Failed: {e}")),
                                }
                            }
                        }

                        app.mode = Mode::Normal;
                    }


                    // 🔍 Enter search mode
                    KeyCode::Char('/') => {
                        app.enter_search_mode();
                    }

                    // ⏎ Apply search OR open PDF
                    KeyCode::Enter => {
                        if matches!(app.mode, Mode::Search) {
                            app.apply_search();
                        } else {
                            // Open PDF if available
                            let active_refs = if !app.filtered_refs.is_empty() {
                                app.filtered_refs.clone()
                            } else {
                                app.references.clone()
                            };

                            if let Some(r) = active_refs.get(app.selected_reference) {
                                let pdf_path = {
                                    // Sanitize DOI for filenames (replace '/' with '-')
                                    let safe_name = r.doi().ok()
                                        .as_deref()
                                        .unwrap_or("")
                                        .replace('/', "-");
                                    app.config.pdfs_dir.join(format!("{safe_name}.pdf"))
                                };

                                if pdf_path.exists() {
                                    if let Err(err) = std::process::Command::new("xdg-open")
                                        .arg(&pdf_path)
                                        .spawn()
                                    {
                                        app.show_alert(&format!("Failed to open PDF: {}", err));
                                    }
                                } else {
                                    app.show_alert(&format!("PDF not found: {}", pdf_path.display()));
                                }
                            }
                        }
                    }

                    KeyCode::Char('e') => {
                        if let Some(entry) = app.references.get(app.selected_reference) {
                            let all_bib_path = app.config.all_bib.to_string_lossy().to_string();
                            let key = &entry.key.clone();

                            app.suspend_tui().ok();
                            let editor = std::env::var("EDITOR").unwrap_or("nvim".into());
                            let _ = std::process::Command::new(editor)
                                .arg(format!("+/@.*{{{},", key))
                                .arg(&all_bib_path)
                                .status();
                            app.resume_tui().ok();
                            terminal.clear().ok();

                            // Check if key was renamed: reload bib and see if old key still exists
                            if let Ok(content) = fs::read_to_string(&all_bib_path) {
                                if let Ok(bib) = Bibliography::parse(&content) {
                                    if bib.get(key).is_none() {
                                        // Key was renamed — find the new key by DOI match or just warn
                                        app.show_alert(&format!(
                                            "⚠️ Key '{key}' no longer exists in all.bib — update projects.json manually or re-add."
                                        ));
                                        // Optionally: remove stale key from project
                                        if let Some(project) = app.projects.get(app.selected_project) {
                                            let proj_map_path = app.config.projects_file.to_string_lossy().to_string();
                                            let _ = remove_from_project(&proj_map_path, project, key);
                                        }
                                    }
                                }
                            }

                            app.load_references();
                        }
                    }

                    KeyCode::Char('N') => {
                        app.mode = Mode::NewProject;
                        app.new_project_name.clear();
                    }

                    KeyCode::Char('A') => {
                        app.mode = Mode::Adding;
                        app.new_ref.clear();
                    }

                    KeyCode::Char('M') => {
                        // Only makes sense if a reference is selected
                        if !app.references.is_empty() {
                            app.mode = Mode::Moving;
                            app.moving_target = 0;
                        }
                    }


                    _ => {}
                }
                app.clear_expired_alert();  
            }
        }

    } // end loop

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}
