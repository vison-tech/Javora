from pathlib import Path


IGNORED = {".git", ".idea", "target", "build", ".gradle", "node_modules"}


def project_context(root: Path, limit: int = 12000) -> str:
    files = []
    for path in sorted(root.rglob("*")):
        if not path.is_file() or any(part in IGNORED for part in path.parts):
            continue
        files.append(str(path.relative_to(root)))
    return "Project root: %s\nFiles:\n%s" % (root, "\n".join(files[:limit]))


def build_command(root: Path) -> list[str] | None:
    if (root / "mvnw").exists() or (root / "pom.xml").exists():
        return ["./mvnw", "test"] if (root / "mvnw").exists() else ["mvn", "test"]
    if (root / "gradlew").exists() or (root / "build.gradle").exists() or (root / "build.gradle.kts").exists():
        return ["./gradlew", "test"] if (root / "gradlew").exists() else ["gradle", "test"]
    return None
