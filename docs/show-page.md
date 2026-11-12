# tvdash: Show page (seasons and episodes)

Part of the Plex Library (laptop). **Status: built 2026-11-12.** Design: canvas page
**Laptop redesign**, board **Library — Show page: seasons and episodes**.

Opens when you tap a show anywhere in the Library: a Finish a season card, an Added this
week / Watch again poster, Browse all, or a library show in the dashboard's search panel
(the row; its button still plays).

## Layout

- Header: "‹ Library" and "TV / Series › *title*".
- Left (4 cols), in the show's poster tint: backdrop · "TV SERIES · PLEX" · title · year,
  rating, season count, genre · summary · "Up next: S1 · E3 — The Breakwater · 14 min left" ·
  **Resume S1 · E3 on TV** (72 px; **Play … on TV** when nothing's started) · **Play from
  S1 · E1** · **Shuffle episodes**.
- Right (8 cols): the season selector, then that season's episodes.

## Season selector

| Seasons | Selector |
|---|---|
| 1–6 | Full tabs: name and progress ("Watched", "2 of 10 watched", "10 episodes") |
| 7–14 | 72 px pills ("S1"…) with a bar under each; the chosen season written out beside them |
| 15+ | "Season 12 ▾" with "20 seasons · 11 watched"; opens a grid of every season, six across |

It opens on the season with the up-next episode. Specials (season 0) are last, "Specials" /
"Sp". Every tab, pill and cell has the season's name and progress as its accessible label.
Not built yet: "Continue to Season N ›" at the end of a season.

## Episodes

About 120 px rows: 16:9 still (176 × 99) with a tick if watched and a bar if started;
number and title ("03  The Breakwater"; specials S1, S2…); runtime plus time left or
"watched"; a one-line summary; **Play**, or a filled **Resume** on the highlighted episode in
progress.

Up next: the episode in progress, else the first unwatched one after the last watched, else
the first unwatched, else S1 · E1 from the start. Specials only count when there's nothing
else.

## Plex data

`GET /library/metadata/{show}` (details, art), `…/children` (seasons: `index`, `leafCount`,
`viewedLeafCount`), `…/allLeaves` (every episode: `parentIndex`, `index`, `title`,
`summary`, `duration`, `thumb`, `viewOffset`, `viewCount`). Read when the page opens.

Code: `shelves.rs` (rules), `bridge/show.rs`, `ui/show.slint`.
