# Javora

Javora is an open-source, interactive AI coding agent for Java projects. It is designed as a terminal-first engineering partner that understands project structure, explains implementation plans, and helps developers analyze, change, build, and test code.

## Status

Early development (`0.1.0`). The current scaffold provides an interactive REPL, project context, model configuration, and Maven/Gradle detection. File editing, command approvals, and the full tool loop are next.

## Quick start

```bash
python3 -m venv .venv
source .venv/bin/activate
pip install -e .
export OPENAI_API_KEY="your-key"
javora
```

For Ollama or another OpenAI-compatible endpoint:

```bash
export JAVORA_BASE_URL="http://localhost:11434/v1"
export JAVORA_MODEL="llama3.1"
```

## Vision

Javora aims to become a reliable Java engineering agent for architecture, Spring, JVM, concurrency, performance, databases, testing, refactoring, and code review.
