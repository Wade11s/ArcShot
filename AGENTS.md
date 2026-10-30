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
