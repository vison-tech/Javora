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

enum Workflow {
    Idle,
    Clarifying { transcript: String },
    AwaitingApproval { requirement: String, plan: String },
    Approved { requirement: String },
}

struct Session {
    history: Vec<String>,
    last_command_result: Option<String>,
    workflow: Workflow,
}

impl Session {
    fn new() -> Self {
        Self {
            history: Vec::new(),
            last_command_result: None,
            workflow: Workflow::Idle,
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

fn confirmed(input: &str) -> bool {
    matches!(
        input.trim().to_lowercase().as_str(),
        "y" | "yes" | "确认" | "同意" | "approve"
    )
}

fn requirement_response(root: &Path, session: &Session, requirement: &str) -> Option<String> {
    let system = "You are Javora's requirements analyst. Do not inspect files or propose code changes. Ask exactly one focused question when certainty is below 95 percent. Return exactly one of: JAVORA_CLARIFY\nCONFIDENCE: 0-94\nQUESTION: one question; or JAVORA_REQUIREMENT_PLAN\nCONFIDENCE: 95-100\nPLAN: concise plan with scope, acceptance criteria, risks, and verification\nEND_JAVORA_REQUIREMENT_PLAN.";
    model_request(root, session, requirement, None, system, false)
}

fn parse_requirement(response: &str, transcript: &str) -> Result<Workflow, String> {
    if let Some(body) = response.strip_prefix("JAVORA_CLARIFY\n") {
        let confidence = body
            .lines()
            .find_map(|line| line.strip_prefix("CONFIDENCE: "))
            .and_then(|value| value.parse::<u8>().ok())
            .ok_or("Missing clarification confidence")?;
        let question = body
            .lines()
            .find_map(|line| line.strip_prefix("QUESTION: "))
            .ok_or("Missing clarification question")?;
        if confidence >= 95 {
            return Err("Clarification confidence must be below 95".into());
        }
        if question.trim().is_empty()
            || question.matches('?').count() > 1
            || question.matches('？').count() > 1
        {
            return Err("Clarification response must contain exactly one focused question".into());
        }
        return Ok(Workflow::Clarifying {
            transcript: transcript.to_owned(),
        });
    }
    let body = response
        .strip_prefix("JAVORA_REQUIREMENT_PLAN\n")
        .and_then(|text| text.strip_suffix("\nEND_JAVORA_REQUIREMENT_PLAN"))
        .ok_or("Model did not return a requirements response")?;
    let confidence = body
        .lines()
        .find_map(|line| line.strip_prefix("CONFIDENCE: "))
        .and_then(|value| value.parse::<u8>().ok())
        .ok_or("Missing plan confidence")?;
    let plan = body
        .lines()
        .skip_while(|line| !line.starts_with("PLAN: "))
        .map(|line| line.strip_prefix("PLAN: ").unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n");
    if confidence < 95 || plan.is_empty() {
        return Err("Plan requires 95+ confidence and a plan".into());
    }
    Ok(Workflow::AwaitingApproval {
        requirement: transcript.to_owned(),
        plan,
    })
}

fn question_from(response: &str) -> Option<&str> {
    response
        .lines()
        .find_map(|line| line.strip_prefix("QUESTION: "))
}

fn begin_or_continue_requirements(root: &Path, session: &mut Session, input: &str) {
    let transcript = match &session.workflow {
        Workflow::Idle => format!("User request: {input}"),
        Workflow::Clarifying { transcript } => format!("{transcript}\nUser answer: {input}"),
        Workflow::AwaitingApproval { requirement, plan } => {
            format!("{requirement}\nUser plan feedback: {input}\nPreviously proposed plan:\n{plan}")
        }
        Workflow::Approved { requirement } => requirement.clone(),
    };
    let Some(response) = requirement_response(root, session, &transcript) else {
        return;
    };
    session.remember(format!(
        "Requirement input: {input}\nRequirements analyst: {response}"
    ));
    match parse_requirement(&response, &transcript) {
        Ok(next @ Workflow::Clarifying { .. }) => {
            if let Some(question) = question_from(&response) {
                println!("{question}");
            }
            session.workflow = next;
        }
        Ok(Workflow::AwaitingApproval { requirement, plan }) => {
            println!("\n需求已澄清（置信度 ≥95%）。\n{plan}\n\n请回复“确认”批准方案，或直接说明需要调整的内容。");
            session.workflow = Workflow::AwaitingApproval { requirement, plan };
        }
        Ok(Workflow::Idle | Workflow::Approved { .. }) | Err(_) => {
            println!("无法可靠解析需求分析结果，请重新描述或输入 /new。");
        }
    }
}

fn execute_approved(root: &Path, session: &mut Session, request: &str) {
    let matches = find_design(root, request);
    let document = if matches.len() == 1 {
        let file = &matches[0];
        println!("Design document detected: {file}");
        document_text(root, file)
    } else {
        if matches.len() > 1 {
            println!(
                "Multiple design documents found; implementation will use project context only."
            );
        }
        None
    };
    let system = "You are Javora, a senior Java engineer. Work only within the approved requirement. Return exactly a JAVORA_PLAN containing SUMMARY, FILE blocks, and optional TEST lines, ending with END_JAVORA_PLAN. Do not expand scope.";
    if let Some(response) = model_request(root, session, request, document.as_deref(), system, true)
    {
        match parse_plan(&response) {
            Ok(plan) if show_plan(root, &plan) => match apply_plan(root, &plan) {
                Ok(()) => {
                    println!("Changes applied.");
                    run_suggested_tests(root, session, &plan.tests);
                }
                Err(error) => println!("Apply failed: {error}"),
            },
            Ok(_) => println!("Changes discarded."),
            Err(error) => println!("Invalid change plan: {error}"),
        }
    }
    session.workflow = Workflow::Idle;
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
    system: &str,
    include_context: bool,
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
    let context = if include_context {
        project_context(root)
    } else {
        String::new()
    };
    let history = if include_context {
        session.context()
    } else {
        String::new()
    };
    let prompt = format!("{context}{document}{history}\n\nUser task: {input}");
    let prompt = prompt
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t");
    let system = system
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t");
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
            let mut closed = false;
            for content_line in lines.by_ref() {
                if content_line == "```" {
                    closed = true;
                    break;
                }
                content.push_str(content_line);
                content.push('\n');
            }
            if !closed {
                return Err(format!("Missing closing content block for {path}"));
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
    let parent = path.parent()?;
    let existing_parent = parent.canonicalize().ok().or_else(|| {
        let mut candidate = parent.to_path_buf();
        while !candidate.exists() {
            candidate = candidate.parent()?.to_path_buf();
        }
        candidate.canonicalize().ok()
    })?;
    existing_parent.strip_prefix(&root).ok()?;
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
            "/help" => println!("/help\n/new\n/files\n/design\n/read <path>\n/search <text>\n/run <command>\n/test\n/implement <request>\n/exit\n\nNatural-language requests always go through clarification first."),
            "/new" => { session.workflow = Workflow::Idle; println!("Current requirement discarded."); }
            "/files" | "/design" | "/test" if !matches!(session.workflow, Workflow::Approved { .. }) => {
                println!("请先完成需求澄清并明确批准方案后再执行此命令。输入 /new 可重新开始。");
            }
            "/files" => println!("{}", project_context(&root)),
            "/design" => { let docs=design_documents(&root); if docs.is_empty(){println!("No design documents detected.")} else { for d in docs {println!("{d}")} } }
            _ if input.starts_with("/read ") => {
                if !matches!(session.workflow, Workflow::Approved { .. }) { println!("请先完成需求澄清并明确批准方案后再读取项目文件。"); continue; }
                let path = input.trim_start_matches("/read ").trim();
                match safe_path(&root, path).and_then(|p| fs::read_to_string(p).ok()) {
                    Some(contents) => println!("{contents}"), None => println!("File not found or outside project")
                }
            }
            _ if input.starts_with("/search ") => {
                if !matches!(session.workflow, Workflow::Approved { .. }) { println!("请先完成需求澄清并明确批准方案后再搜索项目。"); continue; }
                let query = input.trim_start_matches("/search "); let mut found = Vec::new();
                let _ = collect_files(&root, &mut found);
                for file in found { if fs::read_to_string(root.join(&file)).map(|s| s.contains(query)).unwrap_or(false) { println!("{file}"); } }
            }
            _ if input.starts_with("/run ") => {
                if !matches!(session.workflow, Workflow::Approved { .. }) { println!("请先完成需求澄清并明确批准方案后再运行命令。"); continue; }
                let command = input.trim_start_matches("/run ");
                if let Some(result) = run_command(&root, command) { session.last_command_result = Some(result); }
            }
            _ if input.starts_with("/implement ") || (input.contains("根据") && (input.contains("文档") || input.contains("设计"))) => {
                if let Workflow::Approved { requirement } = &session.workflow {
                    let requirement = requirement.clone();
                    execute_approved(&root, &mut session, &format!("{requirement}\nAdditional request: {input}"));
                } else {
                    begin_or_continue_requirements(&root, &mut session, input);
                }
            }
            "/test" => {
                match test_command(&root) {
                    Some(command) => { if let Some(result) = run_command(&root, command) { session.last_command_result = Some(result); } },
                    None => println!("No Maven or Gradle project detected"),
                }
            }
            "" => {}
            _ => {
                match &session.workflow {
                    Workflow::AwaitingApproval { requirement, .. } if confirmed(input) => {
                        let requirement = requirement.clone();
                        session.workflow = Workflow::Approved { requirement: requirement.clone() };
                        execute_approved(&root, &mut session, &requirement);
                    }
                    Workflow::Approved { requirement } => {
                        let requirement = requirement.clone();
                        execute_approved(&root, &mut session, &format!("{requirement}\nAdditional request: {input}"));
                    }
                    _ => begin_or_continue_requirements(&root, &mut session, input),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn low_confidence_response_keeps_the_workflow_clarifying() {
        let response =
            "JAVORA_CLARIFY\nCONFIDENCE: 72\nQUESTION: Which database should this feature use?";
        let workflow = parse_requirement(response, "User request: add a feature").unwrap();
        assert!(matches!(workflow, Workflow::Clarifying { .. }));
    }

    #[test]
    fn high_confidence_response_requires_plan_approval() {
        let response = "JAVORA_REQUIREMENT_PLAN\nCONFIDENCE: 95\nPLAN: Goal: add a health endpoint. Scope: one controller. Acceptance: returns 200. Verification: Maven test. Risks: endpoint exposure.\nEND_JAVORA_REQUIREMENT_PLAN";
        let workflow = parse_requirement(response, "User request: add a health endpoint").unwrap();
        assert!(matches!(workflow, Workflow::AwaitingApproval { .. }));
    }

    #[test]
    fn unclosed_change_content_block_is_rejected() {
        let response = "JAVORA_PLAN\nSUMMARY: add endpoint\nFILE: src/Main.java\n```\nclass Main {}\nEND_JAVORA_PLAN";
        assert!(parse_plan(response).is_err());
    }
}
