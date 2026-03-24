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
    export_project_bib,
    import_bib_file,
    rename_project,
    delete_project,
}; // <-- crate name = [package].name in Cargo.toml

// TODO: 
//  - Cannot show more references than size of terminal! need scrolling
//  - Search/filtering
//  - Creating a new project should check if project already exists.
//  - Using direct link to pdf, download the pdf and store it as {doi}.pdf
//

struct FileBrowser {
    current_dir: std::path::PathBuf,
    entries: Vec<std::path::PathBuf>,
    selected: usize,
}

impl FileBrowser {
    fn new(start: std::path::PathBuf) -> Self {
        let mut fb = FileBrowser {
            current_dir: start,
            entries: Vec::new(),
            selected: 0,
        };
        fb.refresh();
        fb
    }

    fn refresh(&mut self) {
        let mut entries: Vec<std::path::PathBuf> = std::fs::read_dir(&self.current_dir)
            .map(|rd| {
                let mut v: Vec<_> = rd
                    .filter_map(|e| e.ok())
                    .map(|e| e.path())
                    .collect();
                // Dirs first, then files, both sorted alphabetically
                v.sort_by(|a, b| {
                    let da = a.is_dir();
                    let db = b.is_dir();
                    db.cmp(&da).then(a.file_name().cmp(&b.file_name()))
                });
                v
            })
            .unwrap_or_default();

        // Prepend ".." to go up
        entries.insert(0, self.current_dir.join(".."));
        self.entries = entries;
        self.selected = 0;
    }

    fn enter(&mut self) -> Option<std::path::PathBuf> {
        if let Some(path) = self.entries.get(self.selected) {
            if path.is_dir() {
                // Canonicalize handles the ".." case
                if let Ok(canonical) = path.canonicalize() {
                    self.current_dir = canonical;
                    self.refresh();
                }
                None
            } else {
                Some(path.clone())
            }
        } else {
            None
        }
    }
}


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
    import_path: String,
    file_browser: Option<FileBrowser>,
    rename_project_name: String,
    project_scroll: usize,
    ref_scroll: usize,
    detail_scroll: usize,
}

enum Mode {
    Normal,
    Search,
    NewProject,
    Adding,
    Moving,
    Importing,
    FileBrowser,
    Help,
    RenameProject,
    ConfirmDelete,
    ConfirmRemoveRef,
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
            import_path: String::new(),
            file_browser: None,
            rename_project_name: String::new(),
            project_scroll: 0,
            ref_scroll: 0,
            detail_scroll: 0,

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
        self.ref_scroll = 0;
        self.detail_scroll = 0;
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

    fn sync_ref_scroll(&mut self, panel_height: usize) {
        let visible = panel_height.saturating_sub(2);
        if self.selected_reference < self.ref_scroll {
            self.ref_scroll = self.selected_reference;
        } else if self.selected_reference >= self.ref_scroll + visible {
            self.ref_scroll = self.selected_reference - visible + 1;
        }
    }
    
