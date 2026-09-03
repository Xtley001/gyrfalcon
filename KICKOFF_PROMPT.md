# KICKOFF_PROMPT.md
### Paste this verbatim into a fresh session, with both `files.zip` (the 8 MDs +
### DECISIONS.md) and `gyrfalcon-main.zip` attached. Do not paste it into a
### session that already has other work in it — per `00_SYSTEM_CONTEXT.md §5 R6`,
### one step per session, and this prompt starts at Step 1 of a clean sequence.

---

```
Unzip both attached files: the MD set (files.zip) and the gyrfalcon codebase
(gyrfalcon-main.zip). Before doing anything else:

1. Read 00_SYSTEM_CONTEXT.md fully.
2. Read 06_BUILD_ORDER.md fully.
3. Read the actual gyrfalcon-main source — not just the README, the crates
   themselves — for every crate referenced in 00_SYSTEM_CONTEXT.md §3. Confirm
   or correct that section against what you actually find. If the code has
   moved on from what §3 describes, stop and tell me the diff before doing
   anything else — do not silently build against a stale assumption.

Project: gyrfalcon
Goal for this session: complete Step 1 of 06_BUILD_ORDER.md — route-feasibility
stepping in the sizing logic — and nothing else.

Current build state: nothing from this MD set has been built yet. Everything
in 00_SYSTEM_CONTEXT.md §3 "Done / present" is the pre-existing baseline;
everything in "flagged as unverified" is still unverified.

Locked decisions: everything in 00_SYSTEM_CONTEXT.md §2, without exception.
This includes: Solana mainnet-beta only, no new language/library/simulation
engine, "private mempool" means Jito Block Engine and nothing else gets built
to duplicate that.

Active documents for this session: 00_SYSTEM_CONTEXT.md, 04_STRATEGY_RISK.md
§2, 06_BUILD_ORDER.md Step 1 only. Do not read ahead into later steps and
start building toward them.

Rules — apply without exception, from 00_SYSTEM_CONTEXT.md §5:
R1 Do not invent any field, address, endpoint, or threshold not in these
   documents or the existing codebase. Stop and ask.
R2 Do not resolve ambiguity by guessing. Name it and ask.
R3 Stack is locked. No substitutions, no version upgrades, no "better"
   alternative libraries.
R4 Every address you touch must trace to a cited source. If you cannot
   verify one, mark it NOT VERIFIED and do not wire it in.
R5 Do not modify, refactor, or "clean up" anything outside this step's
   scope. Flag it instead, at the end of your response.
R6 Complete this one step fully before touching anything related to Step 2.
R7 Before writing any code: list every file you will create or modify, list
   every assumption you'd have to make that isn't answered by the MDs, list
   the acceptance criteria you understand for this task (from
   04_STRATEGY_RISK.md §2's testable output). Then stop and wait for my
   confirmation before writing code.
R8 If implementing this as specified would cause a technical problem given
   what you actually find in the code, say so before writing code that
   ships the problem.
R9 If 04_STRATEGY_RISK.md and the actual code disagree in a way that isn't
   just "the code hasn't been built yet," stop and name the conflict.
R10 Any decision you make this session that isn't already in the MDs: flag
    it explicitly as "DECISION MADE: [what] — needs to go in DECISIONS.md,"
    and write the actual DECISIONS.md entry in the format that file
    specifies, don't just mention it in prose.

This is flash-loan-adjacent logic — a sizing bug here doesn't just produce
wrong output, it can size a transaction that either fails to land (wasted
tip) or, worse, succeeds with a repay amount that doesn't match what the
flash loan actually needs repaid. Be exact. Do not approximate the
CU/byte-fit check with a guess — either compute it for real against the
actual instruction set, or tell me you need something you don't have to
compute it for real.

Wait for my confirmation after your R7 output before writing any code.
```
