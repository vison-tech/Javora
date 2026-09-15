# Javora

## 中文

Javora 是一个面向 Java 开发者的开源 AI 编程 Agent，采用 Rust 核心引擎和 TypeScript/Node.js CLI，提供类似 Codex CLI 的交互式终端体验。

它能够理解当前项目结构，协助开发者分析代码、设计方案、定位问题，并逐步扩展到代码修改、构建和测试，目标是成为可靠的 Java 技术协作伙伴。

### 当前能力

- 交互式终端对话
- 强制需求确认：每次仅追问一个关键问题，达到 95% 以上把握后先提交方案，用户确认前不读取项目、不修改文件、不执行命令或测试
- 项目级任务记忆：自动保存需求、方案和当前节点；重启后展示恢复节点，由用户输入 `/resume` 决定继续
- 自动读取当前项目文件结构
- Java 工程画像：识别 Maven/Gradle、模块、源代码与测试规模，并索引包、类型及常见 Spring 注解以生成符合既有风格的代码
- OpenAI-compatible 模型接口
- 稳定的模型协议处理：安全 JSON 编码、Unicode/转义响应解析、Markdown 协议块兼容和无效响应拒绝
- 执行安全策略：设计文档和代码注释按不可信输入处理，方案版本化，失败保留任务，拒绝符号链接写入并进行写入后校验
- 支持 OpenAI、Ollama 及其他兼容服务
- Maven/Gradle 项目和测试命令识别
- 执行测试前显示命令并请求确认
- 面向 Java 工程实践的专家提示词
- 自动发现和读取 Markdown、纯文本及常见配置格式的设计文档
- 从自然语言中识别设计文档并生成实施计划（代码修改前确认）
- `/implement`：根据设计文档启动实现计划
- 保留最近对话和最近一次命令结果，供下一轮分析使用
- 应用代码变更后可确认执行模型建议的测试命令

### 快速开始

```bash
npm install
npm run build:core
npm link
javora
```

使用 Ollama 或其他 OpenAI-compatible 服务（模型接入功能正在开发中）：

```bash
export JAVORA_BASE_URL="http://localhost:11434/v1"
export JAVORA_MODEL="llama3.1"
```

使用 Anthropic 原生 Messages API：

```bash
export JAVORA_PROVIDER="anthropic"
export ANTHROPIC_API_KEY="your-key"
export JAVORA_MODEL="your-claude-model"
# 可选；默认 https://api.anthropic.com/v1
export JAVORA_BASE_URL="https://api.anthropic.com/v1"
```

首次运行未设置 Provider 或模型时，Javora 会引导选择 Provider、模型和服务地址，并保存非敏感配置到 `.codex/javora-config`。API Key 仅从环境变量读取，不写入配置文件。

启动后可以直接输入：

```text
分析这个项目的订单创建流程
找出 UserService 中潜在的并发问题
根据 docs/order-design.md 实现订单创建功能
/implement 根据 docs/order-design.md 实现订单创建功能
/design
/test
/help
/exit
```

输入 `/new` 可放弃当前需求并开始新的需求澄清。项目目前处于早期开发阶段，完整工具调用循环正在持续完善。

任务恢复状态保存在项目目录下的 `.codex/javora-session.state`，属于本地运行状态，不应提交到 Git。

## English

Javora is an open-source AI coding agent for Java developers. It uses a Rust core engine and a TypeScript/Node.js CLI to provide an interactive terminal experience inspired by Codex CLI.

It understands the current project structure and helps developers analyze code, design implementation plans, investigate issues, and gradually extend into code editing, building, and testing. The goal is to become a reliable engineering partner for Java development.

### Current capabilities

- Interactive terminal REPL
- Mandatory requirements gate: ask one focused question at a time, present a plan after 95%+ confidence, and do not inspect the project, edit files, run commands, or run tests before explicit approval
- Project task memory: saves the requirement, plan, and current node; after restart Javora displays the node and waits for `/resume`
- Automatic project file context
- Java project profiling: detects Maven/Gradle, modules, source/test scale, and indexes packages, types, and common Spring annotations to generate code consistent with the project style
- OpenAI-compatible model API
- Resilient model protocol handling: safe JSON encoding, Unicode/escaped response parsing, Markdown protocol fence support, and invalid-response rejection
- Execution safeguards: design documents and source comments are untrusted input, plans are versioned, failures preserve the task, symlink writes are rejected, and writes are verified afterward
- Support for OpenAI, Ollama, and compatible providers
- Maven and Gradle project/test command detection
- Test execution with confirmation before running commands
- Expert prompt focused on Java engineering practices
- Design document discovery for Markdown, text, and common configuration files
- Natural-language design-to-implementation planning with confirmation before edits
- `/implement` workflow for turning design documents into implementation requests
- Recent conversation and command-result context for follow-up analysis
- Optional execution of model-suggested tests after applying changes

### Quick start

```bash
npm install
npm run build:core
npm link
javora
```

For Ollama or another OpenAI-compatible provider:

```bash
export JAVORA_BASE_URL="http://localhost:11434/v1"
export JAVORA_MODEL="llama3.1"
```

For Anthropic's native Messages API:

```bash
export JAVORA_PROVIDER="anthropic"
export ANTHROPIC_API_KEY="your-key"
export JAVORA_MODEL="your-claude-model"
# Optional; defaults to https://api.anthropic.com/v1
export JAVORA_BASE_URL="https://api.anthropic.com/v1"
```

On first use, when no provider or model is configured, Javora guides the user through choosing a provider, model, and base URL. It saves only non-sensitive settings in `.codex/javora-config`; API keys are read only from environment variables.

Example commands:

```text
Analyze the order creation flow in this project
Find potential concurrency issues in UserService
Implement the feature based on docs/order-design.md
/implement the feature based on docs/order-design.md
/design
/test
/help
/exit
```

Use `/new` to discard the current requirement and begin a new clarification flow. Javora is currently in early development; the complete tool execution loop continues to evolve.

Resume state is stored locally at `.codex/javora-session.state` and should not be committed to Git.

## Vision

Javora aims to become a reliable Java engineering agent for architecture, Spring, JVM, concurrency, performance, databases, testing, refactoring, and code review.
