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
use refman::{chunks_to_string, authors_to_string, date_to_year_string, publisher_string}; // <-- crate name = [package].name in Cargo.toml

// TODO: 
//  - Cannot show more references than size of terminal! need scrolling
//  - Search/filtering
//  - sort by name, year

struct App {
    projects: Vec<String>,
    selected_project: usize,
    references: Bibliography,
    selected_reference: usize,
    mode: Mode,
    search_mode: bool,
    search_query: String,
    filtered_refs: Bibliography,
    alert_message: Option<String>,
    alert_timer: Option<std::time::Instant>,
    list_state: ListState,
}

enum Mode {
    Normal,
    Search,
    //Adding,
    //Editing,
    //Deleting,
}

impl App {
    fn new() -> Self {
        let mut projects = vec![];
        if let Ok(entries) = fs::read_dir("projects") {
            for entry in entries.flatten() {
                if let Some(name) = entry.path().file_stem() {
                    projects.push(name.to_string_lossy().to_string());
                }
            }
        }
        App {
            projects,
            selected_project: 0,
            references: Bibliography::new(),
            selected_reference: 0,
            mode: Mode::Normal,
            search_mode: false,
            search_query: String::new(),
            filtered_refs: Bibliography::new(),
            alert_message: None,
            alert_timer: None,
            list_state: ListState::default(),
        }
    }

    fn load_references(&mut self) {
        if let Some(project) = self.projects.get(self.selected_project) {
            let path = format!("projects/{}.bib", project);
            if let Ok(data) = fs::read_to_string(&path) {
                if let Ok(refs) = Bibliography::parse(&data) {
                    self.references = refs;
                    self.selected_reference = 0;
                }
            }
        }
    }

    fn clear_filtered_refs(&mut self) {
        self.filtered_refs = Bibliography::new();
    }

    // fn refresh_refs(&mut self) {
    //     self.load_references();
    // }

    fn enter_search_mode(&mut self) {
        self.mode = Mode::Search;
        self.search_mode = true;
        self.search_query.clear();
    }

    // fn apply_search(&mut self) {
    //     if self.search_query.is_empty() {
    //         self.filtered_refs.clear();
    //         self.mode = Mode::Normal;
    //         return;
    //     }

    //     self.filtered_refs = self
    //         .references
    //         .iter()
    //         .map(|entry| {
    //             entry
    //                 .get("title")
    //                 .map(|chunks| {
    //                     chunks
    //                         .iter()
    //                         .map(|span| span.v.get().to_string())
    //                         .collect::<String>()
    //                 })
    //         })
    //         .filter(|r| {
    //             r.to_lowercase().contains(&self.search_query.to_lowercase())
    //                 || r
    //                     .authors
    //                     .iter()
    //                     .any(|a| a.to_lowercase().contains(&self.search_query.to_lowercase()))
    //         })
    //         .cloned()
    //         .collect();

    //     self.selected_reference = 0;
    //     self.search_mode = false;
    //     self.mode = Mode::Normal;
    // }

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

    fn next(&mut self) {
        let i = match self.list_state.selected() {
            Some(i) if i + 1 < self.references.len() => i + 1,
            _ => 0,
        };
        self.list_state.select(Some(i));
    }
    
    fn previous(&mut self) {
        let i = match self.list_state.selected() {
            Some(i) if i > 0 => i - 1,
            _ => self.references.len() - 1,
        };
        self.list_state.select(Some(i));
    }
}



