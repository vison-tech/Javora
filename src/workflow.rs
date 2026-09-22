use super::*;
use std::fmt::Write as _;

pub(crate) fn should_decompose(requirement: &str) -> bool {
    let indicators = [
        "实现",
        "开发",
        "设计并实现",
        "功能",
        "模块",
        "系统",
        "implement",
        "develop",
        "feature",
        "module",
        "system",
    ];

    let word_count = requirement.split_whitespace().count();
    let has_indicator = indicators
        .iter()
        .any(|&word| requirement.to_lowercase().contains(word));

    word_count > 20 || has_indicator
}

pub(crate) fn decompose_task(
    root: &Path,
    session: &Session,
    requirement: &str,
) -> Option<TaskBreakdown> {
    let system = "You are Javora's task decomposition expert. Break down the user requirement into sub-tasks.
Return a structured task breakdown in this format:

JAVORA_TASK_BREAKDOWN
SUMMARY: overall description
STRATEGY: Mixed

TASK: task-1
NAME: Task name
TYPE: Analysis
DEPENDENCIES:
COMPLEXITY: Medium
DESCRIPTION: Detailed description
AGENT_PROMPT: Prompt for the agent to execute this task

TASK: task-2
NAME: Another task
TYPE: Implementation
DEPENDENCIES: task-1
COMPLEXITY: High
DESCRIPTION: Implementation details
AGENT_PROMPT: Implement based on analysis...

END_JAVORA_TASK_BREAKDOWN

Valid TYPEs: Analysis, Design, Implementation, Testing, Integration
Valid COMPLEXITY: Low, Medium, High
Valid STRATEGY: Sequential, Parallel, Mixed
DEPENDENCIES: comma-separated task IDs or empty";

    let response = model_request(root, session, requirement, None, system, true)?;
    parse_task_breakdown(&response).ok()
}

pub(crate) fn parse_task_breakdown(
    response: &str,
) -> std::result::Result<TaskBreakdown, TaskBreakdownError> {
    let body = response
        .strip_prefix("JAVORA_TASK_BREAKDOWN\n")
        .and_then(|text| text.strip_suffix("\nEND_JAVORA_TASK_BREAKDOWN"))
        .ok_or(TaskBreakdownError::MissingEnvelope)?;

    let lines: Vec<&str> = body.lines().collect();
    let summary = lines
        .iter()
        .find_map(|l| l.strip_prefix("SUMMARY: "))
        .ok_or(TaskBreakdownError::MissingField("SUMMARY"))?
        .to_owned();

    let strategy_str = lines
        .iter()
        .find_map(|l| l.strip_prefix("STRATEGY: "))
        .unwrap_or("Mixed");

    let execution_strategy = match strategy_str {
        "Sequential" => ExecutionStrategy::Sequential,
        "Parallel" => ExecutionStrategy::Parallel,
        _ => ExecutionStrategy::Mixed,
    };

    let mut tasks = Vec::new();
    let task_blocks: Vec<&str> = body.split("\nTASK: ").collect();

    for block in task_blocks.iter().skip(1) {
        let block_lines: Vec<&str> = block.lines().collect();
        let id = block_lines
            .first()
            .ok_or(TaskBreakdownError::MissingField("task ID"))?
            .to_string();

        let name = block_lines
            .iter()
            .find_map(|l| l.strip_prefix("NAME: "))
            .ok_or(TaskBreakdownError::MissingField("NAME"))?
            .to_owned();

        let description = block_lines
            .iter()
            .find_map(|l| l.strip_prefix("DESCRIPTION: "))
            .ok_or(TaskBreakdownError::MissingField("DESCRIPTION"))?
            .to_owned();

        let type_str = block_lines
            .iter()
            .find_map(|l| l.strip_prefix("TYPE: "))
            .ok_or(TaskBreakdownError::MissingField("TYPE"))?;

        let complexity_str = block_lines
            .iter()
            .find_map(|l| l.strip_prefix("COMPLEXITY: "))
            .ok_or(TaskBreakdownError::MissingField("COMPLEXITY"))?;

        let deps_str = block_lines
            .iter()
            .find_map(|l| l.strip_prefix("DEPENDENCIES: "))
            .unwrap_or("");

        let agent_prompt = block_lines
            .iter()
            .find_map(|l| l.strip_prefix("AGENT_PROMPT: "))
            .ok_or(TaskBreakdownError::MissingField("AGENT_PROMPT"))?
            .to_owned();

        let task_type = match type_str {
            "Analysis" => TaskType::Analysis,
            "Design" => TaskType::Design,
            "Implementation" => TaskType::Implementation,
            "Testing" => TaskType::Testing,
            _ => TaskType::Integration,
        };

        let estimated_complexity = match complexity_str {
            "Low" => ComplexityLevel::Low,
            "Medium" => ComplexityLevel::Medium,
            _ => ComplexityLevel::High,
        };

        let dependencies: Vec<String> = if deps_str.trim().is_empty() {
            Vec::new()
        } else {
            deps_str.split(',').map(|s| s.trim().to_owned()).collect()
        };

        tasks.push(Task {
            id,
            name,
            description,
            dependencies,
            task_type,
            agent_prompt,
            estimated_complexity,
        });
    }

    if tasks.is_empty() {
        return Err(TaskBreakdownError::NoTasks);
    }

    validate_task_dependencies(&tasks)?;

    Ok(TaskBreakdown {
        summary,
        tasks,
        execution_strategy,
    })
}

