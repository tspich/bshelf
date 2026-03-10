use serde::{Serialize, Deserialize};
// use serde_json;
use anyhow::Result;
// use std::fmt::format;
use std::fs;
use std::path::{Path, PathBuf};
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

pub fn add_reference(proj_file: &str, doi: &str) -> Result<()> {
    // let proj_file = Path::new("projects").join(format!("{project}.bib"));

    // fs::create_dir_all("projects")?;


    // 1. Load bibliography
    // let mut bib = load_bib(&proj_file)?;
    let content = fs::read_to_string(&proj_file)?;
    let mut bib = Bibliography::parse(&content)?;

    let doi_url = format!("https://doi.org/{doi}");

    // 2. Duplicate check
    if bib.iter().any(|e| {
        e.get("doi")
            .map(|chunks|{
                chunks.iter().any(|c| {
                   c.v.get() == doi_url 
                })
            }) 
            .unwrap_or(false)
    }){
        println!("⚠️ DOI already exists: {doi_url}");
        return Ok(());
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
    let mut entry = Entry::new(key, EntryType::Article);

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
    //save_bib(&proj_file, &bib)?;
    // fs::write(&proj_file, bib.to_bibtex_string())?;
    fs::write(&proj_file, bib.to_biblatex_string())?;

    //println!("✅ Added {doi} to project {project}");
    Ok(())
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
    pub projects_dir: PathBuf,
    pub pdfs_dir: PathBuf,
}


pub fn load_config() -> Config {
    let config_dir = dirs::config_dir()
        .expect("Could not find config directory")
        .join("refman");

    let config_path = config_dir.join("config.toml");

    let contents = fs::read_to_string(&config_path)
        .expect("Could not read config file");

    toml::from_str(&contents)
        .expect("Invalid config file")
}

