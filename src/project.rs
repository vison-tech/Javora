use super::*;

fn ignored(path: &Path) -> bool {
    path.components().any(|part| {
        matches!(
            part.as_os_str().to_str(),
            Some(".git" | ".idea" | ".codex" | ".Javora" | "target" | "node_modules")
        )
    })
}

pub(crate) fn collect_files(root: &Path, output: &mut Vec<String>) -> io::Result<()> {
    if ignored(root) {
        return Ok(());
    }
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if ignored(&path) {
            continue;
        }
        // Reject symlinks to prevent following links outside project or circular references
        if let Ok(metadata) = path.symlink_metadata() {
            if metadata.file_type().is_symlink() {
                continue;
            }
        }
        if path.is_dir() {
            collect_files(&path, output)?;
        } else if let Ok(relative) = path.strip_prefix(root) {
            output.push(relative.display().to_string());
        }
    }
    Ok(())
}

pub(crate) fn project_context(root: &Path) -> String {
    let mut files = Vec::new();
    let _ = collect_files(root, &mut files);
    files.sort();
    files.truncate(200);
    format!(
        "Project root: {}\n{}\nFiles:\n{}",
        root.display(),
        java_project_context(root, &files),
        files.join("\n")
    )
}

pub(crate) fn java_project_context(root: &Path, files: &[String]) -> String {
    let maven = root.join("pom.xml").exists();
    let gradle = root.join("build.gradle").exists() || root.join("build.gradle.kts").exists();
    let java_files: Vec<_> = files
        .iter()
        .filter(|file| file.ends_with(".java"))
        .collect();
    let test_count = java_files
        .iter()
        .filter(|file| file.contains("/src/test/") || file.contains("\\src\\test\\"))
        .count();
    let main_count = java_files.len().saturating_sub(test_count);
    let modules: Vec<_> = files
        .iter()
        .filter_map(|file| {
            file.strip_suffix("/pom.xml")
                .or_else(|| file.strip_suffix("/build.gradle"))
                .or_else(|| file.strip_suffix("/build.gradle.kts"))
        })
        .filter(|module| !module.is_empty())
        .take(20)
        .collect();
    let mut symbols = Vec::new();
    for file in java_files.iter().take(80) {
        if let Some(symbol) = java_symbol(root, file) {
            symbols.push(symbol);
        }
        if symbols.len() >= 40 {
            break;
        }
    }
    let build = match (maven, gradle) {
        (true, true) => "Maven and Gradle",
        (true, false) => "Maven",
        (false, true) => "Gradle",
        (false, false) => "unknown",
    };
    let module_text = if modules.is_empty() {
        "single module or no build module detected".to_owned()
    } else {
        modules.join(", ")
    };
    let symbol_text = if symbols.is_empty() {
        "none indexed".to_owned()
    } else {
        symbols.join("\n")
    };
    format!("Java project profile:\nBuild: {build}\nModules: {module_text}\nJava sources: {main_count}, tests: {test_count}\nIndexed symbols:\n{symbol_text}")
}

pub(crate) fn java_symbol(root: &Path, file: &str) -> Option<String> {
    let path = root.join(file);
    let contents = fs::read_to_string(path).ok()?;
    let contents = truncate(&contents, 16_000);
    let package = contents
        .lines()
        .find_map(|line| {
            line.trim()
                .strip_prefix("package ")
                .and_then(|name| name.strip_suffix(';'))
        })
        .unwrap_or("<default>");
    let kind = ["class", "interface", "record", "enum"]
        .iter()
        .find_map(|kind| {
            contents.lines().find_map(|line| {
                line.split_whitespace()
                    .position(|word| word == *kind)
                    .and_then(|index| {
                        line.split_whitespace()
                            .nth(index + 1)
                            .map(|name| (*kind, name.trim_matches('{').trim_matches('(')))
                    })
            })
        })?;
    let annotations: Vec<_> = [
        "@RestController",
        "@Controller",
        "@Service",
        "@Repository",
        "@Component",
        "@Configuration",
        "@SpringBootApplication",
    ]
    .iter()
    .filter(|annotation| contents.contains(**annotation))
    .copied()
    .collect();
    let annotation_text = if annotations.is_empty() {
        String::new()
    } else {
        format!(" [{}]", annotations.join(", "))
    };
    Some(format!(
        "{file}: {package}.{} {}{annotation_text}",
        kind.1, kind.0
    ))
}

pub(crate) fn design_documents(root: &Path) -> Vec<String> {
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

pub(crate) fn find_design(root: &Path, request: &str) -> Vec<String> {
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

pub(crate) fn document_text(root: &Path, file: &str) -> Option<String> {
    safe_path(root, file)
        .and_then(|path| fs::read_to_string(path).ok())
        .filter(|text| text.len() <= MAX_FILE_BYTES)
}

pub(crate) fn safe_path(root: &Path, value: &str) -> Option<PathBuf> {
    let root = root.canonicalize().ok()?;
    let path = root.join(value).canonicalize().ok()?;
    if path.strip_prefix(&root).is_ok() {
        Some(path)
    } else {
        None
    }
}
