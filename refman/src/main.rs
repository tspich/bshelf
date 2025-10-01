use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Serialize, Deserialize, Debug)]
struct Reference {
    id: String,
    title: String,
    authors: String,
    year: u32,
    doi: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
struct Project {
    project: String,
    references: Vec<Reference>,
}


#[derive(Parser)]
#[command(name = "refman", version = "0.1", about = "Minimal reference manager")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new project
    New { name: String },

    /// Add a reference by DOI
    Add { project: String, doi: String },

    /// Export project references as BibTeX
    Export { project: String },
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::New { name } => {
            let projects_dir = Path::new("projects");
            if !projects_dir.exists() {
                fs::create_dir_all(projects_dir).expect("Failed to create projects folder");
            }

            let file_path = projects_dir.join(format!("{}.json", name));

            if file_path.exists() {
                eprintln!("Project '{}' already exists!", name);
            } else {
                let project = Project {
                    project: name.clone(),
                    references: Vec::new(),
                };
                let json = serde_json::to_string_pretty(&project).unwrap();
                fs::write(&file_path, json).expect("Failed to write project file");

                println!("Created project '{}'", name);
            }
        }

        Commands::Add { project, doi } => {
            println!("Adding DOI {} to project {}", doi, project);
            // TODO: fetch metadata + append to JSON
        }
        Commands::Export { project } => {
            println!("Exporting BibTeX for project {}", project);
            // TODO: read JSON + export .bib
        }
    }
}
