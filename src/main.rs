use std::collections::HashMap;
use std::fs;
use std::io::{self};
use std::process::Command;

use reqwest::Client;
use serde::Deserialize;
use structopt::StructOpt;
use tokio::runtime::Runtime;
use tokio::sync::mpsc;
use tokio::task;

#[derive(StructOpt)]
struct RuffFixer {
    #[structopt(help = "OpenAI API Key")]
    api_key: String,

    #[structopt(help = "Path to ruff tool")]
    ruff_path: String,

    #[structopt(help = "Root folder to run Ruff check on")]
    root_folder: String,
}

#[derive(Deserialize)]
struct Issue {
    filename: String,
    code: String,
    message: String,
    location: Location,
}

#[derive(Deserialize)]
struct Location {
    row: u32,
    column: u32,
}

impl RuffFixer {
    async fn run(&self) -> io::Result<()> {
        println!("Formatting code in {}...", self.root_folder);
        self.run_ruff_format(&self.ruff_path, &self.root_folder)?;

        println!("Running Ruff check on {}...", self.root_folder);
        let issues = match self.run_ruff_check(&self.ruff_path, &self.root_folder) {
            Ok(issues) => issues,
            Err(code) => {
                if code == 0 {
                    println!("All good");
                    return Ok(());
                } else {
                    return Err(io::Error::new(io::ErrorKind::Other, "Ruff check failed"));
                }
            }
        };

        // Group issues by file
        let issues_by_file = self.group_issues_by_file(issues);

        let client = Client::new();

        let (tx, mut rx) = mpsc::channel(10);
        for (filename, file_issues) in issues_by_file {
            let tx = tx.clone();
            let client = client.clone();
            let api_key = self.api_key.clone();

            task::spawn(async move {
                println!("Processing file: {}", filename);

                // Read the file content
                match fs::read_to_string(&filename) {
                    Ok(mut file_content) => {
                        for issue in file_issues {
                            println!("Fixing issue in {}: {}", filename, issue.message);

                            // Ask ChatGPT for a fix for the current issue
                            match RuffFixer::ask_chatgpt_for_fix(
                                &client,
                                &api_key,
                                &filename,
                                &issue,
                                &file_content,
                            )
                            .await
                            {
                                Ok(fixed_content) => {
                                    // Print diff and update file content
                                    RuffFixer::print_diff(&file_content, &fixed_content);
                                    file_content = fixed_content; // Update the file content with the fixed content
                                }
                                Err(err) => eprintln!("Error processing {}: {}", filename, err),
                            }
                        }

                        // After fixing all issues, write the final fixed content back to the file
                        if let Err(err) = fs::write(&filename, file_content) {
                            eprintln!("Error writing to {}: {}", filename, err);
                        } else {
                            println!("Fixed issues in {}", filename);
                        }
                    }
                    Err(err) => eprintln!("Error reading {}: {}", filename, err),
                }
                tx.send(()).await.unwrap();
            });
        }

        drop(tx);

        while let Some(_) = rx.recv().await {}

        Ok(())
    }

    fn run_ruff_format(&self, ruff_path: &str, folder: &str) -> io::Result<()> {
        let output = Command::new(ruff_path).args(&["format", folder]).output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("Ruff format failed: {}, {}", stderr, stdout),
            ));
        }

        Ok(())
    }

    fn run_ruff_check(&self, ruff_path: &str, folder: &str) -> Result<Vec<Issue>, i32> {
        let output = Command::new(ruff_path)
            .args(&["check", "--fix", folder, "--output-format", "json"])
            .output()
            .expect("Failed to execute Ruff check");

        let exit_code = output.status.code().unwrap_or(-1);

        if exit_code == 0 {
            return Err(0); // No issues found
        } else if exit_code == 1 {
            // Issues found and handled
            let data = String::from_utf8_lossy(&output.stdout);
            let issues: Vec<Issue> =
                serde_json::from_str(&data).expect("Failed to parse JSON output");
            return Ok(issues);
        } else {
            // Other non-zero exit codes indicate failure
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            eprintln!(
                "Ruff check failed with exit code {}: {}, {}",
                exit_code, stderr, stdout
            );
            return Err(exit_code);
        }
    }

    fn group_issues_by_file(&self, issues: Vec<Issue>) -> HashMap<String, Vec<Issue>> {
        let mut issues_by_file = HashMap::new();
        for issue in issues {
            issues_by_file
                .entry(issue.filename.clone())
                .or_insert_with(Vec::new)
                .push(issue);
        }
        issues_by_file
    }

    async fn ask_chatgpt_for_fix(
        client: &Client,
        api_key: &str,
        filename: &str,
        issue: &Issue,
        file_content: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let issue_row_content = file_content
            .lines()
            .nth(issue.location.row as usize - 1)
            .unwrap_or_default();
        let issue_message = format!("{}", issue.message);

        let prompt = format!(
            "Fix the following issue in the Python code:\n\nIssue description:\n{}\n\nProblematic line:\n{}\n\nHere's the current content of the file:\n\n{}\n\nPlease provide only the entire fixed content of the file addressing the issue listed above, do not provide any explanation, do not wrap the response with backticks.",
            issue_message, issue_row_content, file_content
        );

        let request_body = serde_json::json!({
            "model": "gpt-4o-mini",
            "messages": [
                {"role": "system", "content": "You are an automated bot that fixes Python code issues based on the provided issue report."},
                {"role": "user", "content": prompt}
            ]
        });

        let response = client
            .post("https://api.openai.com/v1/chat/completions")
            .bearer_auth(api_key)
            .json(&request_body)
            .send()
            .await?;

        let response_json: serde_json::Value = response.json().await?;
        let content = response_json["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| "Failed to parse response content")?;

        Ok(content.to_string())
    }

    fn print_diff(original: &str, fixed: &str) {
        let original_lines: Vec<&str> = original.lines().collect();
        let fixed_lines: Vec<&str> = fixed.lines().collect();

        println!("--- Original");
        println!("+++ Fixed");

        let max_len = std::cmp::max(original_lines.len(), fixed_lines.len());
        for i in 0..max_len {
            let original_line = original_lines.get(i).unwrap_or(&"");
            let fixed_line = fixed_lines.get(i).unwrap_or(&"");
            if original_line != fixed_line {
                if !original_line.is_empty() {
                    println!("- {}", original_line);
                }
                if !fixed_line.is_empty() {
                    println!("+ {}", fixed_line);
                }
            }
        }
    }
}

fn main() -> io::Result<()> {
    let fixer = RuffFixer::from_args();
    let rt = Runtime::new()?;
    rt.block_on(fixer.run())
}
