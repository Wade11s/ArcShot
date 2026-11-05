# CI guardrails

Workflow: `.github/workflows/guardrails.yml`.

## Jobs

| Job | Trigger | Environment | Scope |
| --- | --- | --- | --- |
| `source` | pull request, push to main, manual dispatch | GitHub-hosted `macos-26` | Formatting, crate direction, dependency sources/summary, validation-tool tests and report checks. No product binary is built or executed. |
| `native` | manual dispatch only | `[self-hosted, macOS, ARM64, arcshot-macos27]` | Asserts macOS 27 + SDK 27 + arm64, then runs complete `scripts/check-all.sh`, including Rust checks and Debug/Release native smoke. |
| `e2e` | manual dispatch with `e2e_tier` = `quiet` or `ax` | same self-hosted runner | `scripts/e2e-agent-desktop.py --tier <tier>` only. These tiers refuse real keys, pointer moves, `--headed` and `osascript`, so they cannot take over a shared desktop. `physical` and `full` are never run in CI. |

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

4. The `e2e` job needs the runner process to run inside a logged-in GUI session
   with Accessibility granted to the process that launches `agent-desktop`
   (Screen Recording too when a selected unit captures). A runner installed as a
   background daemon has no such session; the job then fails at the permission
   gate, which is a setup problem and not a product failure. Run it with
   `gh workflow run guardrails.yml -f e2e_tier=quiet`.
5. `e2e` declares `needs: native` so it never shares the desktop with the native
   job's smoke runs. That ordering assumes one runner process; a second runner
   on the same Mac must join the same job queue, because two concurrent jobs
   would break the single-instance rule in `TEST.md` §4.1.

No runner or branch-protection setting is changed by the repository scripts.
