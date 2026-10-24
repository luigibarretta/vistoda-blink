"""Unit tests for scripts/check_ci_local_jobs.py and the real gate alignment."""

import importlib.util
import sys
from pathlib import Path

ROOT = Path(__file__).parents[1]
SPEC = importlib.util.spec_from_file_location(
    "check_ci_local_jobs", ROOT / "scripts/check_ci_local_jobs.py"
)
jobs = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = jobs
SPEC.loader.exec_module(jobs)

AUDIT = jobs.Step(
    jobs.AUDIT_STEP,
    uses="audit-all-lockfiles",
    config={"ecosystems": ["cargo"], "cargo": {"ignore": [{"id": "X-1", "reason": "r"}]}},
)
AUDIT_CALL = f"jobs:\n  audit:\n      - run: node x/ci-local.mjs --ci --only {jobs.AUDIT_STEP}\n"


def step(name: str, *needs: str) -> "jobs.Step":
    return jobs.Step(name, f"echo {name}", needs=needs)


def check(steps: list, workflow_jobs: str) -> list[str]:
    return jobs.check_assignments([*steps, AUDIT], AUDIT_CALL + workflow_jobs, ["X-1"])


def test_the_repository_gate_is_aligned() -> None:
    assert jobs.main() == 0


def test_every_step_owned_by_exactly_one_job_passes() -> None:
    workflow = "  a:\n    steps:\n      - run: node x/ci-local.mjs --ci --only one,two\n"
    assert check([step("one"), step("two")], workflow) == []


def test_unassigned_duplicated_and_unknown_steps_fail() -> None:
    workflow = (
        "  a:\n      - run: node x/ci-local.mjs --ci --only one,ghost\n"
        "  b:\n      - run: node x/ci-local.mjs --ci --only one\n"
    )
    problems = check([step("one"), step("lost")], workflow)
    assert "job a: unknown ci-local step ghost" in problems
    assert "step one runs in 2 CI assignments: a, b" in problems
    assert "step lost runs in no CI job" in problems


def test_a_step_pulled_in_only_through_needs_is_not_an_assignment() -> None:
    workflow = "  a:\n      - run: node x/ci-local.mjs --ci --only build\n"
    assert check([step("install"), step("build", "install")], workflow) == [
        "step install runs in no CI job"
    ]


def test_no_needs_requires_an_earlier_call_in_the_same_job() -> None:
    steps = [step("install"), step("build", "install"), step("e2e", "build")]
    ok = (
        "  a:\n      - run: node x/ci-local.mjs --ci --only install\n"
        "  b:\n      - run: node x/ci-local.mjs --ci --only build\n"
        "      - run: node x/ci-local.mjs --ci --no-needs --only e2e\n"
    )
    assert check(steps, ok) == []
    other_job = ok.replace("--only build\n", "--only build\n  c:\n")
    assert any("--no-needs step e2e needs build" in p for p in check(steps, other_job))


def test_workflow_calls_must_use_ci_and_only() -> None:
    _, problems = jobs.workflow_calls(
        "jobs:\n  a:\n      - run: node x/ci-local.mjs --quick\n"
        "      - run: node x/ci-local.mjs --only a\n"
    )
    assert len(problems) == 2
    assert jobs.workflow_calls("jobs:\n  a:\n      # run `node x/ci-local.mjs --quick`\n") == (
        [],
        [],
    )


def test_the_workflow_must_not_repeat_a_manifest_command() -> None:
    clippy = jobs.Step("clippy", "cargo clippy --locked -- -D warnings")
    workflow = (
        "  a:\n      - run: node x/ci-local.mjs --ci --only clippy\n"
        "      - run: cargo clippy --locked -- -D warnings\n"
    )
    assert check([clippy], workflow) == [
        "workflow repeats the command of step clippy outside ci-local: "
        '"cargo clippy --locked -- -D warnings"'
    ]


def test_the_audit_step_covers_cargo_with_exactly_the_reviewed_ignores() -> None:
    assert jobs.check_audit_step(AUDIT, ["X-1"]) == []
    assert jobs.check_audit_step(AUDIT, ["X-1", "X-2"]) == [
        "step audit-lockfiles ignores ['X-1'] but .cargo/audit.toml reviews ['X-1', 'X-2']"
    ]
    npm_only = jobs.Step(
        jobs.AUDIT_STEP, uses="audit-all-lockfiles", config={"ecosystems": ["npm"]}
    )
    assert jobs.check_audit_step(npm_only, []) == ["step audit-lockfiles does not audit cargo"]
    plain = jobs.Step(jobs.AUDIT_STEP, "cargo audit")
    assert jobs.check_audit_step(plain, []) == ["step audit-lockfiles must use audit-all-lockfiles"]
    assert jobs.check_audit_step(None, []) == ["step audit-lockfiles is missing from the manifest"]
