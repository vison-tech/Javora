use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

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

fn main() {
    let root = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    println!("Javora 0.2.0 — Rust core");
    println!("{}", project_context(&root));
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
            "/help" => println!("/help  show help\n/exit  quit\n/test  detect the project test command"),
            "/test" => {
                let command = if root.join("pom.xml").exists() { "mvn test" }
                    else if root.join("build.gradle").exists() || root.join("build.gradle.kts").exists() { "gradle test" }
                    else { "no Maven or Gradle project detected" };
                println!("{}", command);
            }
            "" => {}
            _ => println!("Task received: {}\nAgent model execution will be added in the next iteration.", input),
        }
    }
}
