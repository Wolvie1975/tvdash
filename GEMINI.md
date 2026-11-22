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

## Releases & Versioning
- **When testing is finished, bump the version number** before building a release/RPM so `dnf upgrade` picks up the new package cleanly.
- Update `Cargo.toml`, `CHANGELOG.md`, `docs/fedora-install.txt`, `docs/status.md`, and `releases/README.md` together.
- **Do not run `cargo build --release` or `./packaging/rpm.sh` directly.** Instead, show the build commands to the user so they can run the release/RPM build on the host:
  - `cargo build --release -j 2`
  - `./packaging/rpm.sh`
  - `cp dist/tvdash-<version>-1.x86_64.rpm releases/`
- **When the user says it's done**, commit the updates (including the new `.rpm` in `releases/`), and let the user run `git push`.
