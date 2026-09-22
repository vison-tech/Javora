use super::*;

pub(crate) fn parse_plan(response: &str) -> Result<ChangePlan> {
    let body = response
        .strip_prefix("JAVORA_PLAN\n")
        .and_then(|text| text.strip_suffix("\nEND_JAVORA_PLAN"))
        .ok_or_else(|| anyhow!("Model did not return a Javora change plan"))?;
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
                return Err(anyhow!("Missing content block for {path}"));
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
                return Err(anyhow!("Missing closing content block for {path}"));
            }
            if content.len() > MAX_FILE_BYTES {
                return Err(anyhow!("Generated content for {path} is too large"));
            }
            changes.push(FileChange {
                path: path.to_owned(),
                content,
            });
        }
    }
    if summary.is_empty() || changes.is_empty() {
        Err(anyhow!("Plan requires SUMMARY and at least one FILE"))
    } else {
        Ok(ChangePlan {
            summary,
            changes,
            tests,
        })
    }
}

fn write_path(root: &Path, value: &str) -> Option<PathBuf> {
    use std::path::Component;

    let path_obj = Path::new(value);

    // Explicitly reject absolute paths
    if path_obj.is_absolute() {
        return None;
    }

    // Explicitly reject paths containing parent directory components
    for component in path_obj.components() {
        if matches!(component, Component::ParentDir) {
            return None;
        }
    }

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
    if path.exists() && path.symlink_metadata().ok()?.file_type().is_symlink() {
        return None;
    }
    Some(path)
}

pub(crate) fn show_plan(root: &Path, plan: &ChangePlan) -> bool {
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

pub(crate) fn apply_plan(root: &Path, plan: &ChangePlan) -> Result<()> {
    let mut targets = Vec::new();
    for change in &plan.changes {
        let path = write_path(root, &change.path)
            .ok_or_else(|| anyhow!("Unsafe output path: {}", change.path))?;
        targets.push((path, &change.content));
    }
    if targets
        .iter()
        .any(|(_, content)| content.len() > MAX_FILE_BYTES)
    {
        return Err(anyhow!("Plan contains oversized content"));
    }
    for (path, content) in targets {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create parent directory {}", parent.display()))?;
        }
        fs::write(&path, content)
            .with_context(|| format!("write generated file {}", path.display()))?;
        let written = fs::read_to_string(&path)
            .with_context(|| format!("verify generated file {}", path.display()))?;
        if written != *content {
            return Err(anyhow!(
                "post-write verification failed: {}",
                path.display()
            ));
        }
    }
    Ok(())
}
