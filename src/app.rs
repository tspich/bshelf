use biblatex::{Bibliography, Entry};
use ratatui::widgets::ListState;
use crossterm::{
    execute,
    terminal::{
        disable_raw_mode,
        enable_raw_mode,
        EnterAlternateScreen,
        LeaveAlternateScreen
    },
};
use std::{fs, io};
use anyhow::Result;

use bshelf::{
    Config,
    load_projects_map,
    ProjectsMap,
    save_projects_map,
    entry_matches,
};

// ---------------------------------------------------------------------------- 
// Mode
// ---------------------------------------------------------------------------- 

pub enum Mode {
    Normal,
    Search,
    NewProject,
    Adding,
    Moving,
    FileBrowser,
    Help,
    RenameProject,
    ConfirmDelete,
    ConfirmRemoveRef,
    PdfDoi,
    ImportProject,
    ImportNewProject,
}

pub fn mode_name(mode: &Mode) -> &'static str {
    match mode {
        Mode::Normal           => "NORMAL",
        Mode::Search           => "SEARCH",
        Mode::NewProject       => "NEW PROJECT",
        Mode::Adding           => "ADD REF",
        Mode::Moving           => "COPY TO",
        Mode::FileBrowser      => "FILE BROWSER",
        Mode::RenameProject    => "RENAME",
        Mode::ConfirmDelete    => "DELETE PROJECT",
        Mode::ConfirmRemoveRef => "REMOVE REF",
        Mode::Help             => "HELP",
        Mode::PdfDoi           => "PDF",
        Mode::ImportProject    => "IMPORT TO",
        Mode::ImportNewProject => "IMPORT NEW PROJECT",
    }
}

// ----------------------------------------------------------------------------
// FileBrowser
// ----------------------------------------------------------------------------

pub enum FileBrowserMode {
    Bib,
    Pdf,
}

pub struct FileBrowser {
    pub current_dir: std::path::PathBuf,
    pub entries: Vec<std::path::PathBuf>,
    pub selected: usize,
    pub filter: String,
    pub filtering: bool,
    pub browser_mode: FileBrowserMode,
    pub multi_selected: std::collections::HashSet<std::path::PathBuf>,
}

impl FileBrowser {
    pub fn new(start: std::path::PathBuf, browser_mode: FileBrowserMode) -> Self {
        let mut fb = FileBrowser {
            current_dir: start,
            entries: Vec::new(),
            selected: 1,
            filter: String::new(),
            filtering: false,
            browser_mode,
            multi_selected: std::collections::HashSet::new(),
        };
        fb.refresh();
        fb
    }

    pub fn refresh(&mut self) {
        let mut entries: Vec<std::path::PathBuf> = std::fs::read_dir(&self.current_dir)
            .map(|rd| {
                let mut v: Vec<_> = rd
                    .filter_map(|e| e.ok())
                    .map(|e| e.path())
                    .filter(|p| {
                        p.is_dir() || p.extension()
                            .and_then(|e| e.to_str())
                            .map(|e| match self.browser_mode {
                                FileBrowserMode::Bib => e == "bib",
                                FileBrowserMode::Pdf => e == "pdf",
                            })
                            .unwrap_or(false)
                    })
                    .collect();
                v.sort_by(|a, b| {
                    let da = a.is_dir();
                    let db = b.is_dir();
                    db.cmp(&da).then(a.file_name().cmp(&b.file_name()))
                });
                v
            })
            .unwrap_or_default();

        entries.insert(0, self.current_dir.join(".."));
        self.entries = entries;
        self.selected = 1;
        self.filter.clear();
        self.filtering = false;
        self.multi_selected.clear();
    }

    pub fn visible_entries(&self) -> Vec<&std::path::PathBuf> {
        if self.filter.is_empty() {
            self.entries.iter().collect()
        } else {
            self.entries
                .iter()
                .filter(|p| {
                    p.ends_with("..")
                        || p.file_name()
                            .and_then(|n| n.to_str())
                            .map(|n| n.to_lowercase().contains(&self.filter.to_lowercase()))
                            .unwrap_or(false)
                })
                .collect()
        }
    }

    pub fn enter(&mut self) -> Option<std::path::PathBuf> {
        let visible = self.visible_entries();
        if let Some(path) = visible.get(self.selected) {
            if path.is_dir() {
                if let Ok(canonical) = path.canonicalize() {
                    self.current_dir = canonical;
                    self.refresh();
                }
                None
            } else {
                Some((*path).clone())
            }
        } else {
            None
        }
    }

