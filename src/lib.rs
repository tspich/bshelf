use serde::{Serialize, Deserialize};
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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Reference {
    pub title: String,
    pub authors: Vec<String>,
    pub year: Option<u32>,
    pub doi: Option<String>,
    pub abstract_text: Option<String>,
    pub journal: Option<String>,
    pub volume: Option<String>,
    pub number: Option<String>,
    pub pages: Option<String>,
    pub issn: Option<String>,
    //pub isbn: Option<String>,
    pub publisher: Option<String>,
    //pub pdf: Option<String>,  // relative path to pdf if available
}

fn field<S: AsRef<str>>(s: S) -> Vec<Spanned<Chunk>> {
    vec![Spanned::detached(Chunk::Normal(s.as_ref().to_string()))]
}

// Produce double braces {{...}} can be helpeful in some context like title?
// fn field<S: AsRef<str>>(s: S) -> Vec<Spanned<Chunk>> {
//     vec![Spanned::detached(Chunk::Verbatim(s.as_ref().to_string()))]
// }

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
                // (String::new(), f) => f.clone(),
                // (g, "") => g.clone(),
                // ("", "") => "<unknown>".to_string(),
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

// pub fn load_project(path: &str) -> Result<Vec<String>> {
//     if !std::path::Path::new(path).exists() {
//         return Ok(vec![]);
//     }
//     let content = fs::read_to_string(path)?;
//     Ok(serde_json::from_str(&content)?)
// }

pub fn load_projects_map(path: &str) -> Result<ProjectsMap> {
    if !std::path::Path::new(path).exists() {
        return Ok(HashMap::new());
    }
    let content = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&content)?)
}

// pub fn save_project(path: &str, keys: &[String]) -> Result<()> {
//     fs::write(path, serde_json::to_string_pretty(keys)?)?;
//     Ok(())
// }

pub fn save_projects_map(path: &str, map: &ProjectsMap) -> Result<()> {
    fs::write(path, serde_json::to_string_pretty(map)?)?;
    Ok(())
}

// pub fn add_to_project(proj_path: &str, key: &str) -> Result<()> {
//     let mut keys = load_project(proj_path)?;
//     if !keys.contains(&key.to_string()) {
//         keys.push(key.to_string());
//         save_project(pro, &keys)?;
//     }
//     Ok(())
// }

pub fn add_to_project(proj_map_path: &str, project: &str, key: &str) -> Result<()> {
    let mut map = load_projects_map(proj_map_path)?;
    let keys = map.entry(project.to_string()).or_default();
    if !keys.contains(&key.to_string()) {
        keys.push(key.to_string());
        save_projects_map(proj_map_path, &map)?;
    }
    Ok(())
}

// pub fn remove_from_project(proj_path: &str, key: &str) -> Result<()> {
//     let mut keys = load_project(proj_path)?;
//     keys.retain(|k| k != key);
//     save_project(proj_path, &keys)
// }

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
        // println!("⚠️ DOI already exists: {doi_url}");
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
    let key = format!(
        "{}_{}",
        authors
            .get(0)
            .and_then(|a| a.split_whitespace().last())
            .unwrap_or("ref")
            .to_lowercase(),
        year,
        //title
        //    .split_whitespace()
        //    .next()
        //    .unwrap_or("ref")
        //    .to_lowercase(),
    );


    // 5. Build BibTeX entry
    let mut entry = Entry::new(key.clone(), EntryType::Article);

    entry.set("title", field(title));
    entry.set("author", field(authors.join(" and ")));
    // entry.set("year", field(&year));
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

    //println!("✅ Added {doi} to project {project}");
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

// pub fn entry_to_reference(entry: &Entry) -> Reference {
//     let title = entry.get("title")
//         .map(|c| chunks_to_string(c))
//         .unwrap_or_default();
// 
//     let authors = entry.author().ok()
//         .map(|authors| authors.iter().map(|a| {
//             format!("{} {}", a.given_name, a.name).trim().to_string()
//         }).collect())
//         .unwrap_or_default();
// 
//     let year = entry.date().ok()
//         .and_then(|d| date_to_year_string(d))
//         .and_then(|s| s.parse().ok());
// 
//     let doi = entry.get("doi").map(|c| chunks_to_string(c));
//     let abstract_text = entry.get("abstract").map(|c| chunks_to_string(c));
//     let journal = entry.get("journal").map(|c| chunks_to_string(c));
//     let volume = entry.get("volume").map(|c| chunks_to_string(c));
//     let number = entry.get("issue").map(|c| chunks_to_string(c));
//     let pages = entry.get("pages").map(|c| chunks_to_string(c));
//     let issn = entry.get("issn").map(|c| chunks_to_string(c));
//     let publisher = entry.get("publisher").map(|c| chunks_to_string(c));
// 
//     Reference {
//         title,
//         authors,
//         year,
//         doi,
//         abstract_text,
//         journal,
//         volume,
//         number,
//         pages,
//         issn,
//         publisher,
//     }
// }

// pub fn load_or_create_config() -> Config {
//     let config_dir = dirs::config_dir().unwrap().join("refman");
//     let config_path = config_dir.join("config.toml");
// 
//     if !config_path.exists() {
//         std::fs::create_dir_all(&config_dir).unwrap();
// 
//         let default = r#"
// projects_dir = "~/refman/projects"
// pdfs_dir = "~/refman/pdfs"
// "#;
// 
//         std::fs::write(&config_path, default).unwrap();
// 
//         panic!("Config created at {:?}. Please edit it.", config_path);
//     }
// 
//     let contents = std::fs::read_to_string(config_path).unwrap();
//     toml::from_str(&contents).unwrap()
// }

