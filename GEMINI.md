# tvdash Workspace Rules

## Running Tests and Checks
- **Do not run `cargo test` or `cargo check` directly.** Let the user run Cargo builds and tests on the host.
- **Break up test runs by crate after finishing a stage or phase.** Never suggest `cargo test --workspace`, which spikes CPU and memory. Instead, prompt the user with `-j 2` per modified crate, from lightest to heaviest:
  - `cargo test -j 2 -p tvdash-core`
  - `cargo test -j 2 -p tvdash-media`
  - `cargo test -j 2 -p tvdash-discover` (if modified)
  - `cargo test -j 2 -p tvdash-player` (if modified)
  - `cargo test -j 2 -p tvdash-widgets` (if modified)
  - `cargo test -j 2 -p tvdash-events` (if modified)
  - `cargo test -j 2 -p tvdash -- --test-threads=1`

## Live Testing Before Release
- **Always run a live test on the host before bumping the version or building an RPM** (e.g., `cargo run -j 2 -- --windowed --layout tv` or `cargo run -j 2 -- --windowed`).
- **Working on one dashboard UI?** Build only that one; it needs about half the RAM and compiles much faster:
  - `cargo run -j 2 -p tvdash --no-default-features --features ui-orbit -- --windowed --layout tv`
  - Features: `ui-classic`, `ui-blades`, `ui-ps5`, `ui-orbit` (`ui-ps5` is excluded from default builds while its design is reworked). Combine them with commas.
  - **Always do the final live test and the release/RPM build with default features.**
- Verify the changed UI/feature interactively (navigation, layout, focus, playback/controls) and fix any issues before moving to the version bump and RPM build.
- **Editing `extras/back-pill/`?** tvdash loads the extension unpacked with `--load-extension`, so changes require restarting Brave to take effect.

## Releases & Versioning
- **When unit tests and live testing are finished, bump the version number** before building a release/RPM so `dnf upgrade` picks up the new package cleanly.
- Update `Cargo.toml`, `CHANGELOG.md`, `docs/fedora-install.txt`, `docs/status.md`, and `releases/README.md` together.
- **Do not run `cargo build --release` or `./packaging/rpm.sh` directly.** Instead, show the build commands to the user so they can run the release/RPM build on the host:
  - `cargo build --release -j 2`
  - `./packaging/rpm.sh`
  - `cp dist/tvdash-<version>-1.x86_64.rpm releases/`
- **When the user says it's done**, commit the updates (including the new `.rpm` in `releases/`), and let the user run `git push`.

## Nightly Cleanup
- When the user ends the session ("done for the night"), follow [docs/nightly-cleanup.md](docs/nightly-cleanup.md) after the final commit.
