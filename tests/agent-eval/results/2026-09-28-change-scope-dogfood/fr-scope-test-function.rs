fn consumer_scopes_retain_exact_discovery_and_replay_checked_delivery() {
    python(&[
        "tools/change-scope-acceptance.py",
        "--audit",
        "tests/agent-eval/results/2026-09-28-change-scope",
    ]);
}
