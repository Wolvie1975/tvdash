# Movie Collections Visibility — Notes for Later

Saved 2026-11-17 (v0.1.8) to revisit why Plex Movie Collections were not showing up in **Library › Movies**.

## Where Collections are rendered
- **TV-only (`crates/app/ui/tv/shell.slint`):** On **Library › Movies** (with **Browse All** off), below `CONTINUE WATCHING` and `RECENTLY ADDED` (requires scrolling/arrowing down).
- **Two-screen (`crates/app/ui/library.slint`):** On **Library › Movies**, below `Continue Watching`, `Recently Added`, and `Popular`.

## Likely causes if `COLLECTIONS` is still missing
1. **Silent JSON deserialization failure (`crates/media/src/plex.rs`, `crates/app/src/controller.rs`):**
   - `controller.rs` calls `plex.collections(&sec.key).await.unwrap_or_default()`, swallowing any error into `[]`.
   - `Metadata` in `crates/media/src/plex.rs` defines `child_count: Option<u32>` and `smart: Option<bool>`. If Plex Media Server returns `"0"` / `"1"` or stringified numbers for `smart` or `childCount`, `serde` fails and returns an empty list.
2. **Only the first Movie library is queried (`crates/app/src/controller.rs`):**
   - `sections.into_iter().find(|s| s.kind == target_type)` only inspects the first `"movie"` section. If collections live in a second Movie library, they are skipped.
3. **30-minute shelves cache (`crates/app/src/controller.rs`):**
   - `plex:shelves:false:...` is cached for `1800s` (30 minutes), so an empty/stale cached result persists until expiry.

## Suggested fix when revisiting
- Log warnings on `plex.collections(...)` errors instead of `.unwrap_or_default()`.
- Use flexible serde helpers for `smart` (accept `bool`, `0`/`1`, `"0"`/`"1"`, `"true"`/`"false"`) and `child_count` (accept integer or string).
- Aggregate collections across all `"movie"` sections (not just `.find(...)`).
- Optionally surface `COLLECTIONS` higher on the **Movies** tab if desired.
