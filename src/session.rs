use super::*;
use std::fmt::Write as _;

impl Session {
    pub(crate) fn new() -> Self {
        Self {
            history: Vec::new(),
            conversation: Vec::new(),
            last_command_result: None,
            workflow: Workflow::Idle,
            resume_required: false,
            plan_version: 0,
            user_context: Vec::new(),
            compression_count: 0,
        }
    }

    pub(crate) fn remember(&mut self, entry: String) {
        self.history.push(entry);
        if self.history.len() > 8 {
            self.history.remove(0);
        }
    }

    pub(crate) fn add_turn(&mut self, role: &str, content: String, turn_type: TurnType) {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.conversation.push(ConversationTurn {
            role: role.to_owned(),
            content,
            timestamp,
            turn_type,
        });
        if self.conversation.len() > 20 {
            self.conversation.remove(0);
        }
    }

    pub(crate) fn estimate_tokens(&self) -> usize {
        self.conversation
            .iter()
            .map(|turn| super::text::estimate_tokens(&turn.content))
            .chain(
                self.user_context
                    .iter()
                    .map(|context| super::text::estimate_tokens(context)),
            )
            .chain(
                self.last_command_result
                    .iter()
                    .map(|result| super::text::estimate_tokens(result)),
            )
            .sum()
    }

    pub(crate) fn should_compress(&self) -> bool {
        self.estimate_tokens() > MAX_CONTEXT_TOKENS
    }

    pub(crate) fn apply_compression_summary(&mut self, summary: String) {
        let compressed_turn = ConversationTurn {
            role: "system".to_owned(),
            content: format!(
                "[自动压缩 #{} - {}]\n{}",
                self.compression_count + 1,
                super::llm::chrono_now_str(),
                summary
            ),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            turn_type: TurnType::AssistantResponse,
        };

        let recent_turns: Vec<_> = self
            .conversation
            .iter()
            .rev()
            .take(5)
            .rev()
            .cloned()
            .collect();

        self.conversation.clear();
        self.conversation.push(compressed_turn);
        self.conversation.extend(recent_turns);
        self.compression_count += 1;
    }

    pub(crate) fn compress_context(&mut self, root: &Path, config: &ModelConfig) {
        if !self.should_compress() {
            return;
        }

        println!("上下文已超过 {} tokens，正在压缩...", MAX_CONTEXT_TOKENS);

        let summary = self.generate_summary(root, config);
        if let Some(summary) = summary {
            let recent_count = self.conversation.len().min(5);
            self.apply_compression_summary(summary);

            println!(
                "压缩完成，从 {} 轮对话压缩为摘要 + 最近 5 轮",
                recent_count + 1
            );
        }
    }

    fn generate_summary(&self, root: &Path, config: &ModelConfig) -> Option<String> {
        let mut content = String::new();
        content.push_str("请总结以下对话历史，提取关键信息：\n\n");

        for (index, turn) in self.conversation.iter().enumerate() {
            let label = match turn.turn_type {
                TurnType::UserRequest => "用户",
                TurnType::AssistantQuestion => "Javora[问题]",
                TurnType::AssistantResponse => "Javora",
                TurnType::CommandExecution => "命令结果",
                TurnType::FileInspection => "文件检查",
            };
            write!(content, "[{}] {}: ", index + 1, label).unwrap();
            super::text::append_truncated(&mut content, &turn.content, 500);
            content.push_str("\n\n");
        }

        content.push_str("请生成一个简洁的摘要，保留：\n");
        content.push_str("1. 用户的主要目标和需求\n");
        content.push_str("2. 已完成的关键操作\n");
        content.push_str("3. 重要的决策和结论\n");
        content.push_str("4. 当前进度和待办事项\n");

        let system = "你是 Javora 的会话压缩助手。生成简洁的摘要，去除冗余信息，保留关键上下文。";
        super::llm::compress_request(root, config, &content, system)
    }

    pub(crate) fn structured_context(&self) -> String {
        let mut context = String::new();
        if !self.user_context.is_empty() {
            context.push_str("\n\nUser context:\n");
            for (index, item) in self.user_context.iter().enumerate() {
                if index > 0 {
                    context.push_str(", ");
                }
                context.push_str(item);
            }
        }
        if !self.conversation.is_empty() {
            context.push_str("\n\nConversation history:\n");
            for turn in self.conversation.iter().rev().take(10).rev() {
                let turn_label = match turn.turn_type {
                    TurnType::UserRequest => "User",
                    TurnType::AssistantQuestion => "Javora [question]",
                    TurnType::AssistantResponse => "Javora",
                    TurnType::CommandExecution => "Command result",
                    TurnType::FileInspection => "File inspection",
                };
                write!(context, "{}: ", turn_label).unwrap();
                super::text::append_truncated(&mut context, &turn.content, 200);
                context.push('\n');
            }
        }
        if let Some(result) = &self.last_command_result {
            context.push_str("\nLatest command result:\n");
            super::text::append_truncated(&mut context, result, 500);
            context.push('\n');
        }
        context
    }

