from dataclasses import dataclass
import os


@dataclass(frozen=True)
class Settings:
    model: str = "gpt-4o-mini"
    base_url: str | None = None
    api_key: str | None = None

    @classmethod
    def from_env(cls) -> "Settings":
        return cls(
            model=os.getenv("JAVORA_MODEL", "gpt-4o-mini"),
            base_url=os.getenv("JAVORA_BASE_URL") or None,
            api_key=os.getenv("OPENAI_API_KEY"),
        )
