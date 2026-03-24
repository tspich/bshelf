use serde::Deserialize;
use anyhow::Result;
use std::fs;
use std::path::PathBuf;
use std::ops::Range;
use reqwest::blocking;
use serde_json::Value;
use biblatex::{Bibliography, Chunks, DateValue};
use biblatex::{Entry, EntryType, PermissiveType};
use biblatex::{Chunk, Spanned};
use crossref::Crossref;
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::io;
use std::process::Command;
use std::collections::HashMap;

fn field<S: AsRef<str>>(s: S) -> Vec<Spanned<Chunk>> {
    vec![Spanned::detached(Chunk::Normal(s.as_ref().to_string()))]
}

// Produce double braces {{...}} can be helpeful in some context like title?
// fn field<S: AsRef<str>>(s: S) -> Vec<Spanned<Chunk>> {
//     vec![Spanned::detached(Chunk::Verbatim(s.as_ref().to_string()))]
// }

fn uuid_fallback() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos().to_string())
        .unwrap_or_else(|_| "dup".to_string())
}

pub type ProjectsMap = HashMap<String, Vec<String>>;

pub fn chunks_to_string(chunks: &[Spanned<Chunk>]) -> String {
    chunks
        .iter()
        .map(|c| c.v.get())
        .collect::<String>()
}