    pub(crate) fn add_user_context(&mut self, context: String) {
        if !self.user_context.contains(&context) {
            self.user_context.push(context);
            if self.user_context.len() > 5 {
                self.user_context.remove(0);
            }
        }
    }
}

fn hex_encode_into(output: &mut String, value: &str) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for byte in value.bytes() {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
}

fn hex_encode(value: &str) -> String {
    let mut output = String::with_capacity(value.len() * 2);
    hex_encode_into(&mut output, value);
    output
}

pub(crate) fn hex_decode(value: &str) -> Option<String> {
    if !value.len().is_multiple_of(2) {
        return None;
    }
    let bytes = (0..value.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&value[index..index + 2], 16).ok())
        .collect::<Option<Vec<_>>>()?;
    String::from_utf8(bytes).ok()
}

fn conversation_turn_code(turn_type: &TurnType) -> &'static str {
    match turn_type {
        TurnType::UserRequest => "UR",
        TurnType::AssistantQuestion => "AQ",
        TurnType::AssistantResponse => "AR",
        TurnType::CommandExecution => "CE",
        TurnType::FileInspection => "FI",
    }
}

pub(crate) fn serialize_conversation(conversation: &[ConversationTurn]) -> String {
    let mut output = String::new();
    for (index, turn) in conversation.iter().enumerate() {
        if index > 0 {
            output.push('\n');
        }
        let _ = write!(
            output,
            "{}|{}|{}|",
            conversation_turn_code(&turn.turn_type),
            turn.timestamp,
            turn.role
        );
        hex_encode_into(&mut output, &turn.content);
    }
    output
}

pub(crate) fn serialize_user_context(user_context: &[String]) -> String {
    let mut output = String::new();
    for (index, context) in user_context.iter().enumerate() {
        if index > 0 {
            hex_encode_into(&mut output, "||");
        }
        hex_encode_into(&mut output, context);
    }
    output
}

pub(crate) fn save_session(root: &Path, session: &Session) -> Result<()> {
    let path = root.join(SESSION_STATE);
    let conversation_encoded = serialize_conversation(&session.conversation);
    let user_context_encoded = serialize_user_context(&session.user_context);
    match &session.workflow {
        Workflow::Idle => {
            if path.exists() {
                fs::remove_file(path)?;
            }
        }
        Workflow::Clarifying { transcript } => {
            fs::create_dir_all(path.parent().expect("state parent"))?;
            fs::write(
                path,
                format!(
                    "VERSION: 3\nSTATE: CLARIFYING\nPLAN_VERSION: {}\nCOMPRESSION_COUNT: {}\nTRANSCRIPT: {}\nUSER_CONTEXT: {}\nCONVERSATION:\n{}\n",
                    session.plan_version,
                    session.compression_count,
                    hex_encode(transcript),
                    user_context_encoded,
                    conversation_encoded
                ),
            )?;
        }
        Workflow::AwaitingApproval { requirement, plan } => {
            fs::create_dir_all(path.parent().expect("state parent"))?;
            fs::write(
                path,
                format!(
                    "VERSION: 3\nSTATE: AWAITING_APPROVAL\nPLAN_VERSION: {}\nCOMPRESSION_COUNT: {}\nREQUIREMENT: {}\nPLAN: {}\nUSER_CONTEXT: {}\nCONVERSATION:\n{}\n",
                    session.plan_version,
                    session.compression_count,
                    hex_encode(requirement),
                    hex_encode(plan),
                    user_context_encoded,
                    conversation_encoded
                ),
            )?;
        }
        Workflow::AwaitingTaskApproval {
            requirement,
            task_breakdown,
        } => {
            fs::create_dir_all(path.parent().expect("state parent"))?;
            let breakdown_encoded = serialize_task_breakdown(task_breakdown);
            fs::write(
                path,
                format!(
                    "VERSION: 4\nSTATE: AWAITING_TASK_APPROVAL\nPLAN_VERSION: {}\nCOMPRESSION_COUNT: {}\nREQUIREMENT: {}\nTASK_BREAKDOWN: {}\nUSER_CONTEXT: {}\nCONVERSATION:\n{}\n",
                    session.plan_version,
                    session.compression_count,
                    hex_encode(requirement),
                    hex_encode(&breakdown_encoded),
                    user_context_encoded,
                    conversation_encoded
                ),
            )?;
        }
        Workflow::AgentExecution {
            requirement,
            task_breakdown,
            completed_tasks,
        } => {
            fs::create_dir_all(path.parent().expect("state parent"))?;
            let breakdown_encoded = serialize_task_breakdown(task_breakdown);
            let completed_encoded = hex_encode(&completed_tasks.join("||"));
            fs::write(
                path,
                format!(
                    "VERSION: 4\nSTATE: AGENT_EXECUTION\nPLAN_VERSION: {}\nCOMPRESSION_COUNT: {}\nREQUIREMENT: {}\nTASK_BREAKDOWN: {}\nCOMPLETED_TASKS: {}\nUSER_CONTEXT: {}\nCONVERSATION:\n{}\n",
                    session.plan_version,
                    session.compression_count,
                    hex_encode(requirement),
                    hex_encode(&breakdown_encoded),
                    completed_encoded,
                    user_context_encoded,
                    conversation_encoded
                ),
            )?;
        }
        Workflow::Approved { requirement } => {
            fs::create_dir_all(path.parent().expect("state parent"))?;
            fs::write(
                path,
                format!(
                    "VERSION: 3\nSTATE: APPROVED\nPLAN_VERSION: {}\nCOMPRESSION_COUNT: {}\nREQUIREMENT: {}\nUSER_CONTEXT: {}\nCONVERSATION:\n{}\n",
                    session.plan_version,
                    session.compression_count,
                    hex_encode(requirement),
                    user_context_encoded,
                    conversation_encoded
                ),
            )?;
        }
    }
    Ok(())
}

