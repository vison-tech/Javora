# Javora

## 中文

Javora 是一个面向 Java 开发者的开源 AI 编程 Agent，采用 Rust 核心引擎和 TypeScript/Node.js CLI，提供类似 Codex CLI 的交互式终端体验。

它能够理解当前项目结构，协助开发者分析代码、设计方案、定位问题，并逐步扩展到代码修改、构建和测试，目标是成为可靠的 Java 技术协作伙伴。

### 当前能力

- 交互式终端对话
- 强制需求确认：每次仅追问一个关键问题，达到 95% 以上把握后先提交方案，用户确认前不读取项目、不修改文件、不执行命令或测试
- 自动读取当前项目文件结构
- OpenAI-compatible 模型接口
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

## English

Javora is an open-source AI coding agent for Java developers. It uses a Rust core engine and a TypeScript/Node.js CLI to provide an interactive terminal experience inspired by Codex CLI.

It understands the current project structure and helps developers analyze code, design implementation plans, investigate issues, and gradually extend into code editing, building, and testing. The goal is to become a reliable engineering partner for Java development.

### Current capabilities

- Interactive terminal REPL
- Mandatory requirements gate: ask one focused question at a time, present a plan after 95%+ confidence, and do not inspect the project, edit files, run commands, or run tests before explicit approval
- Automatic project file context
- OpenAI-compatible model API
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

## Vision

Javora aims to become a reliable Java engineering agent for architecture, Spring, JVM, concurrency, performance, databases, testing, refactoring, and code review.
