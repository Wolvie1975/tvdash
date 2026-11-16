# Application Updates (Fedora RPM & Future Options)

All of `tvdash`'s user data lives in `~/.config/tvdash/`, `~/.local/share/tvdash/`, and GNOME Keyring — never in `/usr/bin` or `/usr/share`. Upgrading the RPM replaces only the binary, desktop entry, and icon without touching settings, sign-ins, or browser profiles.

---

## 1. Manual RPM Upgrades (Current)

1. Bump `version` in `Cargo.toml` (e.g., `0.1.0` → `0.1.1`).
2. Build the release binary and package the `.rpm`:
   ```bash
   cargo build --release -j 2
   packaging/rpm.sh
   ```
3. Copy `dist/tvdash-<version>-1.<arch>.rpm` to the destination machine and upgrade:
   ```bash
   sudo dnf upgrade ./tvdash-<version>-1.<arch>.rpm
   ```

> **Note:** If a machine previously installed `tvdash` with `packaging/install.sh`, run `packaging/install.sh --uninstall` once so `~/.local/bin/tvdash` does not shadow `/usr/bin/tvdash` on `PATH`.

---

## 2. Automatic Update Options (Future)

### Option A: Fedora COPR (Best for `dnf upgrade`)
- Host RPM builds on [Fedora COPR](https://copr.fedorainfracloud.org/) (triggered by Git tags or source RPM uploads).
- On each destination machine, enable the repo once:
  ```bash
  sudo dnf copr enable <user>/tvdash
  ```
- Regular `sudo dnf upgrade` (and GNOME Software) will pull new `tvdash` releases automatically alongside system updates.

### Option B: Self-Hosted Static DNF Repo (GitHub Pages / Local NAS)
- Generate repo metadata with `createrepo_c` over a directory of `.rpm` files.
- Add a small `.repo` file under `/etc/yum.repos.d/tvdash.repo` on each machine so `dnf upgrade` checks your hosted repo.

### Option C: In-App Update Check
- On startup, query the latest GitHub Release tag and compare against `env!("CARGO_PKG_VERSION")`.
- Show a non-intrusive toast/banner in the UI when a newer version is available.
