# Nightly Cleanup — tvdash

Run this at the end of every work session, after the final commit, when the user says we're done for the night.

## Goal

Keep the `target/` build directory from filling the HDD, without deleting the compiled dependencies that will be needed again tomorrow.

## Steps

1. **Confirm you're in the tvdash project root.** `Cargo.toml` must be in the current directory. If it isn't, stop and ask.

2. **Make sure nothing is still building.** If a `cargo build`, `cargo run`, `cargo test` or `cargo watch` process is running for this project, stop it first or ask the user.

3. **Record the size before cleanup** (if `target/` doesn't exist, stop and report):
   ```bash
   [ -d target ] || { echo "no target dir"; exit 0; }
   du -sh target
   ```

4. **Make sure cargo-sweep is installed** (if missing and network is unavailable, ask the user to run `cargo install cargo-sweep` on the host):
   ```bash
   command -v cargo-sweep >/dev/null || cargo install cargo-sweep
   ```

5. **Prune stale artifacts:**
   ```bash
   cargo sweep --installed     # artifacts from toolchains that are no longer installed
   cargo sweep --time 7        # anything not used in the last 7 days
   ```

6. **If `target/` is still larger than 15 GB, do a full clean:**
   ```bash
   if [ -d target ]; then
       size_gb=$(du -s --block-size=1G target | cut -f1)
       if [ "$size_gb" -gt 15 ]; then cargo clean; fi
   fi
   ```

7. **Record the size after cleanup** and tell the user in one line, e.g.:
   > Cleanup done: `target/` went from 22G to 6.1G.

## Rules

- Never delete anything outside `target/`. Leave `Cargo.lock`, `~/.cargo`, and all source files alone.
- Don't run cleanup in the middle of a session. Only run it when the user ends the session.
- If any command fails, report the error and stop. Don't fall back to `rm -rf`.
- Do a full `cargo clean` only when step 6 calls for it or the user asks for one.
