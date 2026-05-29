# AI Usage Disclosure

## Tools Used

- **GitHub Copilot (Claude Opus 4.6 via VS Code agent mode)**: Used for project scaffolding, code generation, and design document drafting. All generated code was reviewed, understood, and modified where necessary.

## Specific Usage

1. **Project structure scaffolding**: Copilot generated the initial Cargo workspace setup, Dockerfile configurations, and docker-compose.yml. I directed the architecture decisions (workspace layout, separate binaries for mock PSP vs invoice service).

2. **Crate restructure**: After the initial flat implementation worked, I directed Copilot to restructure into `crates/libs/` + `crates/services/` with lib-core (config, errors, models, BMCs) and lib-auth (middleware, key hashing). I decided which patterns to adopt (OnceLock config, BMC pattern, Ctx) and which to skip (declarative macros, query builders).

3. **Boilerplate code generation**: Handler functions, model structs, and database query patterns were generated with Copilot assistance. Each was reviewed for correctness, especially money handling (ensuring i64 cents throughout) and error handling patterns.

4. **Design document drafting**: The initial DESIGN.md structure was AI-assisted, but all design decisions (state machine transitions, concurrency mechanism choice, timeout values, retry policy numbers) were made by me based on the requirements analysis.

## Four Decisions I Made Myself

1. **5-second PSP timeout (not 10s or 30s)**: AI initially suggested matching a longer timeout. I chose 5s because: the normal PSP response is ~100ms, so 5s is 50x headroom; a 30s hang would be unacceptable UX; and returning 202 Accepted with a pending status gives callers a clear path to resolution.

2. **Row-level lock over optimistic concurrency**: AI suggested optimistic concurrency with a version column. I chose `SELECT ... FOR UPDATE` because: it's simpler to reason about, doesn't require application-level retry loops, and for our access pattern (single invoice lock during payment), the lock contention window is minimal.

3. **BMC pattern without macros**: AI suggested either keeping inline SQL in handlers (simpler) or using declarative macros for CRUD generation. I chose explicit BMC structs with hand-written methods because: macros hurt readability and debuggability in a hiring challenge, the consistent interface (`create`/`get`/`list`) is clear without magic, and special operations (state transitions, payment locking) need explicit methods anyway.

4. **Separate Cargo workspace crates (not a route prefix)**: AI suggested putting the mock PSP as routes in the main service. I chose separate binaries because: it mirrors a real external dependency, makes docker-compose networking realistic, and cleanly separates concerns for testing.

## One Thing AI Got Wrong

The initial webhook retry implementation used `NOW() + (attempts * 5 || ' minutes')::interval` which would give linear backoff (5, 10, 15 min). I corrected this to exponential backoff using `POWER(5, attempts)` to match the design doc's specified intervals (5, 25, 125, 625 minutes). The correction was verified by manually computing the intervals against the retry policy table in DESIGN.md.