pub fn authors_to_string(authors: Vec<biblatex::Person>) -> String {
    authors
        .into_iter()
        .map(|p| {
            match (&p.name, &p.given_name) {
                (g, f) => format!("- {g} {f}"),
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn publisher_string(publisher: Vec<Chunks>) -> String {
    publisher
        .into_iter()
        .map(|p| {
            chunks_to_string(&p)
        })
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn date_to_year_string(
    date: PermissiveType<biblatex::Date>,
) -> Option<String> {
    match date {
        PermissiveType::Typed(d) => {
            match d.value {
                DateValue::At( year )
                | DateValue::After( year ) => Some(year.to_string()),
                _ => None,
            }
        }
        PermissiveType::Chunks(_) => None,
    }
}

pub fn volume_string(
    vol: PermissiveType<i64>,
) -> Option<String> {
    match vol {
        PermissiveType::Typed(v) => {
            Some(v.to_string())
        }
        PermissiveType::Chunks(_) => None,
    }
}

pub fn pages_string(
    pages: PermissiveType<Vec<Range<u32>>>,
) -> Option<String> {
    match pages {
        PermissiveType::Typed(page) => {
            Some(page
                .into_iter()
                .map(|p| {
                    match (&p.start, &p.end) {
                        (s, e) => format!("{s} {e}"),
                    }
                })
                .collect::<Vec<_>>()
                .join(", "))

        }
        PermissiveType::Chunks(_) => None,
    }
}

pub fn load_projects_map(path: &str) -> Result<ProjectsMap> {
    if !std::path::Path::new(path).exists() {
        return Ok(HashMap::new());
    }
    let content = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&content)?)
}

pub fn save_projects_map(path: &str, map: &ProjectsMap) -> Result<()> {
    fs::write(path, serde_json::to_string_pretty(map)?)?;
    Ok(())
}

pub fn add_to_project(proj_map_path: &str, project: &str, key: &str) -> Result<()> {
    let mut map = load_projects_map(proj_map_path)?;
    let keys = map.entry(project.to_string()).or_default();
    if !keys.contains(&key.to_string()) {
        keys.push(key.to_string());
        save_projects_map(proj_map_path, &map)?;
    }
    Ok(())
}

pub fn remove_from_project(proj_map_path: &str, project: &str, key: &str) -> Result<()> {
    let mut map = load_projects_map(proj_map_path)?;
    if let Some(keys) = map.get_mut(project) {
        keys.retain(|k| k != key);
        save_projects_map(proj_map_path, &map)?;
    }
    Ok(())
}

pub fn project_entries<'a>(
    bib: &'a Bibliography,
    keys: &[String],
) -> Vec<&'a Entry> {
    keys.iter()
    .filter_map(|k| bib.get(k))
    .collect()
}

pub fn add_reference(all_bib: &str, doi: &str) -> Result<String> {
    // 1. Load bibliography
    let content = fs::read_to_string(&all_bib)?;
    let mut bib = Bibliography::parse(&content)?;

    let doi_url = format!("https://doi.org/{doi}");

    // 2. Duplicate check
    if let Some(existing) = bib.iter().find(|e| {
        e.get("doi")
            .map(|chunks| chunks.iter().any(|c| c.v.get() == doi_url)) 
            .unwrap_or(false)
    }){
        return Ok(existing.key.clone());
    }

    // 3. Fetch Crossref
    let client = Crossref::builder()
        .build()
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;

    let work = client
        .work(doi)
        .map_err(|e| anyhow::anyhow!("crossref work error: {e:?}"))?;

    let title = work
        .title
        .get(0)
        .cloned()
        .unwrap_or_else(|| "<no title>".to_string());

    let journal = work
        .container_title
        .as_ref()
        .and_then(|v| v.get(0))
        .cloned()
        .unwrap_or_default();

    let year = work
        .issued
        .date_parts
        .0
        .get(0)
        .and_then(|d| d.get(0))
        .and_then(|d| d.map(|y| y.to_string()))
        .unwrap_or_default();

    let authors: Vec<String> = work
        .author
        .as_ref()
        .map(|v| {
            v.iter()
                .map(|c| {
                    let given = c.given.as_deref().unwrap_or("").trim();
                    let family = c.family.clone();
                    format!("{} {}", given, family).trim().to_string()
                })
                .collect()
        })
        .unwrap_or_default();

    let volume = work.volume.clone().unwrap_or_default();
    let issue = work.issue.clone().unwrap_or_default();
    let pages = work.page.clone().unwrap_or_default(); // note `page` field in Work
    let issn = work.issn.as_ref().map(|v| v.join(", ")).unwrap_or_default();
    let publisher = work.publisher.clone();
    let doi_url = work.url.clone();

    let abstract_text = work
        .abstract_
        .as_ref()
        .map(|s| s.clone())
        .unwrap_or_default();

    // 4. Citation key
    // let key = format!(
    //     "{}_{}",
    //     authors
    //         .get(0)
    //         .and_then(|a| a.split_whitespace().last())
    //         .unwrap_or("ref")
    //         .to_lowercase(),
    //     year,
    //     //title
    //     //    .split_whitespace()
    //     //    .next()
    //     //    .unwrap_or("ref")
    //     //    .to_lowercase(),
    // );

    let base_key = format!(
        "{}_{}",
        authors
            .get(0)
            .and_then(|a| a.split_whitespace().last())
            .unwrap_or("ref")
            .to_lowercase(),
        year,
    );
    
    let key = if bib.get(&base_key).is_none() {
        base_key.clone()
    } else {
        // Find the first available suffix: smith_2023a, smith_2023b, ...
        ('a'..='z')
            .map(|c| format!("{}{}", base_key, c))
            .find(|candidate| bib.get(candidate).is_none())
            .unwrap_or_else(|| format!("{}_{}", base_key, uuid_fallback()))
    };

    // 5. Build BibTeX entry
    let mut entry = Entry::new(key.clone(), EntryType::Article);

    entry.set("title", field(title));
    entry.set("author", field(authors.join(" and ")));
    entry.set("date", field(&year));
    entry.set("journal", field(journal));
    entry.set("volume", field(&volume));
    entry.set("issue", field(&issue));
    entry.set("pages", field(&pages));
    entry.set("issn", field(&issn));
    entry.set("publisher", field(&publisher));
    entry.set("doi", field(&doi));
    entry.set("url", field(&doi_url));
    entry.set("abstract", field(&abstract_text));


    // 6. Unpaywall PDF
    let unpaywall_url = format!(
        "https://api.unpaywall.org/v2/{doi}?email=your@email.com"
    );
    let up: Value = blocking::get(&unpaywall_url)?.json()?;

    if let Some(url) = up["best_oa_location"]["url_for_pdf"].as_str() {
        fs::create_dir_all("pdfs")?;
        let filename = doi.replace("/", "_") + ".pdf";
        let path = format!("pdfs/{filename}");

        let resp = blocking::get(url)?;
        if resp
            .headers()
            .get("content-type")
            .and_then(|h| h.to_str().ok())
            .map(|ct| ct.contains("pdf"))
            .unwrap_or(false)
        {
            fs::write(&path, resp.bytes()?)?;
            entry.set("file", field(path));
        }
    }

    // 7. Add + save
    bib.insert(entry);
    // fs::write(&all_bib, bib.to_bibtex_string())?;
    fs::write(&all_bib, bib.to_biblatex_string())?;

    Ok(key)
}

pub fn entry_matches(entry: &biblatex::Entry, query: &str) -> bool {
    let query = query.to_lowercase();

    // -------- TITLE --------
    let title_match = entry
        .get("title")
        .map(|chunks| {
            chunks
                .iter()
                .map(|span| span.v.get())
                .collect::<String>()
        })
        .map(|title| title.to_lowercase().contains(&query))
        .unwrap_or(false);

    if title_match {
        return true;
    }

    // -------- AUTHORS --------
    let author_match = entry
        .author()
        .ok()
        .map(|authors| {
            authors.iter().any(|a| {
                // a.given_name is a String; use as_str()
                let full_name = if a.given_name.trim().is_empty() {
                    a.name.clone()
                } else {
                    format!("{} {}", a.given_name.as_str(), a.name)
                };

                full_name.to_lowercase().contains(&query)
            })
        })
        .unwrap_or(false);
    author_match
}

pub fn open_editor(path: &str, key: &str) -> io::Result<()> {
    let mut stdout = io::stdout();

    // 1️⃣ Restore terminal before launching nvim
    disable_raw_mode()?;
    execute!(stdout, LeaveAlternateScreen)?;

    // 2️⃣ Launch nvim and wait for it
    Command::new("nvim")
        .arg(format!("+/@.*{{{},", key))
        .arg(path)
        .status()?;   // <-- BLOCK until exit

    // 3️⃣ Reinitialize TUI
    execute!(stdout, EnterAlternateScreen)?;
    enable_raw_mode()?;

    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct Config {
    pub projects_file: PathBuf,
    pub pdfs_dir: PathBuf,
    pub all_bib: PathBuf,
}

pub fn load_config() -> Config {
    let config_dir = dirs::config_dir()
        .expect("Could not find config directory")
        .join("bshelf");

    let config_path = config_dir.join("config.toml");

    let contents = fs::read_to_string(&config_path)
        .expect("Could not read config file");

    toml::from_str(&contents)
        .expect("Invalid config file")
}

pub fn export_project_bib(all_bib_path: &str, proj_map_path: &str, project: &str, output_path: &str) -> Result<()> {
    let content = fs::read_to_string(all_bib_path)?;
    let bib = Bibliography::parse(&content)?;

    let map = load_projects_map(proj_map_path)?;
    let keys = map.get(project).cloned().unwrap_or_default();

    let mut project_bib = Bibliography::new();
    for key in &keys {
        if let Some(entry) = bib.get(key) {
            project_bib.insert(entry.clone());
        }
    }

    fs::write(output_path, project_bib.to_biblatex_string())?;
    Ok(())
}

pub fn import_bib_file(all_bib_path: &str, import_path: &str) -> Result<Vec<String>> {
    let import_content = fs::read_to_string(import_path)?;
    let import_bib = Bibliography::parse(&import_content)?;

    let all_content = fs::read_to_string(all_bib_path).unwrap_or_default();
    let mut all_bib = if all_content.is_empty() {
        Bibliography::new()
    } else {
        Bibliography::parse(&all_content)?
    };

    let mut imported_keys = Vec::new();

    for entry in import_bib.iter() {
        // Deduplicate by DOI if present, otherwise by key
        let already_exists = if let Some(doi_chunks) = entry.get("doi") {
            let doi = chunks_to_string(doi_chunks);
            all_bib.iter().any(|e| {
                e.get("doi")
                    .map(|c| chunks_to_string(c) == doi)
                    .unwrap_or(false)
            })
        } else {
            all_bib.get(&entry.key).is_some()
        };

        if !already_exists {
            all_bib.insert(entry.clone());
            imported_keys.push(entry.key.clone());
        }
    }

    fs::write(all_bib_path, all_bib.to_biblatex_string())?;
    Ok(imported_keys)
}

pub fn rename_project(proj_map_path: &str, old_name: &str, new_name: &str) -> Result<()> {
    let mut map = load_projects_map(proj_map_path)?;

    if !map.contains_key(old_name) {
        anyhow::bail!("Project '{}' does not exist", old_name);
    }
    if map.contains_key(new_name) {
        anyhow::bail!("Project '{}' already exists", new_name);
    }

    let keys = map.remove(old_name).unwrap_or_default();
    map.insert(new_name.to_string(), keys);
    save_projects_map(proj_map_path, &map)?;
    Ok(())
}

pub fn delete_project(proj_map_path: &str, project: &str) -> Result<()> {
    let mut map = load_projects_map(proj_map_path)?;

    if !map.contains_key(project) {
        anyhow::bail!("Project '{}' does not exist", project);
    }

    map.remove(project);
    save_projects_map(proj_map_path, &map)?;
    Ok(())
}

pub fn refetch_metadata(all_bib_path: &str, key: &str) -> Result<()> {
    let content = fs::read_to_string(all_bib_path)?;
    let mut bib = Bibliography::parse(&content)?;

    let entry = bib.get(key)
        .ok_or_else(|| anyhow::anyhow!("Key '{}' not found", key))?
        .clone();

    // Get DOI from entry — bail early if none
    let doi = entry.get("doi")
        .map(|c| chunks_to_string(c))
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("No DOI found for '{}'", key))?;

    // Strip https://doi.org/ prefix if present
    let doi = doi
        .strip_prefix("https://doi.org/")
        .or_else(|| doi.strip_prefix("http://doi.org/"))
        .unwrap_or(&doi)
        .to_string();

    let client = Crossref::builder()
        .build()
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;

    let work = client
        .work(&doi)
        .map_err(|e| anyhow::anyhow!("Crossref error: {e:?}"))?;

    // Refetch only missing or empty fields
    let entry = bib.get_mut(key).unwrap();

    let is_empty = |e: &Entry, field: &str| {
        e.get(field)
            .map(|c| chunks_to_string(c).trim().is_empty())
            .unwrap_or(true)
    };

    if is_empty(entry, "abstract") {
        if let Some(abs) = &work.abstract_ {
            entry.set("abstract", field(abs));
        }
    }

    if is_empty(entry, "journal") {
        if let Some(journal) = work.container_title.as_ref().and_then(|v| v.get(0)) {
            entry.set("journal", field(journal));
        }
    }

    if is_empty(entry, "volume") {
        if let Some(vol) = &work.volume {
            entry.set("volume", field(vol));
        }
    }

    if is_empty(entry, "issue") {
        if let Some(issue) = &work.issue {
            entry.set("issue", field(issue));
        }
    }

    if is_empty(entry, "pages") {
        if let Some(pages) = &work.page {
            entry.set("pages", field(pages));
        }
    }

    if is_empty(entry, "issn") {
        if let Some(issn) = &work.issn {
            entry.set("issn", field(issn.join(", ")));
        }
    }

    if is_empty(entry, "publisher") && !work.publisher.is_empty() {
        entry.set("publisher", field(&work.publisher));
    }

    if is_empty(entry, "url") && !work.url.is_empty() {
        entry.set("url", field(&work.url));
    }

    fs::write(all_bib_path, bib.to_biblatex_string())?;
    Ok(())
}
