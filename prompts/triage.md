# Triage a failed Windows build

You run read-only in Azure Pipelines: you cannot write files or run commands.
Read the test log at `ci-logs/test.log` and the code it points to, then decide:

- `flaky`: the failure depends on timing, load, ordering or the machine, and the
  same code is likely to pass on a rerun. Name the evidence.
- `real`: the code or a test expectation is wrong, and a rerun will fail the same
  way. Name the failing test, the file and line, and the likely cause.
- `infra`: the build never reached the tests (toolchain, disk, network).

Be specific and short. Your final answer must follow the JSON schema you were given.
