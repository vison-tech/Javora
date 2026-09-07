use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const MAX_FILE_BYTES: usize = 512 * 1024;
const DANGEROUS_COMMANDS: [&str; 8] = [
    "rm -rf",
    "git reset --hard",
    "git clean -f",
    "mkfs",
    "dd if=",
    "> /dev/",
    "shutdown",
    "reboot",
];

struct FileChange {
    path: String,
    content: String,
}
struct ChangePlan {
    summary: String,
    changes: Vec<FileChange>,
    tests: Vec<String>,
}

struct Session {
    history: Vec<String>,
    last_command_result: Option<String>,
}

impl Session {
    fn new() -> Self {
        Self {
            history: Vec::new(),
            last_command_result: None,
        }
    }

    fn remember(&mut self, entry: String) {
        self.history.push(entry);
        if self.history.len() > 8 {
            self.history.remove(0);
        }
    }

    fn context(&self) -> String {
        let history = self.history.join("\n\n");
        let result = self.last_command_result.as_deref().unwrap_or("");
        format!("\n\nRecent conversation:\n{history}\n\nLatest command result:\n{result}")
    }
}

fn ignored(path: &Path) -> bool {
    path.components().any(|part| {
        matches!(
            part.as_os_str().to_str(),
            Some(".git" | ".idea" | ".codex" | "target" | "node_modules")
        )
    })
}

fn collect_files(root: &Path, output: &mut Vec<String>) -> io::Result<()> {
    if ignored(root) {
        return Ok(());
    }
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if ignored(&path) {
            continue;
        }
        if path.is_dir() {
            collect_files(&path, output)?;
        } else if let Ok(relative) = path.strip_prefix(root) {
            output.push(relative.display().to_string());
        }
    }
    Ok(())
}

fn project_context(root: &Path) -> String {
    let mut files = Vec::new();
    let _ = collect_files(root, &mut files);
    files.sort();
    files.truncate(200);
    format!(
        "Project root: {}\nFiles:\n{}",
        root.display(),
        files.join("\n")
    )
}

fn design_documents(root: &Path) -> Vec<String> {
    let mut all = Vec::new();
    let _ = collect_files(root, &mut all);
    all.into_iter()
        .filter(|f| {
            let lower = f.to_lowercase();
            let supported = [".md", ".txt", ".yaml", ".yml", ".json", ".xml"]
                .iter()
                .any(|x| lower.ends_with(x));
            supported
                && (lower.starts_with("docs/")
                    || [
                        "design",
                        "spec",
                        "architecture",
                        "requirements",
                        "架构",
                        "需求",
                        "方案",
                        "说明",
                    ]
                    .iter()
                    .any(|x| lower.contains(x)))
        })
        .collect()
}

fn find_design(root: &Path, request: &str) -> Vec<String> {
    let docs = design_documents(root);
    let lower = request.to_lowercase();
    docs.into_iter()
        .filter(|f| {
            lower.contains(&f.to_lowercase())
                || f.split('/')
                    .next_back()
                    .map(|n| lower.contains(&n.to_lowercase()))
                    .unwrap_or(false)
        })
        .collect()
}

fn document_text(root: &Path, file: &str) -> Option<String> {
    safe_path(root, file)
        .and_then(|path| fs::read_to_string(path).ok())
        .filter(|text| text.len() <= MAX_FILE_BYTES)
}

fn safe_path(root: &Path, value: &str) -> Option<PathBuf> {
    let root = root.canonicalize().ok()?;
    let path = root.join(value).canonicalize().ok()?;
    if path.strip_prefix(&root).is_ok() {
        Some(path)
    } else {
        None
    }
}

