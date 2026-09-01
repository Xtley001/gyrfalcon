# Phase 0 deployment image (docs/RUNBOOK.md#deployment-phases) — a cloud VM
# running `observe` mode. NOT for Phase 1: bare-metal colocation with kernel
# tuning (CPU pinning, disabled frequency scaling) can't be expressed in a
# container; Phase 1 runs the binary directly on tuned bare metal, see
# deploy/gyrfalcon.service.
#
# Multi-stage build: compile in a full Rust image, run in a minimal one so
# the deployed surface is small.
#
# Build-time note: this workspace currently requires a newer Rust toolchain
# than was available in the sandbox that wrote it (MSRV 1.81, driven by
# klend-interface/solana-sdk — see crates/health/Cargo.toml). The image
# below pins 1.81; bump alongside any future dependency upgrade.
FROM rust:1.81-bookworm AS builder

WORKDIR /build
COPY . .

# Release profile is already tuned for the hot path in the root Cargo.toml
# (lto = true, codegen-units = 1, panic = "abort").
RUN cargo build --release -p gyrfalcon

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

RUN useradd --system --create-home --shell /usr/sbin/nologin gyrfalcon
USER gyrfalcon
WORKDIR /home/gyrfalcon

COPY --from=builder /build/target/release/gyrfalcon /usr/local/bin/gyrfalcon

# Config and the signer keypair are mounted at runtime, never baked into
# the image — see docs/DATA_POLICY.md and .gitignore's exclusion of
# config/gyrfalcon.toml and *.pem. Expect:
#   docker run -v /path/to/gyrfalcon.toml:/home/gyrfalcon/config/gyrfalcon.toml:ro \
#              -v /path/to/keys:/home/gyrfalcon/keys:ro \
#              gyrfalcon --config config/gyrfalcon.toml
ENTRYPOINT ["/usr/local/bin/gyrfalcon"]
CMD ["--config", "config/gyrfalcon.toml", "--halt"]
