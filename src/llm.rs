use super::*;
use std::fmt::Write as _;

fn config_value(root: &Path, key: &str) -> Option<String> {
    fs::read_to_string(root.join(CONFIG_STATE))
        .or_else(|_| fs::read_to_string(root.join(LEGACY_CONFIG_STATE)))
        .ok()?
        .lines()
        .find_map(|line| {
            line.split_once('=')
                .filter(|(name, _)| *name == key)
                .map(|(_, value)| value.to_owned())
        })
}

fn prompt_value(label: &str) -> String {
    print!("{label}: ");
    let _ = io::stdout().flush();
    let mut value = String::new();
    let _ = io::stdin().read_line(&mut value);
    value.trim().to_owned()
}

pub(crate) fn model_config(root: &Path) -> Option<ModelConfig> {
    let configured_provider = env::var("JAVORA_PROVIDER")
        .ok()
        .or_else(|| config_value(root, "provider"));
    let configured_provider = configured_provider.or_else(|| {
        let value = prompt_value("Provider [openai/anthropic]");
        Some(if value.is_empty() {
            "openai".to_owned()
        } else {
            value
        })
    });
    let provider = match configured_provider.as_deref() {
        Some("anthropic") => Provider::Anthropic,
        Some("openai") | Some("") | None => Provider::OpenAi,
        Some(other) => {
            println!("Unsupported JAVORA_PROVIDER: {other}");
            return None;
        }
    };
    let model = env::var("JAVORA_MODEL")
        .ok()
        .or_else(|| config_value(root, "model"))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| prompt_value("Model (required)"));
    if model.is_empty() {
        println!("A model is required.");
        return None;
    }
    let default_base = match provider {
        Provider::OpenAi => "https://api.openai.com/v1",
        Provider::Anthropic => "https://api.anthropic.com/v1",
    };
    let base_url = env::var("JAVORA_BASE_URL")
        .ok()
        .or_else(|| config_value(root, "base_url"))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| prompt_value(&format!("Base URL [{default_base}]")));
    let base_url = if base_url.is_empty() {
        default_base.to_owned()
    } else {
        base_url
    };
    if !root.join(CONFIG_STATE).exists() {
        let provider_name = match provider {
            Provider::OpenAi => "openai",
            Provider::Anthropic => "anthropic",
        };
        let path = root.join(CONFIG_STATE);
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Err(error) = fs::write(
            path,
            format!("provider={provider_name}\nmodel={model}\nbase_url={base_url}\n"),
        ) {
            eprintln!("Unable to save local model configuration: {error}");
        }
        println!("Saved local provider configuration. Set OPENAI_API_KEY or ANTHROPIC_API_KEY in your environment.");
    }
    Some(ModelConfig {
        provider,
        base_url,
        model,
    })
}

fn push_json_escaped(output: &mut String, value: &str) {
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            '\u{0008}' => output.push_str("\\b"),
            '\u{000C}' => output.push_str("\\f"),
            character if character.is_control() => {
                let _ = write!(output, "\\u{:04x}", character as u32);
            }
            character => output.push(character),
        }
    }
}

fn push_json_string(output: &mut String, value: &str) {
    output.push('"');
    push_json_escaped(output, value);
    output.push('"');
}

fn openai_body(model: &str, system: &str, prompt: &str, max_tokens: Option<usize>) -> String {
    let mut body = String::with_capacity(model.len() + system.len() + prompt.len() + 96);
    body.push_str("{\"model\":");
    push_json_string(&mut body, model);
    body.push_str(",\"messages\":[{\"role\":\"system\",\"content\":");
    push_json_string(&mut body, system);
    body.push_str("},{\"role\":\"user\",\"content\":");
    push_json_string(&mut body, prompt);
    body.push_str("}]");
    if let Some(max_tokens) = max_tokens {
        let _ = write!(body, ",\"max_tokens\":{max_tokens}");
    }
    body.push('}');
    body
}

fn anthropic_body(model: &str, system: &str, prompt: &str, max_tokens: usize) -> String {
    let mut body = String::with_capacity(model.len() + system.len() + prompt.len() + 96);
    body.push_str("{\"model\":");
    push_json_string(&mut body, model);
    let _ = write!(body, ",\"max_tokens\":{max_tokens},\"system\":");
    push_json_string(&mut body, system);
    body.push_str(",\"messages\":[{\"role\":\"user\",\"content\":");
    push_json_string(&mut body, prompt);
    body.push_str("}]}");
    body
}

fn build_prompt(context: &str, document: Option<&str>, history: &str, input: &str) -> String {
    let document_capacity = document.map_or(0, |value| value.len() + 19);
    let mut prompt =
        String::with_capacity(context.len() + document_capacity + history.len() + input.len() + 14);
    prompt.push_str(context);
    if let Some(document) = document {
        prompt.push_str("\n\nDesign document:\n");
        prompt.push_str(document);
    }
    prompt.push_str(history);
    prompt.push_str("\n\nUser task: ");
    prompt.push_str(input);
    prompt
}

fn json_text_value(value: &serde_json::Value) -> Option<String> {
    if let Some(text) = value.as_str() {
        return Some(text.to_owned());
    }
    value.as_array()?.iter().find_map(|item| {
        item.get("text")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
    })
}