fn serialize_task_breakdown(breakdown: &TaskBreakdown) -> String {
    let mut result = String::new();
    let _ = write!(
        result,
        "SUMMARY:{}\nSTRATEGY:{:?}\nTASKS:\n",
        breakdown.summary, breakdown.execution_strategy
    );
    for task in &breakdown.tasks {
        let _ = write!(
            result,
            "ID:{}\nNAME:{}\nDESC:{}\nTYPE:{:?}\nCOMPLEXITY:{:?}\nDEPS:",
            task.id, task.name, task.description, task.task_type, task.estimated_complexity
        );
        for (index, dependency) in task.dependencies.iter().enumerate() {
            if index > 0 {
                result.push(',');
            }
            result.push_str(dependency);
        }
        result.push_str("\nPROMPT:");
        result.push_str(&task.agent_prompt);
        result.push_str("\n---\n");
    }
    result
}

fn deserialize_task_breakdown(data: &str) -> Option<TaskBreakdown> {
    let lines: Vec<&str> = data.lines().collect();
    let summary = lines.iter().find_map(|l| l.strip_prefix("SUMMARY:"))?;
    let strategy_str = lines.iter().find_map(|l| l.strip_prefix("STRATEGY:"))?;
    let execution_strategy = match strategy_str {
        "Sequential" => ExecutionStrategy::Sequential,
        "Parallel" => ExecutionStrategy::Parallel,
        _ => ExecutionStrategy::Mixed,
    };

    let mut tasks = Vec::new();
    let task_sections: Vec<&str> = data.split("---\n").collect();

    for section in task_sections.iter().skip(1) {
        if section.trim().is_empty() {
            continue;
        }
        let section_lines: Vec<&str> = section.lines().collect();
        let id = section_lines
            .iter()
            .find_map(|l| l.strip_prefix("ID:"))?
            .to_owned();
        let name = section_lines
            .iter()
            .find_map(|l| l.strip_prefix("NAME:"))?
            .to_owned();
        let description = section_lines
            .iter()
            .find_map(|l| l.strip_prefix("DESC:"))?
            .to_owned();
        let type_str = section_lines.iter().find_map(|l| l.strip_prefix("TYPE:"))?;
        let complexity_str = section_lines
            .iter()
            .find_map(|l| l.strip_prefix("COMPLEXITY:"))?;
        let deps_str = section_lines
            .iter()
            .find_map(|l| l.strip_prefix("DEPS:"))?
            .to_owned();
        let prompt = section_lines
            .iter()
            .find_map(|l| l.strip_prefix("PROMPT:"))?
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

        let dependencies: Vec<String> = if deps_str.is_empty() {
            Vec::new()
        } else {
            deps_str.split(',').map(|s| s.to_owned()).collect()
        };

        tasks.push(Task {
            id,
            name,
            description,
            dependencies,
            task_type,
            agent_prompt: prompt,
            estimated_complexity,
        });
    }

    Some(TaskBreakdown {
        summary: summary.to_owned(),
        tasks,
        execution_strategy,
    })
}

