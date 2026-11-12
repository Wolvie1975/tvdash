# tvdash: Movie collections

Part of the Plex Library (laptop). **Status: built 2026-11-12.** Design: canvas page
**Laptop redesign**, boards **Library C — Smart shelves** (Movies) and **Library — Collection
page (Movies)**.

## Plex data

| Need | Request | Fields used |
|---|---|---|
| Collections in a movie section | `GET /library/sections/{id}/collections` | `ratingKey`, `title`, `summary`, `thumb`, `art`, `smart` |
| Movies in a collection | `GET /library/collections/{ratingKey}/children` | normal movie fields |

Read with the rest of the library (start, sign-in, every 30 minutes, opening the Library).
Smart collections arrive filled in and behave like hand-made ones. Collection order = the
order `/children` returns (the collection's `collectionSort`). Backdrop: the collection's
`art`, else its first movie's.

## Collections row (Library, Movies)

Replaces Genres on the Movies side (genres for movies are chips in **Browse all**). Header
"COLLECTIONS" and "23 collections"; five 128 px cards across, scrolling sideways for more:
poster (69 × 104), name, "8 movies · 3 watched" ("all watched" when complete, "· smart" for
smart ones), a thin progress bar; the card takes the poster's tint. Order: collections with
something left to watch first, then by the most recently added movie.

## Collection page

- Header: "‹ Library" and "Movies › Collections › *name*".
- Left (4 cols): backdrop · "COLLECTION · PLEX" · name · "8 movies · 1998–2024 · 15 h 40 min
  in total" · progress and "3 of 8 watched" · summary · "Up next: …" · the up-next button
  (72 px) · **Shuffle an unwatched one**.
- Right (8 cols): **Collection order** (#1, #2…), **Release date**, **A–Z** (sort title);
  **Hide watched**; the movies four across (poster with a tick or progress bar, position or
  year, title, "year · watched / unwatched / time left"). Tap plays; hold (or right-click)
  shows the Browse detail panel over the page.

Up-next button: a started movie → **Resume *title* on TV**; else the first unwatched in
collection order → **Play *title* on TV**; all watched → **Play from the start** (first movie).

Code: `crates/app/src/shelves.rs` (rules), `bridge/library.rs`, `ui/collection.slint`.