fn response_content(payload: &serde_json::Value) -> Option<String> {
    payload
        .get("content")
        .and_then(json_text_value)
        .or_else(|| payload.get("text").and_then(json_text_value))
        .or_else(|| {
            payload
                .get("choices")
                .and_then(serde_json::Value::as_array)
                .and_then(|choices| choices.first())
                .and_then(|choice| choice.get("message"))
                .and_then(|message| message.get("content"))
                .and_then(json_text_value)
        })
}
fn normalize_model_content(value: String) -> String {
    let value = value.trim();
    if let Some(body) = value
        .strip_prefix("```")
        .and_then(|text| text.find('\n').map(|index| &text[index + 1..]))
    {
        return body.strip_suffix("```").unwrap_or(body).trim().to_owned();
    }
    value.to_owned()
}

pub(crate) fn chrono_now_str() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let datetime = now;
    format!("{}", datetime)
}

pub(crate) fn create_curl_header(config: &ModelConfig, auth_header: &str) -> Result<NamedTempFile> {
    let mut header_file = NamedTempFile::new().context("create temporary curl header file")?;
    let provider_header = if matches!(config.provider, Provider::Anthropic) {
        "header = \"anthropic-version: 2023-06-01\"\n"
    } else {
        ""
    };
    let contents = format!(
        "header = \"{}\"\nheader = \"Content-Type: application/json\"\n{}",
        auth_header.replace('"', "\\\""),
        provider_header
    );
    header_file
        .write_all(contents.as_bytes())
        .context("write temporary curl header file")?;
    Ok(header_file)
}

pub(crate) fn compress_request(
    _root: &Path,
    config: &ModelConfig,
    content: &str,
    system: &str,
) -> Option<String> {
    let key_name = match config.provider {
        Provider::OpenAi => "OPENAI_API_KEY",
        Provider::Anthropic => "ANTHROPIC_API_KEY",
    };
    let key = env::var(key_name).unwrap_or_default();
    if key.is_empty() {
        return None;
    }

    let (url, body, auth_header) = match config.provider {
        Provider::OpenAi => (
            format!("{}/chat/completions", config.base_url.trim_end_matches('/')),
            openai_body(&config.model, system, content, Some(1000)),
            format!("Authorization: Bearer {key}"),
        ),
        Provider::Anthropic => (
            format!("{}/messages", config.base_url.trim_end_matches('/')),
            anthropic_body(&config.model, system, content, 1000),
            format!("x-api-key: {key}"),
        ),
    };
    let header_file = create_curl_header(config, &auth_header).ok()?;
    let header_path = header_file.path().to_str()?;
    let output = Command::new("curl")
        .args([
            "-fsS",
            "--connect-timeout",
            "10",
            "--max-time",
            "60",
            "--config",
            header_path,
            "-X",
            "POST",
            &url,
            "-d",
            &body,
        ])
        .output();
    match output {
        Ok(output) if output.status.success() => {
            model_content(&String::from_utf8_lossy(&output.stdout)).ok()
        }
        _ => None,
    }
}

pub(crate) fn model_request(
    root: &Path,
    session: &Session,
    input: &str,
    document: Option<&str>,
    system: &str,
    include_context: bool,
) -> Option<String> {
    let config = model_config(root)?;
    let key_name = match config.provider {
        Provider::OpenAi => "OPENAI_API_KEY",
        Provider::Anthropic => "ANTHROPIC_API_KEY",
    };
    let key = env::var(key_name).unwrap_or_default();
    if key.is_empty() {
        println!("{key_name} is not set.");
        return None;
    }
    let context = if include_context {
        project_context(root)
    } else {
        String::new()
    };
    let history = if include_context {
        session.structured_context()
    } else {
        String::new()
    };
    let prompt = build_prompt(&context, document, &history, input);
    let (url, body, auth_header) = match config.provider {
        Provider::OpenAi => (
            format!("{}/chat/completions", config.base_url.trim_end_matches('/')),
            openai_body(&config.model, system, &prompt, None),
            format!("Authorization: Bearer {key}"),
        ),
        Provider::Anthropic => (
            format!("{}/messages", config.base_url.trim_end_matches('/')),
            anthropic_body(&config.model, system, &prompt, 4096),
            format!("x-api-key: {key}"),
        ),
    };
    let header_file = match create_curl_header(&config, &auth_header) {
        Ok(file) => file,
        Err(error) => {
            println!("Unable to prepare model request headers: {error}");
            return None;
        }
    };
    let header_path = match header_file.path().to_str() {
        Some(path) => path,
        None => {
            println!("Unable to prepare model request headers: invalid temporary path");
            return None;
        }
    };
    let output = Command::new("curl")
        .args([
            "-fsS",
            "--connect-timeout",
            "10",
            "--max-time",
            "120",
            "--config",
            header_path,
            "-X",
            "POST",
            &url,
            "-d",
            &body,
        ])
        .output();
    match output {
        Ok(output) if output.status.success() => {
            match model_content(&String::from_utf8_lossy(&output.stdout)) {
                Ok(content) => Some(content),
                Err(error) => {
                    println!("Model response invalid: {error}");
                    None
                }
            }
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
pub(crate) fn model_content(response: &str) -> Result<String> {
    let payload: serde_json::Value =
        serde_json::from_str(response).context("parse model response as JSON")?;
    if let Some(error) = payload.get("error") {
        let message = error
            .get("message")
            .and_then(serde_json::Value::as_str)
            .or_else(|| error.as_str())
            .unwrap_or("unknown model error");
        return Err(anyhow!("model returned an error: {message}"));
    }
    response_content(&payload)
        .map(normalize_model_content)
        .filter(|content| !content.is_empty())
        .ok_or_else(|| anyhow!("missing non-empty model text content"))
}
