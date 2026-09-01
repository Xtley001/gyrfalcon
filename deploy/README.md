# Deployment artifacts

Supports `docs/RUNBOOK.md#deployment-phases`. Nothing here replaces that
doc — this is the executable side of it.

| File | Phase | Use |
|---|---|---|
| `Dockerfile` | Phase 0 | Cloud VM, `observe` mode. Defaults to `--halt` — an operator must explicitly drop that flag to do anything else. |
| `gyrfalcon.service` | Phase 1 | Bare-metal, colocated, systemd-managed. Only after Phase 0's `observe` data confirms the adapters predict real liquidations on time (`docs/RUNBOOK.md#first-action`). |

## Before promoting to `live`

Run the readiness-check binary, not a manual read of the checklist in
`docs/RUNBOOK.md#production-readiness-checklist`:

```
cargo run --bin readiness-check -- --config config/gyrfalcon.toml
```

Non-zero exit means at least one item isn't `VERIFIED`. As shipped in this
repo, that's everything requiring live program state, a funded wallet, or
`observe`-mode data this build environment has no way to produce — see
each item's detail line for exactly what's missing and why.