pub(crate) fn load_session(root: &Path) -> Result<Option<Session>> {
    let path = root.join(SESSION_STATE);
    let path = if path.exists() {
        path
    } else {
        root.join(LEGACY_SESSION_STATE)
    };
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path)
        .with_context(|| format!("read session state {}", path.display()))?;
    let lines = text.lines();
    let mut fields = std::collections::HashMap::new();
    let mut conversation_lines = Vec::new();
    let mut in_conversation = false;

    for line in lines {
        if line.starts_with("CONVERSATION:") {
            in_conversation = true;
            continue;
        }
        if in_conversation {
            if !line.is_empty() {
                conversation_lines.push(line);
            }
        } else if let Some((key, value)) = line.split_once(':') {
            fields.insert(key, value.trim());
        }
    }

    let version = fields.get("VERSION").copied().unwrap_or("2");
    if version != "4" && version != "3" && version != "2" {
        return Ok(None);
    }

    let plan_version = fields
        .get("PLAN_VERSION")
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    let compression_count = fields
        .get("COMPRESSION_COUNT")
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    let decode_field = |name: &str| fields.get(name).and_then(|value| hex_decode(value));

    let mut conversation = Vec::new();
    let mut user_context = Vec::new();

    if version == "3" || version == "4" {
        if let Some(encoded) = fields.get("USER_CONTEXT").and_then(|v| hex_decode(v)) {
            user_context = encoded
                .split("||")
                .filter(|s| !s.is_empty())
                .map(|s| s.to_owned())
                .collect();
        }

        for line in conversation_lines {
            let parts: Vec<&str> = line.splitn(4, '|').collect();
            if parts.len() == 4 {
                let turn_type = match parts[0] {
                    "UR" => TurnType::UserRequest,
                    "AQ" => TurnType::AssistantQuestion,
                    "AR" => TurnType::AssistantResponse,
                    "CE" => TurnType::CommandExecution,
                    "FI" => TurnType::FileInspection,
                    _ => continue,
                };
                if let (Ok(timestamp), Some(content)) =
                    (parts[1].parse::<u64>(), hex_decode(parts[3]))
                {
                    conversation.push(ConversationTurn {
                        role: parts[2].to_owned(),
                        content,
                        timestamp,
                        turn_type,
                    });
                }
            }
        }
    }

    let workflow = match fields.get("STATE").copied() {
        Some("CLARIFYING") => match decode_field("TRANSCRIPT") {
            Some(transcript) => Workflow::Clarifying { transcript },
            None => return Ok(None),
        },
        Some("AWAITING_APPROVAL") => match (decode_field("REQUIREMENT"), decode_field("PLAN")) {
            (Some(requirement), Some(plan)) => Workflow::AwaitingApproval { requirement, plan },
            _ => return Ok(None),
        },
        Some("AWAITING_TASK_APPROVAL") => {
            match (decode_field("REQUIREMENT"), decode_field("TASK_BREAKDOWN")) {
                (Some(requirement), Some(breakdown_data)) => {
                    if let Some(task_breakdown) = deserialize_task_breakdown(&breakdown_data) {
                        Workflow::AwaitingTaskApproval {
                            requirement,
                            task_breakdown,
                        }
                    } else {
                        return Ok(None);
                    }
                }
                _ => return Ok(None),
            }
        }
        Some("AGENT_EXECUTION") => {
            match (
                decode_field("REQUIREMENT"),
                decode_field("TASK_BREAKDOWN"),
                decode_field("COMPLETED_TASKS"),
            ) {
                (Some(requirement), Some(breakdown_data), Some(completed_data)) => {
                    if let Some(task_breakdown) = deserialize_task_breakdown(&breakdown_data) {
                        let completed_tasks: Vec<String> = completed_data
                            .split("||")
                            .filter(|s| !s.is_empty())
                            .map(|s| s.to_owned())
                            .collect();
                        Workflow::AgentExecution {
                            requirement,
                            task_breakdown,
                            completed_tasks,
                        }
                    } else {
                        return Ok(None);
                    }
                }
                _ => return Ok(None),
            }
        }
        Some("APPROVED") => match decode_field("REQUIREMENT") {
            Some(requirement) => Workflow::Approved { requirement },
            None => return Ok(None),
        },
        _ => return Ok(None),
    };
    let session = Session {
        history: Vec::new(),
        conversation,
        last_command_result: None,
        workflow,
        resume_required: true,
        plan_version,
        user_context,
        compression_count,
    };
    if path == root.join(LEGACY_SESSION_STATE) {
        let _ = save_session(root, &session);
    }
    Ok(Some(session))
}