pub(crate) fn topological_task_levels(
    tasks: &[Task],
) -> std::result::Result<Vec<Vec<String>>, TaskBreakdownError> {
    use std::collections::{HashMap, HashSet, VecDeque};

    let mut known_ids = HashSet::new();
    for task in tasks {
        if !known_ids.insert(task.id.clone()) {
            return Err(TaskBreakdownError::DuplicateTaskId(task.id.clone()));
        }
    }

    for task in tasks {
        for dep in &task.dependencies {
            if dep == &task.id {
                return Err(TaskBreakdownError::SelfDependency(task.id.clone()));
            }
            if !known_ids.contains(dep) {
                return Err(TaskBreakdownError::UnknownDependency {
                    task: task.id.clone(),
                    dependency: dep.clone(),
                });
            }
        }
    }

    let mut indegree: HashMap<String, usize> = tasks
        .iter()
        .map(|task| (task.id.clone(), task.dependencies.len()))
        .collect();
    let mut dependents: HashMap<String, Vec<String>> = HashMap::new();
    for task in tasks {
        for dependency in &task.dependencies {
            dependents
                .entry(dependency.clone())
                .or_default()
                .push(task.id.clone());
        }
    }

    let mut ready: VecDeque<String> = tasks
        .iter()
        .filter(|task| indegree[&task.id] == 0)
        .map(|task| task.id.clone())
        .collect();
    let mut processed = 0;
    let mut levels = Vec::new();
    while !ready.is_empty() {
        let current_level: Vec<String> = ready.drain(..).collect();
        processed += current_level.len();
        let mut next_level = VecDeque::new();
        for task_id in &current_level {
            if let Some(children) = dependents.get(task_id) {
                for child in children {
                    let count = indegree
                        .get_mut(child)
                        .expect("dependency child must be known");
                    *count -= 1;
                    if *count == 0 {
                        next_level.push_back(child.clone());
                    }
                }
            }
        }
        levels.push(current_level);
        ready = next_level;
    }

    if processed != tasks.len() {
        Err(TaskBreakdownError::DependencyCycle)
    } else {
        Ok(levels)
    }
}

pub(crate) fn validate_task_dependencies(
    tasks: &[Task],
) -> std::result::Result<(), TaskBreakdownError> {
    topological_task_levels(tasks).map(|_| ())
}

pub(crate) fn show_task_breakdown(breakdown: &TaskBreakdown) {
    println!(
        "\n我建议将这个任务分解为 {} 个子任务：",
        breakdown.tasks.len()
    );
    println!("{}\n", breakdown.summary);

    for (idx, task) in breakdown.tasks.iter().enumerate() {
        let type_label = match task.task_type {
            TaskType::Analysis => "分析",
            TaskType::Design => "设计",
            TaskType::Implementation => "实现",
            TaskType::Testing => "测试",
            TaskType::Integration => "集成",
        };

        println!("{}. {} [{}]", idx + 1, task.name, type_label);
        println!("   {}", task.description);

        if !task.dependencies.is_empty() {
            let dep_names: Vec<String> = breakdown
                .tasks
                .iter()
                .filter(|t| task.dependencies.contains(&t.id))
                .map(|t| t.name.clone())
                .collect();
            if !dep_names.is_empty() {
                println!("   依赖：{}", dep_names.join("、"));
            }
        }
    }

    let strategy_desc = match breakdown.execution_strategy {
        ExecutionStrategy::Sequential => "按顺序逐个执行",
        ExecutionStrategy::Parallel => "并行执行所有任务",
        ExecutionStrategy::Mixed => "根据依赖关系自动调度执行",
    };
    println!("\n执行策略：{}", strategy_desc);
    println!("\n请回复「确认」开始执行，或者说明需要调整的地方。");
}

fn task_system_prompt(task_type: &TaskType) -> &'static str {
    match task_type {
        TaskType::Analysis => "You are a code analysis expert for Javora. Analyze the code and provide insights. Be thorough and specific.",
        TaskType::Design => "You are a software architect for Javora. Design a clean, maintainable solution following Java best practices.",
        TaskType::Implementation => "You are Javora's implementation agent. Generate a JAVORA_PLAN with file changes following the exact format.",
        TaskType::Testing => "You are a testing expert for Javora. Design comprehensive test cases covering edge cases.",
        TaskType::Integration => "You are an integration expert for Javora. Merge and reconcile results coherently.",
    }
}

