# CI guardrails

Workflow: `.github/workflows/guardrails.yml`.

## Jobs

| Job | Trigger | Environment | Scope |
| --- | --- | --- | --- |
| `source` | pull request, push to main, manual dispatch | GitHub-hosted `macos-26` | Formatting, crate direction, dependency sources/summary, validation-tool tests and report checks. No product binary is built or executed. |
| `native` | manual dispatch only | `[self-hosted, macOS, ARM64, arcshot-macos27]` | Asserts macOS 27 + SDK 27 + arm64, then runs complete `scripts/check-all.sh`, including Rust checks and Debug/Release native smoke. |

GitHub-hosted stable images currently do not provide the required macOS 27 product
baseline. The source job therefore does not claim Rust compilation, tests, docs or
native validation. Keeping those steps in the native job avoids lowering
`MACOSX_DEPLOYMENT_TARGET` or reporting a different SDK as a pass.

The only third-party action is official `actions/checkout`, pinned to the v7.0.1
commit SHA. Dependency fetching is a build-time CI operation; ArcShot's runtime
network prohibition is unchanged.

## Required user setup

1. After the first source run succeeds, require the `source` status check in main's
   branch protection.
2. Register an arm64 macOS 27 self-hosted runner with labels `macOS`, `ARM64` and
   `arcshot-macos27` before enabling native CI for PRs. Runner registration and any
   credentials remain user-managed.
3. Run native validation manually with `gh workflow run guardrails.yml`. Only make
   `native` a required PR check after changing its trigger to include pull requests
   and proving the runner is reliably available.

No runner or branch-protection setting is changed by the repository scripts.
