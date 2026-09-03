use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;


fn ignored(path: &Path) -> bool {
    path.components().any(|part| matches!(part.as_os_str().to_str(), Some(".git" | ".idea" | ".codex" | "target" | "node_modules")))
}

fn collect_files(root: &Path, output: &mut Vec<String>) -> io::Result<()> {
    if ignored(root) { return Ok(()); }
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if ignored(&path) { continue; }
        if path.is_dir() { collect_files(&path, output)?; }
        else if let Ok(relative) = path.strip_prefix(root) { output.push(relative.display().to_string()); }
    }
    Ok(())
}

fn project_context(root: &Path) -> String {
    let mut files = Vec::new();
    let _ = collect_files(root, &mut files);
    files.sort();
    files.truncate(200);
    format!("Project root: {}\nFiles:\n{}", root.display(), files.join("\n"))
}

fn safe_path(root: &Path, value: &str) -> Option<PathBuf> {
    let path = root.join(value).canonicalize().ok()?;
    path.starts_with(root).then_some(path)
}

fn confirm(command: &str) -> bool {
    print!("Execute `{command}`? [y/N] "); let _ = io::stdout().flush();
    let mut answer = String::new(); io::stdin().read_line(&mut answer).is_ok()
        && matches!(answer.trim(), "y" | "Y" | "yes")
}

fn main() {
    let root = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    println!("Javora 0.3.0 — {}", root.display());
    println!("Type /help for commands, /exit to quit.");
    let stdin = io::stdin();
    loop {
        print!("\n> ");
        let _ = io::stdout().flush();
        let mut input = String::new();
        if stdin.read_line(&mut input).is_err() { break; }
        let input = input.trim();
        match input {
            "/exit" | "/quit" => break,
            "/help" => println!("/help\n/files\n/read <path>\n/search <text>\n/run <command>\n/test\n/exit"),
            "/files" => println!("{}", project_context(&root)),
            _ if input.starts_with("/read ") => {
                let path = input.trim_start_matches("/read ").trim();
                match safe_path(&root, path).and_then(|p| fs::read_to_string(p).ok()) {
                    Some(contents) => println!("{contents}"), None => println!("File not found or outside project")
                }
            }
            _ if input.starts_with("/search ") => {
                let query = input.trim_start_matches("/search "); let mut found = Vec::new();
                let _ = collect_files(&root, &mut found);
                for file in found { if fs::read_to_string(root.join(&file)).map(|s| s.contains(query)).unwrap_or(false) { println!("{file}"); } }
            }
            _ if input.starts_with("/run ") => {
                let command = input.trim_start_matches("/run ");
                if confirm(command) { println!("Result: {:?}", Command::new("sh").arg("-c").arg(command).current_dir(&root).status()); }
            }
            "/test" => {
                let command = if root.join("pom.xml").exists() { "mvn test" }
                    else if root.join("build.gradle").exists() || root.join("build.gradle.kts").exists() { "gradle test" }
                    else { "no Maven or Gradle project detected" };
                println!("{}", command);
            }
            "" => {}
            _ => {
                let key = env::var("OPENAI_API_KEY").unwrap_or_default();
                if key.is_empty() { println!("OPENAI_API_KEY is not set."); continue; }
                let base = env::var("JAVORA_BASE_URL").unwrap_or_else(|_| "https://api.openai.com/v1".into());
                let model = env::var("JAVORA_MODEL").unwrap_or_else(|_| "gpt-4o-mini".into());
                let prompt = format!("{}\n\nUser task: {input}", project_context(&root));
                let prompt = prompt.replace('"', "\\\"").replace('\n', "\\n");
                let body = format!(r###"{{"model":"{}","messages":[{{"role":"user","content":"{}"}}]}}"###, model, prompt);
                match Command::new("curl").args(["-fsS", "-X", "POST", &format!("{}/chat/completions", base.trim_end_matches('/')), "-H", &format!("Authorization: Bearer {key}"), "-H", "Content-Type: application/json", "-d", &body]).output() {
                    Ok(output) if output.status.success() => println!("{}", String::from_utf8_lossy(&output.stdout)),
                    Ok(output) => println!("Model request failed: {}", String::from_utf8_lossy(&output.stderr)),
                    Err(error) => println!("Unable to start curl: {error}"),
                }
            }
        }
    }
}
