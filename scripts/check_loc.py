"""Enforce the repository's physical line budget."""

from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MAX_LINES = 250
SUFFIXES = {".json", ".md", ".py", ".toml", ".yaml", ".yml"}
EXCLUDED = {".git", ".pytest_cache", ".ruff_cache", ".venv", "__pycache__"}


def maintained_files():
    """Yield maintained text files without generated environments."""
    for path in ROOT.rglob("*"):
        if (
            path.is_file()
            and path.suffix in SUFFIXES
            and not EXCLUDED.intersection(path.relative_to(ROOT).parts)
        ):
            yield path


def main() -> None:
    """Fail with every over-budget path in deterministic order."""
    violations = []
    for path in sorted(maintained_files()):
        lines = len(path.read_text(encoding="utf-8").splitlines())
        if lines > MAX_LINES:
            violations.append(f"{path.relative_to(ROOT)}: {lines} > {MAX_LINES}")
    if violations:
        raise SystemExit("LOC budget exceeded:\n" + "\n".join(violations))


if __name__ == "__main__":
    main()
