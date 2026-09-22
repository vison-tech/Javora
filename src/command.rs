use super::*;

pub(crate) enum AllowedCommand {
    Maven {
        program: String,
        subcommand: String,
        args: Vec<String>,
    },
    Gradle {
        program: String,
        task: String,
        args: Vec<String>,
    },
    GitRead {
        subcommand: String,
        args: Vec<String>,
    },
    FileRead {
        command: String,
        args: Vec<String>,
    },
    JavaTool {
        tool: String,
        args: Vec<String>,
    },
}

impl fmt::Display for AllowedCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Maven {
                program,
                subcommand,
                args,
            } => {
                write!(formatter, "{program} {subcommand}")?;
                for arg in args {
                    write!(formatter, " {arg}")?;
                }
            }
            Self::Gradle {
                program,
                task,
                args,
            } => {
                write!(formatter, "{program} {task}")?;
                for arg in args {
                    write!(formatter, " {arg}")?;
                }
            }
            Self::GitRead { subcommand, args } => {
                write!(formatter, "git {subcommand}")?;
                for arg in args {
                    write!(formatter, " {arg}")?;
                }
            }
            Self::FileRead { command, args }
            | Self::JavaTool {
                tool: command,
                args,
            } => {
                write!(formatter, "{command}")?;
                for arg in args {
                    write!(formatter, " {arg}")?;
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub(crate) enum CommandValidationError {
    #[error("empty command")]
    EmptyCommand,
    #[error("shell metacharacters not allowed in {location}: {value}")]
    ShellMetacharacters {
        location: &'static str,
        value: String,
    },
    #[error("paths outside the project are not allowed in command arguments: {value}")]
    OutsideProjectPath { value: String },
    #[error("{tool} requires a subcommand or task")]
    MissingSubcommand { tool: &'static str },
    #[error("{tool} value '{value}' is not allowed")]
    UnsupportedSubcommand { tool: &'static str, value: String },
    #[error("Git branch arguments are blocked because they can modify refs")]
    GitBranchArguments,
    #[error("Git argument '{0}' is blocked because it can write files or execute helpers")]
    BlockedGitArgument(String),
    #[error("Find action '{0}' is blocked because it can modify files or execute commands")]
    BlockedFindAction(String),
    #[error("{tool} is restricted to -version/--version; use Maven or Gradle for approved builds and tests")]
    JavaToolRestricted { tool: String },
    #[error("javap -J options are blocked because they can execute JVM agents")]
    JavapJvmOption,
    #[error("command '{0}' is not in the whitelist")]
    UnknownCommand(String),
}

const ALLOWED_MAVEN_SUBCOMMANDS: &[&str] =
    &["clean", "compile", "test", "package", "install", "verify"];
const ALLOWED_GRADLE_TASKS: &[&str] = &["build", "test", "assemble", "check", "clean"];
const ALLOWED_GIT_SUBCOMMANDS: &[&str] = &["status", "log", "diff", "branch", "show"];
const ALLOWED_FILE_COMMANDS: &[&str] = &["ls", "cat", "find", "grep", "tree", "head", "tail"];

fn has_shell_metacharacters(s: &str) -> bool {
    // Detect shell injection attempts
    s.contains('|')
        || s.contains(';')
        || s.contains('&')
        || s.contains('$')
        || s.contains('`')
        || s.contains('>')
        || s.contains('<')
        || s.contains('*')
        || s.contains('?')
        || s.contains('[')
        || s.contains(']')
        || s.contains('(')
        || s.contains(')')
        || s.contains('{')
        || s.contains('}')
        || s.contains('\\')
        || s.contains('"')
        || s.contains('\'')
}

fn has_outside_project_path(arg: &str) -> bool {
    use std::path::Component;

    let candidate = arg.split_once('=').map_or(arg, |(_, value)| value);
    let path = Path::new(candidate);
    path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
        || (candidate.len() >= 3
            && candidate.as_bytes()[0].is_ascii_alphabetic()
            && candidate.as_bytes()[1] == b':'
            && matches!(candidate.as_bytes()[2], b'/' | b'\\'))
}

fn validate_plain_args(args: &[String]) -> std::result::Result<(), CommandValidationError> {
    for arg in args {
        if has_shell_metacharacters(arg) {
            return Err(CommandValidationError::ShellMetacharacters {
                location: "arguments",
                value: arg.clone(),
            });
        }
        if has_outside_project_path(arg) {
            return Err(CommandValidationError::OutsideProjectPath { value: arg.clone() });
        }
    }
    Ok(())
}

fn validate_git_args(
    subcommand: &str,
    args: &[String],
) -> std::result::Result<(), CommandValidationError> {
    validate_plain_args(args)?;

    if subcommand == "branch" && !args.is_empty() {
        return Err(CommandValidationError::GitBranchArguments);
    }

    const BLOCKED_GIT_ARGS: &[&str] = &["--output", "--ext-diff", "--textconv"];
    if let Some(arg) = args
        .iter()
        .find(|arg| BLOCKED_GIT_ARGS.contains(&arg.as_str()) || arg.starts_with("--output="))
    {
        return Err(CommandValidationError::BlockedGitArgument(arg.clone()));
    }

    Ok(())
}

fn validate_file_args(
    command: &str,
    args: &[String],
) -> std::result::Result<(), CommandValidationError> {
    validate_plain_args(args)?;

    if command == "find" {
        const BLOCKED_FIND_ACTIONS: &[&str] = &[
            "-delete", "-exec", "-execdir", "-ok", "-okdir", "-fls", "-fprint", "-fprint0",
            "-fprintf",
        ];
        if let Some(arg) = args
            .iter()
            .find(|arg| BLOCKED_FIND_ACTIONS.contains(&arg.as_str()))
        {
            return Err(CommandValidationError::BlockedFindAction(arg.clone()));
        }
    }

    Ok(())
}

pub(crate) fn parse_and_validate_command(
    command: &str,
) -> std::result::Result<AllowedCommand, CommandValidationError> {
    let command = command.trim();
    if command.is_empty() {
        return Err(CommandValidationError::EmptyCommand);
    }

    // Split on whitespace to get command parts
    let parts: Vec<&str> = command.split_whitespace().collect();
    if parts.is_empty() {
        return Err(CommandValidationError::EmptyCommand);
    }

    let cmd = parts[0];
    let args: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();

    // Check for shell metacharacters in the base command
    if has_shell_metacharacters(cmd) {
        return Err(CommandValidationError::ShellMetacharacters {
            location: "command",
            value: cmd.to_owned(),
        });
    }

    // Maven
    if cmd == "mvn" || cmd == "./mvnw" {
        if args.is_empty() {
            return Err(CommandValidationError::MissingSubcommand { tool: "Maven" });
        }
        let subcommand = &args[0];
        if !ALLOWED_MAVEN_SUBCOMMANDS.contains(&subcommand.as_str()) {
            return Err(CommandValidationError::UnsupportedSubcommand {
                tool: "Maven",
                value: subcommand.clone(),
            });
        }
        validate_plain_args(&args)?;
        return Ok(AllowedCommand::Maven {
            program: cmd.to_string(),
            subcommand: subcommand.clone(),
            args: args[1..].to_vec(),
        });
    }

    // Gradle
    if cmd == "gradle" || cmd == "./gradlew" {
        if args.is_empty() {
            return Err(CommandValidationError::MissingSubcommand { tool: "Gradle" });
        }
        let task = &args[0];
        if !ALLOWED_GRADLE_TASKS.contains(&task.as_str()) {
            return Err(CommandValidationError::UnsupportedSubcommand {
                tool: "Gradle",
                value: task.clone(),
            });
        }
        validate_plain_args(&args)?;
        return Ok(AllowedCommand::Gradle {
            program: cmd.to_string(),
            task: task.clone(),
            args: args[1..].to_vec(),
        });
    }

    // Git (read-only operations)
    if cmd == "git" {
        if args.is_empty() {
            return Err(CommandValidationError::MissingSubcommand { tool: "Git" });
        }
        let subcommand = &args[0];
        if !ALLOWED_GIT_SUBCOMMANDS.contains(&subcommand.as_str()) {
            return Err(CommandValidationError::UnsupportedSubcommand {
                tool: "Git",
                value: subcommand.clone(),
            });
        }
        validate_git_args(subcommand, &args[1..])?;
        return Ok(AllowedCommand::GitRead {
            subcommand: subcommand.clone(),
            args: args[1..].to_vec(),
        });
    }

    // File read operations
    if ALLOWED_FILE_COMMANDS.contains(&cmd) {
        validate_file_args(cmd, &args)?;
        return Ok(AllowedCommand::FileRead {
            command: cmd.to_string(),
            args,
        });
    }

    // Java runtime and compiler execution is restricted to version inspection.
    if matches!(cmd, "java" | "javac" | "jshell") {
        validate_plain_args(&args)?;
        if args.len() != 1 || !matches!(args[0].as_str(), "-version" | "--version") {
            return Err(CommandValidationError::JavaToolRestricted {
                tool: cmd.to_owned(),
            });
        }
        return Ok(AllowedCommand::JavaTool {
            tool: cmd.to_string(),
            args,
        });
    }

    // javap inspects class files but must not pass arbitrary options to its JVM.
    if cmd == "javap" {
        validate_plain_args(&args)?;
        if args.iter().any(|arg| arg.starts_with("-J")) {
            return Err(CommandValidationError::JavapJvmOption);
        }
        return Ok(AllowedCommand::JavaTool {
            tool: cmd.to_string(),
            args,
        });
    }

    Err(CommandValidationError::UnknownCommand(cmd.to_owned()))
}

fn execute_allowed_command(allowed: &AllowedCommand, root: &Path) -> Result<String> {
    let mut cmd = match allowed {
        AllowedCommand::Maven {
            program,
            subcommand,
            args,
        } => {
            let mut c = Command::new(program);
            c.arg(subcommand);
            c.args(args);
            c
        }
        AllowedCommand::Gradle {
            program,
            task,
            args,
        } => {
            let mut c = Command::new(program);
            c.arg(task);
            c.args(args);
            c
        }
        AllowedCommand::GitRead { subcommand, args } => {
            let mut c = Command::new("git");
            c.arg(subcommand);
            c.args(args);
            c
        }
        AllowedCommand::FileRead { command, args } => {
            let mut c = Command::new(command);
            c.args(args);
            c
        }
        AllowedCommand::JavaTool { tool, args } => {
            let mut c = Command::new(tool);
            c.args(args);
            c
        }
    };

    cmd.current_dir(root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }

    let child = cmd
        .spawn()
        .with_context(|| format!("unable to start allowed command {allowed}"))?;
    let pid = child.id();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = child.wait_with_output();
        let _ = tx.send(result);
    });

    let output = match rx.recv_timeout(Duration::from_secs(120)) {
        Ok(Ok(output)) => output,
        Ok(Err(error)) => return Err(anyhow!("command {allowed} failed: {error}")),
        Err(_) => {
            kill_process_tree(pid);
            return Err(anyhow!("command {allowed} timed out after 120 seconds"));
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    Ok(format!(
        "Exit status: {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        truncate(&stdout, 12_000),
        truncate(&stderr, 12_000)
    ))
}

#[cfg(unix)]
fn kill_process_tree(pid: u32) {
    // The command is its own process group leader, so a negative pid reaches descendants.
    let _ = Command::new("kill")
        .arg("-9")
        .arg(format!("-{pid}"))
        .status();
}

#[cfg(windows)]
fn kill_process_tree(pid: u32) {
    let _ = Command::new("taskkill")
        .arg("/PID")
        .arg(pid.to_string())
        .args(["/T", "/F"])
        .status();
}

#[cfg(not(any(unix, windows)))]
fn kill_process_tree(_pid: u32) {
    eprintln!("Process-tree termination is unsupported on this platform");
}

pub(crate) fn run_command(root: &Path, command: &str) -> Option<String> {
    // Parse and validate command against whitelist
    let allowed_cmd = match parse_and_validate_command(command) {
        Ok(cmd) => cmd,
        Err(err) => {
            println!("❌ Command blocked: {}", err);
            println!("💡 See docs/command-security.md for the command policy.");
            return None;
        }
    };

    // User confirmation
    if !confirm(command) {
        println!("Command skipped.");
        return None;
    }

    // Execute using parameterized command (not shell string)
    match execute_allowed_command(&allowed_cmd, root) {
        Ok(output) => {
            println!("✅ Command completed");
            // Print output for user (simplified - all non-empty lines)
            for line in output.lines() {
                if !line.is_empty() {
                    println!("{}", line);
                }
            }
            Some(format!("{}\n{}", command, output))
        }
        Err(err) => {
            println!("❌ Command failed: {}", err);
            Some(format!("{}\nFailed: {}", command, err))
        }
    }
}

pub(crate) fn confirm(command: &str) -> bool {
    print!("Execute `{command}`? [y/N] ");
    let _ = io::stdout().flush();
    let mut answer = String::new();
    io::stdin().read_line(&mut answer).is_ok() && matches!(answer.trim(), "y" | "Y" | "yes")
}