fn build_task_context(results: &[AgentResult], current_task: &Task) -> String {
    let mut context = String::new();

    for dep_id in &current_task.dependencies {
        if let Some(result) = results.iter().find(|r| r.task_id == *dep_id) {
            let _ = write!(context, "\n### 依赖任务 {} 的输出：\n", dep_id);
            super::text::append_truncated(&mut context, &result.output, 2000);
            context.push('\n');
        }
    }

    context
}

pub(crate) fn execute_tasks_with_dependencies(
    root: &Path,
    session: &mut Session,
    breakdown: &TaskBreakdown,
) -> Vec<AgentResult> {
    use std::collections::{HashMap, HashSet};

    let mut results = Vec::new();
    let mut completed: HashSet<String> = HashSet::new();
    let mut failed: HashSet<String> = HashSet::new();
    let task_by_id: HashMap<&str, &Task> = breakdown
        .tasks
        .iter()
        .map(|task| (task.id.as_str(), task))
        .collect();
    let levels = match topological_task_levels(&breakdown.tasks) {
        Ok(levels) => levels,
        Err(error) => {
            println!("\n抱歉，任务依赖无效：{error}");
            return breakdown
                .tasks
                .iter()
                .map(|task| AgentResult {
                    task_id: task.id.clone(),
                    status: AgentStatus::Failed,
                    output: String::new(),
                    plan: None,
                })
                .collect();
        }
    };

    for level in levels {
        for task_id in level {
            let task = task_by_id[task_id.as_str()];
            if task.dependencies.iter().any(|dep| failed.contains(dep)) {
                println!("\n跳过：{}（依赖的任务未成功完成）", task.name);
                failed.insert(task.id.clone());
                results.push(AgentResult {
                    task_id: task.id.clone(),
                    status: AgentStatus::Failed,
                    output: String::new(),
                    plan: None,
                });
                continue;
            }

            println!("\n开始：{}", task.name);

            let context = build_task_context(&results, task);
            let prompt = if context.is_empty() {
                task.agent_prompt.clone()
            } else {
                let mut prompt = String::with_capacity(task.agent_prompt.len() + 2_050);
                prompt.push_str(&task.agent_prompt);
                prompt.push_str("\n\n前置任务输出：");
                super::text::append_truncated(&mut prompt, &context, 2000);
                prompt
            };

            let system = task_system_prompt(&task.task_type);

            let response = model_request(root, session, &prompt, None, system, true);

            let result = match response {
                Some(output) => {
                    let plan = if matches!(task.task_type, TaskType::Implementation) {
                        parse_plan(&output).ok()
                    } else {
                        None
                    };

                    completed.insert(task.id.clone());

                    AgentResult {
                        task_id: task.id.clone(),
                        status: AgentStatus::Completed,
                        output,
                        plan,
                    }
                }
                None => {
                    failed.insert(task.id.clone());
                    AgentResult {
                        task_id: task.id.clone(),
                        status: AgentStatus::Failed,
                        output: String::new(),
                        plan: None,
                    }
                }
            };

            if matches!(result.status, AgentStatus::Completed) {
                println!("✓ 完成");
            } else {
                println!("✗ 失败");
            }

            results.push(result);
        }
    }

    results
}

pub(crate) fn aggregate_results(
    _root: &Path,
    _session: &mut Session,
    requirement: &str,
    results: &[AgentResult],
) -> Option<ChangePlan> {
    use std::collections::HashMap;

    let mut all_changes: HashMap<String, String> = HashMap::new();
    let mut all_tests: Vec<String> = Vec::new();

    for result in results {
        if let Some(plan) = &result.plan {
            for change in &plan.changes {
                all_changes.insert(change.path.clone(), change.content.clone());
            }
            all_tests.extend(plan.tests.clone());
        }
    }

    if !all_changes.is_empty() {
        let changes: Vec<FileChange> = all_changes
            .into_iter()
            .map(|(path, content)| FileChange { path, content })
            .collect();

        println!("\n所有任务已完成，我已经为你准备好了代码变更。");

        return Some(ChangePlan {
            summary: format!("多Agent协作完成：{}", requirement),
            changes,
            tests: all_tests,
        });
    }

    println!("\n好的，所有分析和设计任务都完成了：");
    for result in results {
        if matches!(result.status, AgentStatus::Completed) {
            println!("\n【{}】", result.task_id);
            let preview = truncate(&result.output, 500);
            println!("{}", preview);
        }
    }

    None
}
