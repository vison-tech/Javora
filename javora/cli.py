import sys
from pathlib import Path

from openai import OpenAI

from .config import Settings
from .project import build_command, project_context
from .prompts import SYSTEM_PROMPT


def main() -> None:
    root = Path.cwd()
    settings = Settings.from_env()
    client = OpenAI(api_key=settings.api_key or "ollama", base_url=settings.base_url)
    messages = [{"role": "system", "content": SYSTEM_PROMPT + "\n\n" + project_context(root)}]
    print(f"Javora {__import__('javora').__version__} — {root}")
    print("Type /help for commands, /exit to quit.")
    while True:
        try:
            question = input("\n> ").strip()
        except (EOFError, KeyboardInterrupt):
            print()
            return
        if not question:
            continue
        if question in {"/exit", "/quit"}:
            return
        if question == "/help":
            print("/help  show help\n/exit  quit\n/test  run Maven or Gradle tests")
            continue
        if question == "/test":
            command = build_command(root)
            print(f"Detected test command: {' '.join(command) if command else 'none'}")
            continue
        messages.append({"role": "user", "content": question})
        response = client.chat.completions.create(model=settings.model, messages=messages, stream=True)
        answer = ""
        for chunk in response:
            text = chunk.choices[0].delta.content or ""
            print(text, end="", flush=True)
            answer += text
        print()
        messages.append({"role": "assistant", "content": answer})


if __name__ == "__main__":
    main()
