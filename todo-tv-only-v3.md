# tvdash v3 TV-only implementation plan

## Goal
Build the TV-only layout as a second shell of the same app, with the following baseline decisions:
- Plex-only
- Air mouse / wireless mouse + keyboard shortcut as the input baseline for v3
- Auto sentence-clock after ~5 minutes on Home
- Night mode forced Dark from 22:00–06:00

## Phase 1: Clean assumptions and docs

### 1.1 Remove stale Jellyfin references
- Update deployment/setup docs to reflect Plex-only behavior only.
- Remove any remaining Jellyfin setup flow, product references, or instructions that contradict the current decision.
- Keep the path for Plex sign-in and app/browser setup only.

Acceptance:
- No Jellyfin references remain in deployment docs.
- Project docs consistently say Plex-only.

### 1.2 Align v3 config and defaults
- Document the actual v3 defaults clearly:
  - `layout = "tv"` for single-screen TV-only
  - `layout = "dual"` for current dual-screen path
  - keyboard shortcut/Home command support as the fallback to return to Home
  - Dark night mode forced from 22:00–06:00
  - sentence-clock idle auto-start after ~5 minutes on Home

Acceptance:
- New developer can read docs and know the baseline defaults without ambiguity.

## Phase 2: TV-only shell foundation

### 2.1 Add layout switch
- Support `display.layout = "dual" | "tv"` in config.
- Start a distinct TV-only shell when `layout = "tv"`.
- Keep the existing dual-shell behavior unchanged.

Acceptance:
- `layout = "tv"` launches one full-screen TV window.
- `layout = "dual"` still launches the current dual-screen app layout.

### 2.2 Create tv-only app shell
- Add the TV-only shell structure under the app crate or UI shell layer.
- Separate UI concerns from dual-screen shell code.
- Reuse the same app logic and event model for shared features.

Acceptance:
- TV-only shell uses the same data/state/model layer as dual mode.
- There is no product divergence in playback logic, sign-in flow, or settings behavior.

## Phase 3: Input and focus system

### 3.1 Navigation model
- Build arrow-key / OK / Back navigation for the TV-only shell.
- Support hover-and-focus behavior for mouse users.
- Keep focus ring and default focus state per row.

Acceptance:
- All primary screens can be traversed with the TV remote / keyboard only.
- Mouse users can hover and activate interface elements without touch input.

### 3.2 Rail behavior
- Implement side rail open/close states.
- Keep rail menu items: Home, Library, Apps, Settings, plus theme and Screensaver.
- Search is a top-of-rail action, not part of the menu list.

Acceptance:
- Rail opens/closes clearly and predictably.
- Focus can reach menu items and theme controls without losing place.

## Phase 4: Home + return-to-home path

### 4.1 Home screen layout
- Build the Home screen for TV-only with:
  - On the TV / Up next hero
  - Continue Watching row
  - Your services row
  - side rail and theme control
- Ensure 4K and 1440p differences are handled by layout sizes and arrangement, not separate content logic.

Acceptance:
- Home is the primary TV-only landing screen and remains easy to navigate.

### 4.2 Return-to-Home from playback and browser
- Ensure Home can be reached while mpv or a browser is in full-screen mode.
- Use the keyboard shortcut / app control path as v3 baseline.
- Support the full-screen return flow reliably.

Acceptance:
- Pressing Home while a browser or mpv window is active returns to the app Home screen.

### 4.3 Home controls
- Play/Pause from Home card should work without leaving Home.
- Full-screen action returns to the current playback or streaming site.
- Stop action should cleanly leave playback state if needed.

Acceptance:
- Home controls can operate active playback from the Home layer.

## Phase 5: Playback and overlay

### 5.1 Playback overlay
- Add overlay for content currently playing.
- Show title, progress, end time and basic control information.
- Hide automatically after ~5 seconds without user input.

Acceptance:
- Overlay appears on OK and disappears cleanly after the timeout.

### 5.2 Playback background behavior
- The TV-only shell should support the “video playing behind Home” behavior as designed.
- If needed, pause/resume without leaving the Home layer.

Acceptance:
- Home can remain visible while media continues in the background.

## Phase 6: Theme and idle behavior

### 6.1 Night mode
- Force Dark theme between 22:00 and 06:00.
- This applies regardless of user-selected theme.

Acceptance:
- TV display is in Dark mode during the forced window.

### 6.2 Idle sentence-clock
- Start the sentence-clock automatically after ~5 minutes on Home.
- Keep the manual rail Screensaver button as a second entry point.
- Any input should wake the app and return to the previous screen state.

Acceptance:
- Idle behavior is automatic and predictable on Home.
- Manual activation still works.

## Phase 7: Library / detail pages at TV sizes

### 7.1 Library
- Adapt Library to TV-only, keeping the same content rules as the laptop layout.
- Watching now replaces Tonight’s pick.
- Popular on Republic City remains a full-width row below the shelves.
- Support 4K and 1440p size differences via the same content/layout system.

Acceptance:
- Library page has the correct content model and 10-foot presentation.

### 7.2 Show page
- Adapt the Show page to the TV-only layout with the same content structure.
- Respect the season selector rules, episode list behavior, and controls.

Acceptance:
- Show page is navigable in the TV-only shell using arrows + OK + Back.

### 7.3 Collection page
- Adapt the Collection page to TV-only sizes and arrangement.
- Maintain content ordering and hide-watched controls.

Acceptance:
- Collection page is readable at TV distances and navigable with TV remote input.

## Phase 8: QA and handoff

### 8.1 Acceptance checklist against v3 plan
Check each point from the TV-only plan:
- `layout = "tv"` launches a single full-screen window on the TV
- `layout = "dual"` still works unchanged
- Home and rail are reachable with arrows + OK only
- Home returns from mpv/browser
- Playback overlay appears and hides
- Night-mode hours force Dark
- Library, Show and Collection pages work with arrows + OK + Back and mouse
- Library Watching now / Popular content matches the design

Acceptance:
- All required v3 checklist items are satisfied.

### 8.2 Final product notes
- Summarize any compromises or follow-up items for a later phase.
- Keep v3 focused on the chosen input baseline and the current docs decisions.

Acceptance:
- The v3 build is traceable to the approved design decisions and ready to hand off.
