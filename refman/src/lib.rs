use serde::{Serialize, Deserialize};
use serde_json;
use anyhow::Result;
use std::fs;
use std::path::Path;
use reqwest::blocking;
use serde_json::Value;

#[derive(Serialize, Deserialize)]
pub struct Reference {
    pub title: String,
    pub authors: Vec<String>,
    pub year: Option<u32>,
    pub doi: Option<String>,
    pub pdf: Option<String>,  // relative path to pdf if available
}

/// Create a new project JSON file (empty list).
pub fn new_project(project: &str) -> Result<()> {
    fs::create_dir_all("projects")?;
    let proj_file = format!("projects/{}.json", project);
    if !Path::new(&proj_file).exists() {
        fs::write(&proj_file, "[]")?;
        println!("Created new project: {}", project);
    } else {
        println!("Project {} already exists", project);
    }
    Ok(())
}

/// Add a reference from a DOI (fetch Crossref + Unpaywall).
pub fn add_reference(project: &str, doi: &str) -> Result<()> {
    // 1. Crossref metadata
    let crossref_url = format!("https://api.crossref.org/works/{}", doi);
    let crossref_resp: Value = blocking::get(&crossref_url)?.json()?;
    let item = &crossref_resp["message"];

    let title = item["title"][0].as_str().unwrap_or("").to_string();
    let authors: Vec<String> = item["author"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .map(|a| {
            format!(
                "{} {}",
                a["given"].as_str().unwrap_or(""),
                a["family"].as_str().unwrap_or("")
            )
            .trim()
            .to_string()
        })
        .collect();
    let year = item["issued"]["date-parts"][0][0].as_u64().map(|y| y as u32);

    // 2. Unpaywall for PDF
    let unpaywall_url = format!(
        "https://api.unpaywall.org/v2/{}?email={}",
        doi, "your@email.com"
    );
    let unpaywall_resp: Value = blocking::get(&unpaywall_url)?.json()?;
    let pdf_url = unpaywall_resp["best_oa_location"]["url_for_pdf"]
        .as_str()
        .map(|s| s.to_string());

    // 3. If available, download PDF into pdfs/
    //let pdf_path = if let Some(url) = pdf_url {
    //    fs::create_dir_all("pdfs")?;
    //    let filename = doi.replace("/", "_") + ".pdf";
    //    let full_path = format!("pdfs/{}", filename);

    //    let mut resp = blocking::get(&url)?;
    //    let mut file = fs::File::create(&full_path)?;
    //    std::io::copy(&mut resp, &mut file)?;

    //    Some(full_path)
    //} else {
    //    None
    //};
    let pdf_path = if let Some(url) = pdf_url {
        fs::create_dir_all("pdfs")?;
        let filename = doi.replace("/", "_") + ".pdf";
        let full_path = format!("pdfs/{}", filename);
    
        let resp = blocking::get(&url)?;
        if let Some(ct) = resp.headers().get("content-type") {
            if ct.to_str().unwrap_or("").contains("pdf") {
                let mut file = fs::File::create(&full_path)?;
                let content = resp.bytes()?;
                std::io::copy(&mut content.as_ref(), &mut file)?;
                Some(full_path)
            } else {
                println!("⚠️ Not a PDF: {}", url);
                None
            }
        } else {
            None
        }
    } else {
        None
    };


    // 4. Build Reference
    let reference = Reference {
        title,
        authors,
        year,
        doi: Some(doi.to_string()),
        pdf: pdf_path,
    };

    // 5. Append to project JSON
    fs::create_dir_all("projects")?;
    let proj_file = format!("projects/{}.json", project);
    let mut refs: Vec<Reference> = if let Ok(data) = fs::read_to_string(&proj_file) {
        match serde_json::from_str(&data) {
            Ok(parsed) => parsed,
            Err(_) => vec![], // fallback if file is `{}` or corrupted
        }
    } else {
        vec![]
    };
    refs.push(reference);
    let serialized = serde_json::to_string_pretty(&refs)?;
    fs::write(&proj_file, serialized)?;

    println!("Added reference {} to project {}", doi, project);
    Ok(())
}
