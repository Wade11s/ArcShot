## Agent skills

### Issue tracker

Issues and specs are tracked in GitHub Issues for `Wade11s/ArcShot` using the `gh` CLI. See `docs/agents/issue-tracker.md`.

### Triage labels

The five canonical triage labels are used unchanged. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context layout: root `GLOSSARY.md` and `docs/adr/`. See `docs/agents/domain.md`.

### Source map

See `docs/agents/source-map.md` when locating product code or research/prototype branches.

### Delegation

Only when the current harness is Pi, dispatch subagents with the `arcshot-delegation` skill and use `docs/agents/model-pool.md`. Other harnesses use their native subagent tools and policies; these two Pi-specific documents do not apply.

### Git workflow

Create branches and worktrees, merge, and commit with the `arcshot-git` skill.

### Review

Review changes against `REVIEW.md`; report Standards and Spec separately.

### Testing

See `TEST.md` for test seams and commands, `docs/validation/` for evidence, and `docs/agents/ci.md` for CI scope.

### End-to-end testing

Drive ArcShot yourself with `agent-desktop`; never hand a scripted case to the user because a click, menu command or hotkey could be sent by the agent instead. Load the `agent-desktop` skill, run `agent-desktop skills get platform`, then gate on `agent-desktop status` before driving anything.

The stable MAC-E2E IDs, the preconditions and the observation contract are in `TEST.md` §4. In short: drive the native layer headless (status item, its four menu items, titled windows), read product state from the stderr diagnostics instead of the GPUI tree, and re-check windows with `list-windows --app`. The three GPUI surfaces (permission notice, hotkey settings, frozen overlay) are annotated for accessibility (#53/#54/#55), so drive their `button` nodes by `native_id` or name; `--headed` coordinate clicks never count as acceptance evidence, and `AXPress` only works because each control registers `on_a11y_action`. Global combos go through `press` with an unregistered combo as the negative control; keys meant for one of our windows need `--headed` after the product activates that window. Run E2E serially with a single ArcShot instance, and record evidence with the `agent-desktop status` output, the exact commands and the referenced log lines (see `docs/validation/e2e-agent-desktop.md`).

When a case is blocked by a missing product seam, fix the seam (a text diagnostic line, or `accesskit`-level accessibility) instead of falling back to the user or to coordinate clicking; `TEST.md` §4.5 lists the open gaps.

Ask the user only for macOS TCC approvals and visual-taste judgments (`TEST.md` §4.4). When a case needs those, record it as a limitation rather than asking the user to walk a scripted test.

The entry is `scripts/e2e-agent-desktop.py [--release]`; run it instead of retyping the steps, and keep `TEST.md` §4.3 in step with it when a case changes.