fn main() -> anyhow::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
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
                    Constraint::Percentage(40),
                    Constraint::Percentage(40),
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

            let mut entry_keys: Vec<Entry> = refs_to_show.iter().cloned().collect();
            entry_keys.sort_unstable_by(|a, b| a.key.to_lowercase().cmp(&b.key.to_lowercase()));

            let refs_to_show = refs_to_show.into_vec();

            let ref_items: Vec<ListItem> = entry_keys
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
            

            //ref_items.sort_by_key(|item| &item.author);

            let ref_list = List::new(ref_items)
                .block(Block::default().title("References").borders(Borders::ALL));
            // f.render_widget(ref_list, panels[1]);
            f.render_stateful_widget(ref_list, panels[1], &mut app.list_state);

            // Right panel: details - use refs_to_show and check bounds
            let details = if !refs_to_show.is_empty() && app.selected_reference < refs_to_show.len() {
                let r = &refs_to_show[app.selected_reference];
                format!(
                    "Title:\n{}\nAuthors:\n{}\nYear: {}\nJournal: {}\nDOI: {}\nPublisher: {}\nAbstract:\n {}",
                    //"Title:\n{}\n\nAuthors:\n{}\n\nYear:\t{}\nJournal:\t{}\nDOI:\t{}\nPublisher:\t{}\nAbstract:\n {}",
                    r.title().ok().map(chunks_to_string).unwrap_or_else(|| "<no title>".to_string()),
                    r.author().ok().map(authors_to_string).unwrap_or_else(|| "no authors".to_string()),
                    r.date().ok().and_then(date_to_year_string).unwrap_or_else(|| "<no year>".to_string()),
                    r.journal().ok().map(chunks_to_string).unwrap_or_else(|| "<no jounal>".to_string()),
                    r.doi().ok().as_deref().unwrap_or(""),
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
            let search_text = if app.search_mode {
            //let search_text = if app.mode = Mode::Search {
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

        })?;

        if crossterm::event::poll(std::time::Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => break,
        
                    // Navigation
                    KeyCode::Up | KeyCode::Char('k') => {
                        if app.selected_reference > 0 {
                            app.selected_reference -= 1;
                        }
                    }
        
                    KeyCode::Down | KeyCode::Char('j') => {
                        let active_refs = if !app.filtered_refs.is_empty() {
                            &app.filtered_refs
                        } else {
                            &app.references
                        };
                        if app.selected_reference + 1 < active_refs.len() {
                            app.selected_reference += 1;
                        }
                    }
        
                    KeyCode::Left | KeyCode::Char('h') => {
                        if app.selected_project > 0 {
                            app.selected_project -= 1;
                            app.load_references();
                            app.clear_filtered_refs();
                        }
                    }
        
                    KeyCode::Right | KeyCode::Char('l') => {
                        if app.selected_project + 1 < app.projects.len() {
                            app.selected_project += 1;
                            app.load_references();
                            app.clear_filtered_refs();
                        }
                    }
        
                    // 🔍 Enter search mode
                    KeyCode::Char('/') => {
                        app.enter_search_mode();
                    }
        
                    // ⏎ Apply search OR open PDF
                    KeyCode::Enter => {
                        if app.search_mode {
                            // app.apply_search();
                        } else {
                            // Open PDF if available
                            let active_refs = if !app.filtered_refs.is_empty() {
                                app.filtered_refs.clone()
                            } else {
                                app.references.clone()
                            };
                            let active_refs = active_refs.into_vec();

                            if let Some(r) = active_refs.get(app.selected_reference) {
                                // Try using explicit pdf path from JSON first
                                // let pdf_path = if let Some(p) = &r.pdf {
                                //     std::path::PathBuf::from(p)
                                // } else
                                let pdf_path = if let doi = &r.doi().ok() {
                                    // Sanitize DOI for filenames (replace '/' with '-')
                                    let safe_name = doi.as_deref().unwrap_or("").replace('/', "-");
                                    std::path::Path::new("pdfs").join(format!("{safe_name}.pdf"))
                                } else {
                                    std::path::PathBuf::new()
                                };
                            
                                if pdf_path.exists() {
                                    if let Err(err) = std::process::Command::new("xdg-open")
                                        .arg(&pdf_path)
                                        .spawn()
                                    {
                                        eprintln!("Failed to open PDF: {}", err);
                                    }
                                } else {
                                    //eprintln!("PDF not found: {}", pdf_path.display());
                                    app.show_alert(&format!("PDF not found: {}", pdf_path.display()));
                                }
                            }
                        }
                    }
        
                    // // Typing during search
                    // KeyCode::Char(c) if app.search_mode => {
                    //     app.search_query.push(c);
                    // }
        
                    // // Backspace during search
                    // KeyCode::Backspace if app.search_mode => {
                    //     app.search_query.pop();
                    // }
        
                    // // Cancel search
                    // KeyCode::Esc => {
                    //     app.search_mode = false;
                    //     app.search_query.clear();
                    //     app.filtered_refs.clear();
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