    fn sync_project_scroll(&mut self, panel_height: usize) {
        let visible = panel_height.saturating_sub(2);
        if self.selected_project < self.project_scroll {
            self.project_scroll = self.selected_project;
        } else if self.selected_project >= self.project_scroll + visible {
            self.project_scroll = self.selected_project - visible + 1;
        }
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

            // // Left panel: projects
            // let project_items: Vec<ListItem> = app
            //     .projects
            //     .iter()
            //     .enumerate()
            //     .map(|(i, p)| {
            //         let style = if i == app.selected_project {
            //             Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            //         } else {
            //             Style::default()
            //         };
            //         ListItem::new(Span::styled(p, style))
            //     })
            //     .collect();

            app.sync_ref_scroll(panels[1].height as usize);
            app.sync_project_scroll(panels[0].height as usize);

            let visible_projects: Vec<ListItem> = app
                .projects
                .iter()
                .enumerate()
                .skip(app.project_scroll)
                .take(panels[0].height.saturating_sub(2) as usize)
                .map(|(i, p)| {
                    let style = if i == app.selected_project {
                        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    };
                    ListItem::new(Span::styled(p, style))
                })
                .collect();

            let project_list = List::new(visible_projects)
                .block(Block::default().title("Projects").borders(Borders::ALL));
            f.render_widget(project_list, panels[0]);

            // Middle panel: references - choose filtered list if present

            // let refs_to_show = app.references.clone();
            let refs_to_show = if !app.filtered_refs.is_empty() {
                app.filtered_refs.clone()
            } else {
                app.references.clone()
            };

            // let ref_items: Vec<ListItem> = refs_to_show
            //     .iter()
            //     .enumerate()
            //     .map(|(i, r)| {
            //         let key = r.key.to_string();
            //         let style = if i == app.selected_reference {
            //             Style::default().fg(Color::Cyan)
            //         } else {
            //             Style::default()
            //         };
            //         ListItem::new(Span::styled(key, style))
            //     })
            //     .collect();

            let visible_refs: Vec<ListItem> = refs_to_show
                .iter()
                .enumerate()
                .skip(app.ref_scroll)
                .take(panels[1].height.saturating_sub(2) as usize)
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

            let ref_list = List::new(visible_refs)
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
                .scroll((app.detail_scroll as u16, 0))
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

            if matches!(app.mode, Mode::FileBrowser) {
                if let Some(fb) = &app.file_browser {
                    let size = f.size();
                    let area = Rect {
                        x: size.width / 6,
                        y: size.height / 8,
                        width: size.width * 2 / 3,
                        height: size.height * 3 / 4,
                    };
            
                    // Current dir as title, truncated from the left if too long
                    let dir_str = fb.current_dir.to_string_lossy();
                    let max_title = area.width.saturating_sub(4) as usize;
                    let title = if dir_str.len() > max_title {
                        format!("…{}", &dir_str[dir_str.len() - max_title + 1..])
                    } else {
                        dir_str.to_string()
                    };
            
                    let inner_height = area.height.saturating_sub(2) as usize;
                    // Scroll window so selected entry is always visible
                    let scroll_offset = if fb.selected >= inner_height {
                        fb.selected - inner_height + 1
                    } else {
                        0
                    };
            
                    let items: Vec<ListItem> = fb.entries
                        .iter()
                        .enumerate()
                        .skip(scroll_offset)
                        .take(inner_height)
                        .map(|(i, path)| {
                            let is_dir = path.is_dir();
                            let name = if path.ends_with("..") {
                                "  ..".to_string()
                            } else if is_dir {
                                format!("  {}/", path.file_name().unwrap_or_default().to_string_lossy())
                            } else {
                                format!("   {}", path.file_name().unwrap_or_default().to_string_lossy())
                            };
            
                            let style = if i == fb.selected {
                                if is_dir {
                                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                                } else {
                                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
                                }
                            } else if is_dir {
                                Style::default().fg(Color::Blue)
                            } else {
                                Style::default()
                            };
            
                            ListItem::new(name).style(style)
                        })
                        .collect();
            
                    let block = Block::default()
                        .title(format!(" 📂 {} ", title))
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::Yellow));
            
                    let list = List::new(items).block(block);
            
                    f.render_widget(Clear, area);
                    f.render_widget(list, area);
            
                    // Footer hint
                    let hint = Paragraph::new(" Enter: open/select   j/k: navigate   Esc: cancel ")
                        .style(Style::default().fg(Color::DarkGray));
                    let hint_area = Rect { x: area.x + 1, y: area.y + area.height - 1, width: area.width - 2, height: 1 };
                    f.render_widget(Clear, hint_area); // clear
                    f.render_widget(hint, hint_area);
                }
            }

            if matches!(app.mode, Mode::Help) {
                let size = f.size();
                let area = Rect {
                    x: size.width / 6,
                    y: size.height / 8,
                    width: size.width * 2 / 3,
                    height: size.height * 3 / 4,
                };
            
                let help_text = vec![
                    "  NAVIGATION",
                    "  ──────────────────────────────────────",
                    "  h / ←        Previous project",
                    "  l / →        Next project",
                    "  j / ↓        Next reference",
                    "  k / ↑        Previous reference",
                    "  d / u         Scroll details panel down / up",
                    "",
                    "  ACTIONS",
                    "  ──────────────────────────────────────",
                    "  A             Add reference by DOI",
                    "  B             Export project to .bib",
                    "  I             Import a .bib file",
                    "  M             Copy reference to project",
                    "  N             Create new project",
                    "  R             Rename current project",
                    "  D             Delete reference from project",
                    "  e             Edit reference in $EDITOR",
                    "  Enter         Open PDF (if available)",
                    "",
                    "  SEARCH",
                    "  ──────────────────────────────────────",
                    "  /             Enter search mode",
                    "  Enter         Apply search",
                    "  Esc           Clear search / cancel",
                    "",
                    "  OTHER",
                    "  ──────────────────────────────────────",
                    "  H             Toggle this help screen",
                    "  q             Quit",
                    "",
                    "  Press Esc, q or H to close",
                ];
            
                let text = help_text.join("\n");
            
                let paragraph = Paragraph::new(text)
                    .block(
                        Block::default()
                            .title(" Help ")
                            .borders(Borders::ALL)
                            .border_style(Style::default().fg(Color::Yellow)),
                    )
                    .style(Style::default().fg(Color::White));
            
                f.render_widget(Clear, area);
                f.render_widget(paragraph, area);
            }

            if matches!(app.mode, Mode::RenameProject) {
                let size = f.size();
                let area = Rect {
                    x: size.width / 4,
                    y: size.height / 2 - 2,
                    width: size.width / 2,
                    height: 3,
                };
            
                f.render_widget(Clear, area);
            
                let input = Paragraph::new(app.rename_project_name.as_str())
                    .block(
                        Block::default()
                            .title(" Rename project ")
                            .borders(Borders::ALL)
                            .border_style(Style::default().fg(Color::Yellow)),
                    );
            
                f.render_widget(input, area);
            }

            if matches!(app.mode, Mode::ConfirmDelete) {
                let current = &app.projects[app.selected_project];
                let size = f.size();
                let area = Rect {
                    x: size.width / 4,
                    y: size.height / 2 - 2,
                    width: size.width / 2,
                    height: 3,
                };
            
                f.render_widget(Clear, area);
            
                let msg = format!("Delete project '{}'? (y/n)", current);
                let confirm = Paragraph::new(msg)
                    .alignment(Alignment::Center)
                    .block(
                        Block::default()
                            .title(" Confirm ")
                            .borders(Borders::ALL)
                            .border_style(Style::default().fg(Color::Red)),
                    );
            
                f.render_widget(confirm, area);
            }

            if matches!(app.mode, Mode::ConfirmRemoveRef) {
                let active_refs = if !app.filtered_refs.is_empty() {
                    &app.filtered_refs
                } else {
                    &app.references
                };
            
                let key = active_refs
                    .get(app.selected_reference)
                    .map(|e| e.key.as_str())
                    .unwrap_or("?");
            
                let size = f.size();
                let area = Rect {
                    x: size.width / 4,
                    y: size.height / 2 - 2,
                    width: size.width / 2,
                    height: 3,
                };
            
                f.render_widget(Clear, area);
            
                let msg = format!("Remove '{}' from project? (y/n)", key);
                let confirm = Paragraph::new(msg)
                    .alignment(Alignment::Center)
                    .block(
                        Block::default()
                            .title(" Confirm ")
                            .borders(Borders::ALL)
                            .border_style(Style::default().fg(Color::Red)),
                    );
            
                f.render_widget(confirm, area);
            }

            // if matches!(app.mode, Mode::Importing) {
            //     let size = f.size();
            //     let area = Rect {
            //         x: size.width / 4,
            //         y: size.height / 2 - 2,
            //         width: size.width / 2,
            //         height: 3,
            //     };
            // 
            //     f.render_widget(Clear, area);
            // 
            //     let label = if app.projects[app.selected_project] != "all" {
            //         format!("Import .bib path (→ all.bib + '{}')", app.projects[app.selected_project])
            //     } else {
            //         "Import .bib path (→ all.bib only)".to_string()
            //     };
            // 
            //     let input = Paragraph::new(app.import_path.as_str())
            //         .block(Block::default().title(label).borders(Borders::ALL));
            // 
            //     f.render_widget(input, area);
            // }

        })?;

        if crossterm::event::poll(std::time::Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    // quit bshelf
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

                    // Export current project to bib file
                    KeyCode::Char('B') if matches!(app.mode, Mode::Normal) => {
                        if let Some(project) = app.projects.get(app.selected_project) {
                            if project != "all" {
                                let all_bib_path = app.config.all_bib.to_string_lossy().to_string();
                                let proj_map_path = app.config.projects_file.to_string_lossy().to_string();
                                let output_path = format!("{}.bib", project);

                                match export_project_bib(&all_bib_path, &proj_map_path, project, &output_path) {
                                    Ok(_) => app.show_alert(&format!("Exported to {output_path}")),
                                    Err(e) => app.show_alert(&format!("Export failed: {e}")),
                                }
                            } else {
                                app.show_alert("Cannot export 'all' — select a specific project first");
                            }
                        }
                    }

                    // Cancel search filtering
                    KeyCode::Esc if matches!(app.mode, Mode::Normal) => {
                        app.search_query.clear();
                        app.clear_filtered_refs();
                    }

                    // Typing new project
                    KeyCode::Char(c) if matches!(app.mode, Mode::NewProject) => {
                        app.new_project_name.push(c);
                    }
                    
                    // Backspace new project
                    KeyCode::Backspace if matches!(app.mode, Mode::NewProject) => {
                        app.new_project_name.pop();
                    }
                    
                    // Cancel new project
                    KeyCode::Esc if matches!(app.mode, Mode::NewProject) => {
                        app.mode = Mode::Normal;
                    }

                    // Typing new ref
                    KeyCode::Char(c) if matches!(app.mode, Mode::Adding) => {
                        app.new_ref.push(c);
                    }
                    
                    // Backspace new ref
                    KeyCode::Backspace if matches!(app.mode, Mode::Adding) => {
                        app.new_ref.pop();
                    }

                    // Cancel adding new ref
                    KeyCode::Esc if matches!(app.mode, Mode::Adding) => {
                        app.mode = Mode::Normal;
                    }

                    // Create new porject
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

                    // Add given new ref
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
                            app.detail_scroll = 0;
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
                            app.detail_scroll = 0;
                        }
                    }

                    KeyCode::Left | KeyCode::Char('h') if matches!(app.mode, Mode::Normal) => {
                        if app.selected_project > 0 {
                            app.selected_project -= 1;
                            app.project_scroll = 0;
                            app.load_references();
                            app.clear_filtered_refs();
                        }
                    }

                    KeyCode::Right | KeyCode::Char('l') if matches!(app.mode, Mode::Normal) => {
                        if app.selected_project + 1 < app.projects.len() {
                            app.selected_project += 1;
                            app.project_scroll = 0;
                            app.load_references();
                            app.clear_filtered_refs();
                        }
                    }

                    // Scroll detail panel
                    KeyCode::Char('u') if matches!(app.mode, Mode::Normal) => {
                        app.detail_scroll = app.detail_scroll.saturating_sub(3);
                    }
                    
                    KeyCode::Char('d') if matches!(app.mode, Mode::Normal) => {
                        app.detail_scroll += 3;
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

                    // Cancel moving ref to new project
                    KeyCode::Esc if matches!(app.mode, Mode::Moving) => {
                        app.mode = Mode::Normal;
                    }
                    
                    // Accept moving ref to selected project
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
                    KeyCode::Enter if matches!(app.mode, Mode::Search) => {
                        app.apply_search();
                    } 
                    KeyCode::Enter if matches!(app.mode, Mode::Normal) => {
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

                    // edit selected reference
                    KeyCode::Char('e') if matches!(app.mode, Mode::Normal) => {
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

                    // Create a new project
                    KeyCode::Char('N') if matches!(app.mode, Mode::Normal) => {
                        app.mode = Mode::NewProject;
                        app.new_project_name.clear();
                    }

                    // Add a new ref
                    KeyCode::Char('A') if matches!(app.mode, Mode::Normal) => {
                        app.mode = Mode::Adding;
                        app.new_ref.clear();
                    }

                    // Move ref to a new project
                    KeyCode::Char('M') if matches!(app.mode, Mode::Normal) => {
                        // Only makes sense if a reference is selected
                        if !app.references.is_empty() {
                            app.mode = Mode::Moving;
                            app.moving_target = 0;
                        }
                    }

                    KeyCode::Char('I') if matches!(app.mode, Mode::Normal) => {
                        //let start = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
                        let start = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
                        app.file_browser = Some(FileBrowser::new(start));
                        app.mode = Mode::FileBrowser;
                    }
                    
                    KeyCode::Up | KeyCode::Char('k') if matches!(app.mode, Mode::FileBrowser) => {
                        if let Some(fb) = &mut app.file_browser {
                            if fb.selected > 0 {
                                fb.selected -= 1;
                            }
                        }
                    }
                    
                    KeyCode::Down | KeyCode::Char('j') if matches!(app.mode, Mode::FileBrowser) => {
                        if let Some(fb) = &mut app.file_browser {
                            if fb.selected + 1 < fb.entries.len() {
                                fb.selected += 1;
                            }
                        }
                    }

                    KeyCode::Enter if matches!(app.mode, Mode::FileBrowser) => {
                        let selected_file = app.file_browser.as_mut().and_then(|fb| fb.enter());
                    
                        if let Some(path) = selected_file {
                            let all_bib_path = app.config.all_bib.to_string_lossy().to_string();
                            let proj_map_path = app.config.projects_file.to_string_lossy().to_string();
                    
                            match import_bib_file(&all_bib_path, path.to_str().unwrap_or("")) {
                                Ok(keys) if keys.is_empty() => {
                                    app.show_alert("No new entries found (all already exist)");
                                }
                                Ok(keys) => {
                                    let current_project = app.projects[app.selected_project].clone();
                                    if current_project != "all" {
                                        for key in &keys {
                                            let _ = add_to_project(&proj_map_path, &current_project, key);
                                        }
                                        app.show_alert(&format!(
                                            "Imported {} entries into '{}' and all.bib",
                                            keys.len(), current_project
                                        ));
                                    } else {
                                        app.show_alert(&format!("Imported {} entries into all.bib", keys.len()));
                                    }
                                    app.load_references();
                                }
                                Err(e) => app.show_alert(&format!("Import failed: {e}")),
                            }
                    
                            app.file_browser = None;
                            app.mode = Mode::Normal;
                        }
                        // If enter was on a dir, FileBrowser::enter() already navigated — stay in mode
                    }
                    
                    KeyCode::Esc if matches!(app.mode, Mode::FileBrowser) => {
                        app.file_browser = None;
                        app.mode = Mode::Normal;
                    }

                    KeyCode::Char('H') if matches!(app.mode, Mode::Normal) => {
                        app.mode = Mode::Help;
                    }
                    
                    KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('H') if matches!(app.mode, Mode::Help) => {
                        app.mode = Mode::Normal;
                    }

                    KeyCode::Char('D') if matches!(app.mode, Mode::Normal) => {
                        let active_refs = if !app.filtered_refs.is_empty() {
                            &app.filtered_refs
                        } else {
                            &app.references
                        };
                    
                        if active_refs.get(app.selected_reference).is_some() {
                            let current = &app.projects[app.selected_project];
                            if current == "all" {
                                app.show_alert("Cannot delete from 'all' — switch to a specific project");
                            } else {
                                app.mode = Mode::ConfirmRemoveRef;
                            }
                        }
                    }

                    KeyCode::Char('y') | KeyCode::Char('Y') if matches!(app.mode, Mode::ConfirmRemoveRef) => {
                        let active_refs = if !app.filtered_refs.is_empty() {
                            app.filtered_refs.clone()
                        } else {
                            app.references.clone()
                        };
                    
                        if let Some(entry) = active_refs.get(app.selected_reference) {
                            let key = entry.key.clone();
                            let current_project = app.projects[app.selected_project].clone();
                            let proj_map_path = app.config.projects_file.to_string_lossy().to_string();
                    
                            match remove_from_project(&proj_map_path, &current_project, &key) {
                                Ok(_) => {
                                    app.show_alert(&format!("Removed '{}' from '{}'", key, current_project));
                                    app.load_references();
                                }
                                Err(e) => app.show_alert(&format!("Failed: {e}")),
                            }
                        }
                        app.mode = Mode::Normal;
                    }
                    
                    KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc
                        if matches!(app.mode, Mode::ConfirmRemoveRef) =>
                    {
                        app.show_alert("Removal cancelled");
                        app.mode = Mode::Normal;
                    }

                    KeyCode::Char('R') if matches!(app.mode, Mode::Normal) => {
                        let current = &app.projects[app.selected_project];
                        if current == "all" {
                            app.show_alert("Cannot rename 'all'");
                        } else {
                            app.rename_project_name = current.clone();
                            app.mode = Mode::RenameProject;
                        }
                    }
                    
                    KeyCode::Char(c) if matches!(app.mode, Mode::RenameProject) => {
                        app.rename_project_name.push(c);
                    }
                    
                    KeyCode::Backspace if matches!(app.mode, Mode::RenameProject) => {
                        app.rename_project_name.pop();
                    }
                    
                    KeyCode::Esc if matches!(app.mode, Mode::RenameProject) => {
                        app.mode = Mode::Normal;
                    }
                    
                    KeyCode::Enter if matches!(app.mode, Mode::RenameProject) => {
                        let new_name = app.rename_project_name.trim().to_string();
                        let old_name = app.projects[app.selected_project].clone();
                    
                        if !new_name.is_empty() && new_name != old_name {
                            let proj_map_path = app.config.projects_file.to_string_lossy().to_string();
                    
                            match rename_project(&proj_map_path, &old_name, &new_name) {
                                Ok(_) => {
                                    // Update in-memory project list, preserving selection
                                    app.projects[app.selected_project] = new_name.clone();
                                    app.projects.sort();
                                    // "all" is always index 0, find new position of renamed project
                                    app.selected_project = app.projects
                                        .iter()
                                        .position(|p| p == &new_name)
                                        .unwrap_or(0);
                                    app.show_alert(&format!("Renamed '{}' to '{}'", old_name, new_name));
                                }
                                Err(e) => app.show_alert(&format!("Rename failed: {e}")),
                            }
                        }
                    
                        app.mode = Mode::Normal;
                    }

                    KeyCode::Char('X') if matches!(app.mode, Mode::Normal) => {
                        let current = app.projects[app.selected_project].clone();
                        if current == "all" {
                            app.show_alert("Cannot delete 'all'");
                        } else {
                            app.mode = Mode::ConfirmDelete;
                            // let proj_map_path = app.config.projects_file.to_string_lossy().to_string();
                    
                            // match delete_project(&proj_map_path, &current) {
                            //     Ok(_) => {
                            //         app.projects.remove(app.selected_project);
                            //         // Make sure selection doesn't go out of bounds
                            //         app.selected_project = app.selected_project
                            //             .saturating_sub(1)
                            //             .min(app.projects.len().saturating_sub(1));
                            //         app.load_references();
                            //         app.show_alert(&format!("Deleted project '{}'", current));
                            //     }
                            //     Err(e) => app.show_alert(&format!("Delete failed: {e}")),
                            // }
                        }
                    }

                    KeyCode::Char('y') | KeyCode::Char('Y') if matches!(app.mode, Mode::ConfirmDelete) => {
                        let current = app.projects[app.selected_project].clone();
                        let proj_map_path = app.config.projects_file.to_string_lossy().to_string();
                    
                        match delete_project(&proj_map_path, &current) {
                            Ok(_) => {
                                app.projects.remove(app.selected_project);
                                app.selected_project = app.selected_project
                                    .saturating_sub(1)
                                    .min(app.projects.len().saturating_sub(1));
                                app.load_references();
                                app.show_alert(&format!("Deleted project '{}'", current));
                            }
                            Err(e) => app.show_alert(&format!("Delete failed: {e}")),
                        }
                        app.mode = Mode::Normal;
                    }
                    
                    KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc
                        if matches!(app.mode, Mode::ConfirmDelete) =>
                    {
                        app.show_alert("Delete cancelled");
                        app.mode = Mode::Normal;
                    }
                    
                    // KeyCode::Char(c) if matches!(app.mode, Mode::Importing) => {
                    //     app.import_path.push(c);
                    // }
                    // 
                    // KeyCode::Backspace if matches!(app.mode, Mode::Importing) => {
                    //     app.import_path.pop();
                    // }
                    // 
                    // KeyCode::Esc if matches!(app.mode, Mode::Importing) => {
                    //     app.mode = Mode::Normal;
                    // }
                    
                    // KeyCode::Enter if matches!(app.mode, Mode::Importing) => {
                    //     if !app.import_path.is_empty() {
                    //         let all_bib_path = app.config.all_bib.to_string_lossy().to_string();
                    //         let proj_map_path = app.config.projects_file.to_string_lossy().to_string();
                    //         let import_path = app.import_path.trim().to_string();
                    // 
                    //         // Expand ~ if present
                    //         let expanded = if import_path.starts_with("~/") {
                    //             dirs::home_dir()
                    //                 .map(|h| h.join(&import_path[2..]))
                    //                 .and_then(|p| p.to_str().map(|s| s.to_string()))
                    //                 .unwrap_or(import_path.clone())
                    //         } else {
                    //             import_path.clone()
                    //         };
                    // 
                    //         match import_bib_file(&all_bib_path, &expanded) {
                    //             Ok(keys) if keys.is_empty() => {
                    //                 app.show_alert("No new entries found (all already exist)");
                    //             }
                    //             Ok(keys) => {
                    //                 // If current project is not "all", also add imported keys to it
                    //                 let current_project = app.projects[app.selected_project].clone();
                    //                 if current_project != "all" {
                    //                     for key in &keys {
                    //                         let _ = add_to_project(&proj_map_path, &current_project, key);
                    //                     }
                    //                     app.show_alert(&format!(
                    //                         "Imported {} entries into '{}' and all.bib",
                    //                         keys.len(), current_project
                    //                     ));
                    //                 } else {
                    //                     app.show_alert(&format!(
                    //                         "Imported {} entries into all.bib", keys.len()
                    //                     ));
                    //                 }
                    //                 app.load_references();
                    //             }
                    //             Err(e) => app.show_alert(&format!("Import failed: {e}")),
                    //         }
                    //     }
                    //     app.mode = Mode::Normal;
                    // }


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