    pub fn toggle_current(&mut self) {
        let path = self.visible_entries()
            .get(self.selected)
            .filter(|p| !p.is_dir() && !p.ends_with(".."))
            .map(|p| (*p).clone());

        if let Some(path) = path {
            if self.multi_selected.contains(&path) {
                self.multi_selected.remove(&path);
            } else {
                self.multi_selected.insert(path);
            }
        }
    }
}

// ---------------------------------------------------------------------------- 
// App
// ---------------------------------------------------------------------------- 

pub struct App {
    pub config: Config,
    pub projects: Vec<String>,
    pub selected_project: usize,
    pub references: Vec<Entry>,
    pub selected_reference: usize,
    pub mode: Mode,
    pub search_query: String,
    pub new_project_name: String,
    pub new_ref: String,
    pub filtered_refs: Vec<Entry>,
    pub alert_message: Option<String>,
    pub alert_timer: Option<std::time::Instant>,
    pub list_state: ListState,
    pub moving_target: usize,
    pub file_browser: Option<FileBrowser>,
    pub rename_project_name: String,
    pub project_scroll: usize,
    pub ref_scroll: usize,
    pub detail_scroll: usize,
    pub pending_pdf_path: Option<std::path::PathBuf>,
    pub pdf_doi_input: String,
    pub clipboard: Option<arboard::Clipboard>,
    pub help_scroll: usize,
    pub pending_import_paths: Vec<std::path::PathBuf>,
    pub import_project_target: usize,  // index into the picker list
    pub import_new_project_name: String,
}

impl App {
    pub fn new(config: Config) -> Self {
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
            search_query: String::new(),
            new_project_name: String::new(),
            new_ref: String::new(),
            filtered_refs: Vec::new(),
            alert_message: None,
            alert_timer: None,
            list_state: ListState::default(),
            moving_target: 0,
            file_browser: None,
            rename_project_name: String::new(),
            project_scroll: 0,
            ref_scroll: 0,
            detail_scroll: 0,
            pending_pdf_path: None,
            pdf_doi_input: String::new(),
            clipboard: arboard::Clipboard::new().ok(),
            help_scroll: 0,
            pending_import_paths: Vec::new(),
            import_project_target: 0,
            import_new_project_name: String::new(),
        }
    }

    pub fn load_references(&mut self) {
        let all_bib_path = self.config.all_bib.to_string_lossy().to_string();

        let bib = fs::read_to_string(&all_bib_path)
            .ok()
            .and_then(|content| Bibliography::parse(&content).ok())
            .unwrap_or_default();

        let selected = self.projects.get(self.selected_project).map(|s| s.as_str());

        let mut refs: Vec<Entry> = match selected {
            Some("all") => bib.iter().cloned().collect(),
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

    pub fn clear_filtered_refs(&mut self) {
        self.filtered_refs.clear();
    }

    pub fn enter_search_mode(&mut self) {
        self.mode = Mode::Search;
        self.search_query.clear();
    }

    pub fn apply_search(&mut self) {
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
        self.mode = Mode::Normal;
    }

    pub fn show_alert(&mut self, msg: &str) {
        self.alert_message = Some(msg.to_string());
        self.alert_timer = Some(std::time::Instant::now());
    }

    pub fn clear_expired_alert(&mut self) {
        if let Some(start) = self.alert_timer {
            if start.elapsed().as_secs() > 3 {
                self.alert_message = None;
                self.alert_timer = None;
            }
        }
    }

    pub fn suspend_tui(&self) -> io::Result<()> {
        disable_raw_mode()?;
        execute!(io::stdout(), LeaveAlternateScreen)?;
        Ok(())
    }

    pub fn resume_tui(&self) -> io::Result<()> {
        enable_raw_mode()?;
        execute!(io::stdout(), EnterAlternateScreen)?;
        Ok(())
    }

    pub fn new_project(&self, project: &str) -> Result<()> {
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

    pub fn sync_ref_scroll(&mut self, panel_height: usize) {
        let visible = panel_height.saturating_sub(2);
        if self.selected_reference < self.ref_scroll {
            self.ref_scroll = self.selected_reference;
        } else if self.selected_reference >= self.ref_scroll + visible {
            self.ref_scroll = self.selected_reference - visible + 1;
        }
    }

    pub fn sync_project_scroll(&mut self, panel_height: usize) {
        let visible = panel_height.saturating_sub(2);
        if self.selected_project < self.project_scroll {
            self.project_scroll = self.selected_project;
        } else if self.selected_project >= self.project_scroll + visible {
            self.project_scroll = self.selected_project - visible + 1;
        }
    }
}