fn model_request(
    root: &Path,
    session: &Session,
    input: &str,
    document: Option<&str>,
) -> Option<String> {
    let key = env::var("OPENAI_API_KEY").unwrap_or_default();
    if key.is_empty() {
        println!("OPENAI_API_KEY is not set.");
        return None;
    }
    let base = env::var("JAVORA_BASE_URL").unwrap_or_else(|_| "https://api.openai.com/v1".into());
    let model = env::var("JAVORA_MODEL").unwrap_or_else(|_| "gpt-4o-mini".into());
    let document = document
        .map(|d| format!("\n\nDesign document:\n{d}"))
        .unwrap_or_default();
    let prompt = format!(
        "{}{}{}\n\nUser task: {input}",
        project_context(root),
        document,
        session.context(),
    );
    let prompt = prompt
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t");
    let system = "You are Javora, a senior Java engineer. Return a concise implementation plan before code changes.";
    let body = format!(
        r#"{{"model":"{}","messages":[{{"role":"system","content":"{}"}},{{"role":"user","content":"{}"}}]}}"#,
        model, system, prompt
    );
    match Command::new("curl")
        .args([
            "-fsS",
            "-X",
            "POST",
            &format!("{}/chat/completions", base.trim_end_matches('/')),
            "-H",
            &format!("Authorization: Bearer {key}"),
            "-H",
            "Content-Type: application/json",
            "-d",
            &body,
        ])
        .output()
    {
        Ok(output) if output.status.success() => {
            Some(model_content(&String::from_utf8_lossy(&output.stdout)))
        }
        Ok(output) => {
            println!(
                "Model request failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            None
        }
        Err(error) => {
            println!("Unable to start curl: {error}");
            None
        }
    }
}

fn model_content(response: &str) -> String {
    let marker = "\"content\":\"";
    let Some(start) = response.find(marker).map(|i| i + marker.len()) else {
        return response.to_owned();
    };
    let mut content = String::new();
    let mut escaped = false;
    for ch in response[start..].chars() {
        if escaped {
            content.push(match ch {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                '\\' => '\\',
                '"' => '"',
                other => other,
            });
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == '"' {
            break;
        } else {
            content.push(ch);
        }
    }
    content
}

fn test_command(root: &Path) -> Option<&'static str> {
    if root.join("mvnw").exists() {
        Some("./mvnw test")
    } else if root.join("pom.xml").exists() {
        Some("mvn test")
    } else if root.join("gradlew").exists() {
        Some("./gradlew test")
    } else if root.join("build.gradle").exists() || root.join("build.gradle.kts").exists() {
        Some("gradle test")
    } else {
        None
    }
}

fn parse_plan(response: &str) -> Result<ChangePlan, String> {
    let body = response
        .strip_prefix("JAVORA_PLAN\n")
        .and_then(|text| text.strip_suffix("\nEND_JAVORA_PLAN"))
        .ok_or("Model did not return a Javora change plan")?;
    let mut summary = String::new();
    let mut changes = Vec::new();
    let mut tests = Vec::new();
    let mut lines = body.lines().peekable();
    while let Some(line) = lines.next() {
        if let Some(value) = line.strip_prefix("SUMMARY: ") {
            summary = value.to_owned();
        } else if let Some(value) = line.strip_prefix("TEST: ") {
            tests.push(value.to_owned());
        } else if let Some(path) = line.strip_prefix("FILE: ") {
            if lines.next() != Some("```") {
                return Err(format!("Missing content block for {path}"));
            }
            let mut content = String::new();
            for content_line in lines.by_ref() {
                if content_line == "```" {
                    break;
                }
                content.push_str(content_line);
                content.push('\n');
            }
            if content.len() > MAX_FILE_BYTES {
                return Err(format!("Generated content for {path} is too large"));
            }
            changes.push(FileChange {
                path: path.to_owned(),
                content,
            });
        }
    }
    if summary.is_empty() || changes.is_empty() {
        Err("Plan requires SUMMARY and at least one FILE".into())
    } else {
        Ok(ChangePlan {
            summary,
            changes,
            tests,
        })
    }
}

fn write_path(root: &Path, value: &str) -> Option<PathBuf> {
    let root = root.canonicalize().ok()?;
    let path = root.join(value);
    let parent = path.parent()?.canonicalize().ok()?;
    parent.strip_prefix(&root).ok()?;
    Some(path)
}

fn show_plan(root: &Path, plan: &ChangePlan) -> bool {
    println!("\nPlan: {}", plan.summary);
    for change in &plan.changes {
        let old = safe_path(root, &change.path)
            .and_then(|path| fs::read_to_string(path).ok())
            .unwrap_or_default();
        println!(
            "\n--- {}\n+++ {}",
            if old.is_empty() {
                "/dev/null"
            } else {
                &change.path
            },
            change.path
        );
        for line in old.lines() {
            println!("-{line}");
        }
        for line in change.content.lines() {
            println!("+{line}");
        }
    }
    if !plan.tests.is_empty() {
        println!("\nSuggested tests:");
        for test in &plan.tests {
            println!("  {test}");
        }
    }
    confirm("Apply the displayed file changes")
}

fn apply_plan(root: &Path, plan: &ChangePlan) -> Result<(), String> {
    let mut targets = Vec::new();
    for change in &plan.changes {
        let path = write_path(root, &change.path)
            .ok_or_else(|| format!("Unsafe output path: {}", change.path))?;
        targets.push((path, &change.content));
    }
    for (path, content) in targets {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        fs::write(path, content).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn command_allowed(command: &str) -> bool {
    let normalized = command.to_lowercase();
    !DANGEROUS_COMMANDS
        .iter()
        .any(|pattern| normalized.contains(pattern))
}

fn run_command(root: &Path, command: &str) -> Option<String> {
    if !command_allowed(command) {
        println!("Blocked dangerous command. Run it manually if you intend to proceed.");
        return None;
    }
    if !confirm(command) {
        println!("Command skipped.");
        return None;
    }
    match Command::new("sh")
        .arg("-c")
        .arg(command)
        .current_dir(root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(mut child) => {
            let deadline = Instant::now() + Duration::from_secs(120);
            while Instant::now() < deadline && child.try_wait().ok().flatten().is_none() {
                std::thread::sleep(Duration::from_millis(100));
            }
            if child.try_wait().ok().flatten().is_none() {
                let _ = child.kill();
                println!("Command timed out after 120 seconds.");
                return Some(format!("{command}\nTimed out after 120 seconds."));
            }
            let output = match child.wait_with_output() {
                Ok(output) => output,
                Err(error) => {
                    println!("Command failed: {error}");
                    return Some(format!("{command}\nFailed: {error}"));
                }
            };
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let result = format!(
                "{command}\nExit status: {}\nstdout:\n{}\nstderr:\n{}",
                output.status,
                truncate(&stdout, 12_000),
                truncate(&stderr, 12_000)
            );
            println!("Result: {}", output.status);
            if !stdout.is_empty() {
                print!("{stdout}");
            }
            if !stderr.is_empty() {
                eprint!("{stderr}");
            }
            Some(result)
        }
        Err(error) => {
            println!("Unable to start command: {error}");
            None
        }
    }
}

fn truncate(value: &str, limit: usize) -> String {
    if value.len() <= limit {
        value.to_owned()
    } else {
        format!("{}\n... output truncated ...", &value[..limit])
    }
}

fn run_suggested_tests(root: &Path, session: &mut Session, tests: &[String]) {
    for test in tests {
        if let Some(result) = run_command(root, test) {
            session.last_command_result = Some(result);
        }
    }
}

fn confirm(command: &str) -> bool {
    print!("Execute `{command}`? [y/N] ");
    let _ = io::stdout().flush();
    let mut answer = String::new();
    io::stdin().read_line(&mut answer).is_ok() && matches!(answer.trim(), "y" | "Y" | "yes")
}

fn main() {
    let root = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    println!("Javora 0.3.0 — {}", root.display());
    println!("Type /help for commands, /exit to quit.");
    let stdin = io::stdin();
    let mut session = Session::new();
    loop {
        print!("\n> ");
        let _ = io::stdout().flush();
        let mut input = String::new();
        if stdin.read_line(&mut input).is_err() {
            break;
        }
        let input = input.trim();
        match input {
            "/exit" | "/quit" => break,
            "/help" => println!("/help\n/files\n/design\n/read <path>\n/search <text>\n/run <command>\n/test\n/implement <request>\n/exit"),
            "/files" => println!("{}", project_context(&root)),
            "/design" => { let docs=design_documents(&root); if docs.is_empty(){println!("No design documents detected.")} else { for d in docs {println!("{d}")} } }
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
                if let Some(result) = run_command(&root, command) { session.last_command_result = Some(result); }
            }
            _ if input.starts_with("/implement ") || (input.contains("根据") && (input.contains("文档") || input.contains("设计"))) => {
                let request = input.strip_prefix("/implement ").unwrap_or(input);
                let matches=find_design(&root,request);
                if matches.len()==1 { let d=&matches[0]; println!("Design document detected: {d}"); if let Some(doc)=document_text(&root,d) { println!("Design document loaded ({} bytes).",doc.len()); if confirm(&format!("Generate an implementation plan from `{d}`")){ if let Some(response)=model_request(&root, &session, request, Some(&doc)) { match parse_plan(&response) { Ok(plan) if show_plan(&root,&plan) => match apply_plan(&root,&plan) { Ok(())=>{ println!("Changes applied."); run_suggested_tests(&root, &mut session, &plan.tests); }, Err(error)=>println!("Apply failed: {error}") }, Ok(_) => println!("Changes discarded."), Err(error)=>println!("Invalid change plan: {error}"), } } } } }
                else if matches.is_empty(){println!("No matching design document found. Use /design to list candidates.")} else {println!("Multiple design documents found:"); for d in matches {println!("- {d}")} }
            }
            "/test" => {
                match test_command(&root) {
                    Some(command) => { if let Some(result) = run_command(&root, command) { session.last_command_result = Some(result); } },
                    None => println!("No Maven or Gradle project detected"),
                }
            }
            "" => {}
            _ => {
                if let Some(answer) = model_request(&root, &session, input, None) { println!("{answer}"); session.remember(format!("User: {input}\nAssistant: {answer}")); }
            }
        }
    }
}
