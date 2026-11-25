# tvdash — macOS-inspired "Stage" dashboard UI (pinned)

Status: **pinned 2026-11-23**. Fully designed; **build spec: `tvdash-macos-stage-build.md`**. Not started in code.

**Design canvas:** https://claude.ai/artifact/7TFUvQwFXoVHZbG36u5FvD (page **macOS-inspired (pinned)**)

A possible Dashboard UI option alongside Classic (Tile hub), Blades and PS5 (Departures was dropped 2026-11-23). It's inspired by macOS **Stage Manager + Spotlight**, with no Apple branding: grey window dots instead of red, yellow and green, Inter instead of SF, and brand-coloured app squares with names instead of real logos.

## Chosen

| Area | Choice |
|---|---|
| Concept | **Option 3 · Stage Manager + Spotlight** (Desktop + widgets and Finder window rejected) |
| Shell | Menu bar across the top (`tvdash` + current section; right: Sleep, Theme, weather, date and clock) · Spotlight-style search bar top centre · **section stack** down the left (the other sections as small window previews) · current section's window on the **centre stage** · **dock** along the bottom |
| Dock | 7 recent apps + All apps (same `apps_bar` data as Classic and Blades) · divider · quick actions (Search on Home, Screensaver, Sleep) |
| Home | What's On window in the centre (title, meta, plot, progress, Resume / From start) · widget column on the right: clock + weather, Next Game, Up next list |
| Library | Library window takes the stage; Home joins the left stack. Window title bar: **TV Series / Movies / Browse all** segmented control + Watching now pill. Focus backdrop + 3-line summary block, **On this page** card (PS5 idea), **Finish the season** as 16:9 cards with episode segments, **Start watching** as posters with numbered badges. Movies: Continue watching (progress) + Recently added (numbered) |
| Apps | Launchpad-style **A–Z** grid of large rounded icons, 6 across, no sort, filter or recency labels |
| Show page | Structure from PS5 **Show A**. Top: "‹ Library" in the window header, kicker, title, meta, fixed 3-line summary, **Resume** + **From S1 · E1**, up-next episode in a Quick Look-style card, up to 4 season cards (5+ → 3 + "+N more ↓"). Scrolled: sticky toolbar, season strip in **rows of 4, max 2 rows, 9+ → 7 + All N seasons** (selected season = light card), 3-across episode grid, "Continue to Season 3 ›" as the last card |
| Sports | Sports window with a **Competitions / Scoreboard** segmented control. Scoreboard: toolbar (‹ date ›, All sports, Refresh, freshness), Live / Upcoming / Final groups of 3-across cards with box scores, **★ Following** tag (no Star), Game ↗. Competitions: Finder **list view** (Time, Event, Sport, TV, Location, Links), grey day bars, striped rows, schedule filter, Earlier / Later paging |
| Collections | Landing: Finder **icon view** of collection posters (name, "8 movies · 3 watched", progress, Smart tag), Icons / List toggle. Collection page: info panel on the left (art, meta, "3 of 8 watched", summary, Up next, **Resume … on TV** + From start; no Shuffle), order segmented control (Collection order / Release date / A–Z) + Hide watched toggle, 4-across numbered posters |
| Search | Spotlight grows into a large panel over the dimmed desktop: 6 × 6 keyboard + filter · Spotlight-style result list (Top hit, In your library, Included with your apps, Costs extra, Not streaming) · preview of the selected result with its actions |
| Settings | System Settings-style window: search field, profile row, coloured section icons (Display, Accounts, Sites, Sports, Server, Diagnostics, What's new); Display with a 4-option **Dashboard UI** thumbnail picker and grouped rows (popup, segmented controls, text field, toggle + Preview) |
| Startup animation | Boot screen (wordmark + progress bar) → hold → wallpaper, menu bar slides down, dock springs up, stack slides in, window scales up, Spotlight drops in, widgets fade in. Approved at the slowed timing (boot 2.0 s, reveal 1.9 s) |
| Widgets on sub-pages | The widget column steps aside so the window can be wider; the clock and weather stay in the menu bar |

## Slint workarounds (agreed 2026-11-23)

Checked against the code at v0.1.68 (Slint 1.18).

| Effect | Slint limit | Approach |
|---|---|---|
| **Frosted glass** (panels, menu bar, dock, widgets) | No live blur of content behind an element | **Pre-blur the wallpaper / backdrop once in Rust** with the existing `image` crate (cache the blurred copy next to the sharp one in `ImageCache`) and draw the blurred copy, clipped to each panel's rect, under a semi-transparent tint. Fallback: tinted panels without blur |
| **Stage Manager previews** (tilted 3D window thumbnails) | 2D `transform-rotation` / `transform-scale` only, no perspective | **Flat previews**: slightly smaller and offset, static icon + section gradient (as in the mockups), not live mini-screens. A pre-rendered skewed image is the option if flat looks too plain |
| Everything else (rounded windows, gradients, shadows, focus rings, scale-on-focus dock, Spotlight `TextInput`, intro animation) | Supported | Same techniques Classic and Blades already use |
