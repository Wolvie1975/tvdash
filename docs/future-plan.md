# tvdash — Future Iterations Plan (v1.1 → v4)

Companion to `v1-plan.md`. Nothing here changes v1. Pick it up once v1's M1–M6 are done.

Design canvas: https://claude.ai/artifact/UggZzEChLyVVffZNfdGnTf, page **Future iterations**.

> **Scope change — 2026-11-11:** calendar and homelab (Portainer, Uptime Kuma) are removed from tvdash entirely, in v1 and every later version. References below have been taken out.

---

## 1. Roadmap at a glance

| Version | Theme | Headline features |
|---|---|---|
| **v1.1** | Polish + search | Manage streaming sites · Where to watch (TMDB) · colour-coded widgets + light themes · stale/error states · display settings · crossfades + pixel shift · diagnostics |
| **v2** | Ambient | Night mode schedule · alerts strip · severe-weather takeover · headlines · richer "up next" |
| **v3** | Control from the couch | Phone remote · HDMI-CEC remote · next episode + skip intro · audio/subtitle/volume |
| **v4** | Smart home | "What should I watch?" via Ollama · watch stats · Home Assistant · cross-service watchlist |

---

## 2. v1.1 — Manage streaming sites

### What it does

- **Settings › Streaming** lists every site. Drag to reorder (that's the order of the dashboard tiles). Each site has Edit, Hide/Show and Remove.
- **Hidden** sites disappear from the dashboard but still count as "your services" for search, and keep their browser sign-in.
- **Add site** dialog: tile name, start page, optional search page, TMDB providers, **Test on TV**.
- **Remove** asks to confirm, and has an option to also clear that site's cookies from the browser profile.
- Changes are written back to `config.toml` and the tiles reload without a restart.

### Config change

The v1 `[[streaming]]` entries gain four optional fields. v1 configs keep working unchanged.

```toml
[[streaming]]
name = "Hulu"
url = "https://www.hulu.com"
search_url = ""                 # optional; {title} is replaced, URL-encoded
tmdb_providers = [15]           # TMDB provider IDs this site counts as
hidden = false
# array order = tile order
```

```rust
#[derive(Serialize, Deserialize, Clone)]
pub struct StreamingSite {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub search_url: Option<String>,
    #[serde(default)]
    pub tmdb_providers: Vec<u32>,
    #[serde(default)]
    pub hidden: bool,
}
```

> **Don't hard-code provider IDs:** the `15` above is only an example. The add dialog fills `tmdb_providers` by matching the site name against TMDB's US provider list, and you can add more (ad tiers, "… Amazon Channel" variants). Check the IDs against `/3/watch/providers/tv?watch_region=US`.

### Writing config safely

- Write to `config.toml.tmp`, then `rename()` over the original so a crash never leaves a half-written file.
- Keep a single `Config` owner (e.g. `Arc<RwLock<Config>>` in `core`) and emit `AppEvent::StreamingChanged` so the UI rebuilds the tiles.
- Editing with `toml_edit` instead of `toml` keeps any comments you've written in the file.

### Clearing one site's cookies

Chrome stores cookies in an SQLite file inside the profile, which is locked while Chrome runs. V1.1 approach:

1. Close the TV browser if it's open.
2. Offer "Sign out of all sites" (delete the profile directory), **or**
3. For a single site, open `chrome://settings/content/all?searchSubpage=<domain>` on the TV and let you clear it there.

Per-site deletion straight from the app is fragile (the cookie DB is encrypted with the keyring), so keep it manual.

### Done when

- [ ] Add, edit, hide, show, remove and reorder all work and survive a restart.
- [ ] Test on TV opens the URL full screen in the TV profile.
- [ ] Removing a site asks to confirm; the tile disappears without a restart.
- [ ] A v1 config with no new fields loads without errors.

---

## 3. v1.1 — Where to watch

### What it does

Search any movie or show. Results are **grouped by what you can watch right now**:

| Group | Rule | Action |
|---|---|---|
| **In your library** | TMDB ID found in Plex or Jellyfin | **Resume / Play on TV** (mpv) |
| **On your services** | US `flatrate`, `free` or `ads` offer matches one of your sites' `tmdb_providers` | **Open on TV** (site's search page, or its start page) |
| **Costs extra** | Has offers, but only rent/buy, or only on services you don't have | Listed, no action |
| **Not streaming** | No US offers | Listed, no action |

- An **Only what I can watch now** switch hides the last two groups.
- Within each group, TMDB's own relevance order is kept.
- The detail panel repeats the same split for the selected title, and its footer credits JustWatch and links to TMDB.

### New crate

```text
crates/
└─ discover/        # TMDB client, availability cache, ranking
```

`discover` depends on `core` (config, keyring) and on `media` (to look up library matches).

### Keyring

| Key | Holds |
|---|---|
| `tmdb_api_key` | TMDB v4 read access token (Bearer), reused from your other project |

### TMDB endpoints

| Need | Request |
|---|---|
| Search | `GET https://api.themoviedb.org/3/search/multi?query=…&include_adult=false&language=en-US&page=1` |
| Movie availability | `GET /3/movie/{id}/watch/providers` |
| TV availability | `GET /3/tv/{id}/watch/providers` |
| Provider list (add dialog) | `GET /3/watch/providers/movie?watch_region=US` and `/3/watch/providers/tv?watch_region=US` |
| Poster | `https://image.tmdb.org/t/p/w185{poster_path}` |

Send `Authorization: Bearer <token>`. In the availability response, read `results.US`: `flatrate`, `free`, `ads`, `rent`, `buy`, each an array of `{ provider_id, provider_name, … }`, plus `link` (TMDB's watch page for the title).

> **Required:** the availability data comes from JustWatch through TMDB, and TMDB requires apps to credit JustWatch. The design's footer line "Availability in the US from JustWatch, via TMDB" covers this. TMDB can revoke API access if it's missing.

### Library matching

Build a `HashSet<u32>` of TMDB IDs from both servers when the app starts, and refresh it when the library changes.

| Server | Where the TMDB ID lives |
|---|---|
| Plex | `Guid` array on each item, e.g. `{"id": "tmdb://12345"}`. Request with `includeGuids=1` |
| Jellyfin | `ProviderIds.Tmdb` on each item. Request `Fields=ProviderIds` |

Map TMDB ID → `MediaItem` so a match can go straight to **Resume on TV**. For TV shows, match on the **series** TMDB ID, not the episode.

### Ranking

```rust
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub enum Tier { Library, Included, CostsExtra, NotStreaming }

pub fn tier(r: &SearchHit, mine: &HashSet<u32>, library: &HashSet<u32>) -> Tier {
    if library.contains(&r.tmdb_id) {
        return Tier::Library;
    }
    match r.us_offers.as_ref() {
        Some(p) if p.flatrate.iter().chain(&p.free).chain(&p.ads)
            .any(|o| mine.contains(&o.provider_id)) => Tier::Included,
        Some(p) if !(p.flatrate.is_empty() && p.free.is_empty() && p.ads.is_empty()
                     && p.rent.is_empty() && p.buy.is_empty()) => Tier::CostsExtra,
        _ => Tier::NotStreaming,
    }
}

// keep TMDB's relevance order inside each tier
hits.sort_by_key(|h| (tier(h, &mine, &library), h.search_rank));
```

`mine` is the union of `tmdb_providers` across **all** streaming sites, hidden ones included.

### Opening a result on the TV

```rust
fn open_url(site: &StreamingSite, title: &str) -> String {
    match &site.search_url {
        Some(tpl) if tpl.contains("{title}") =>
            tpl.replace("{title}", &urlencoding::encode(title)),
        _ => site.url.clone(),
    }
}
```

TMDB doesn't return deep links into the services. The search-page template is the closest thing, so it's optional per site and you can test it with **Test on TV**.

### Performance and caching

- Search returns up to 20 hits per page; each needs its own watch-providers call. Fetch them in parallel (`futures::future::join_all`) and cap at ~8 concurrent requests.
- Debounce typing by ~300 ms before searching.
- Cache availability per `(media_type, tmdb_id)` for 24 hours in SQLite (`~/.local/share/tvdash/cache.db`). Cache the provider list for a week.
- Show results immediately in a "checking…" state, then move each one into its group as availability arrives (crossfade, no jumping while you're hovering).

### Entry point (open)

The design doesn't yet say where search starts. The suggestion is a search field in the laptop dashboard header. It would search your library and Where to watch together, landing on the Where to watch screen.

### Done when

- [ ] Searching shows grouped results; the switch hides the last two groups.
- [ ] A title in Plex or Jellyfin lands in **In your library** and Resume on TV plays it.
- [ ] A title on one of your services lands in **On your services**, and Open on TV opens that site (search page if set).
- [ ] Hidden sites still count as yours.
- [ ] JustWatch credit is visible on the screen.
- [ ] A repeated search within 24 h makes no availability calls (cache hit).

---

## 4. v1.1 — Colour and themes

> **Direction change in progress:** you're now leaning towards **Option B** (the On the TV card tinted from the poster, everything else neutral). See the canvas page **Laptop redesign**. The light themes (Stone, Mist) and the theme setting below still apply; the Option A role colours will be replaced once the redesign settles.

Design: canvas page **Color** (Option A board and the Light mode board).

### Decisions

- **Option A:** each kind of information has its own colour. Colour goes on icons, labels, dots, tinted cards and buttons; backgrounds stay neutral.
- **Themes:** Settings › Display › Theme = **Follow system / Dark / Stone / Mist**. Stone and Mist are mid-tone light themes with no white surfaces.
- **The TV window always uses Dark**, whatever the laptop theme.
- Status always comes with words ("6 / 6 up", "Sign-in expired"), so a coloured tile is never read as a warning.

> **tvdash decision (2026-11-11):** default theme is **Dark**; the header switch offers Dark / Stone / Mist.

### Base tokens

| Token | Dark | Stone | Mist |
|---|---|---|---|
| `bg` | `#0B0D10` | `#CBC5BA` | `#C3CBD3` |
| `surface` | `#14171C` | `#DAD5CC` | `#D3DAE1` |
| `surface-raised` | `#1B1F26` | `#E3DFD7` | `#DEE3E8` |
| `border` | `#2A2F37` | `#B9B2A6` | `#AEB7C1` |
| `text-primary` | `#E8EAED` | `#1F1D1A` | `#1A1E23` |
| `text-secondary` | `#A0A6AF` | `#47433D` | `#3F464F` |
| `text-muted` | `#8A9099` | `#4E4943` | `#47505A` |
| `on-role` (text on a filled role button) | `#0B0D10` | `#FFFFFF` | `#FFFFFF` |

### Role colours (`fg` / `tint` / `edge`)

| Role | Dark | Stone | Mist |
|---|---|---|---|
| Weather | `#F2B45C` / `#261F12` / `#3A2F1C` | `#6F4800` / `#CFC7B8` / `#BFB299` | `#6F4800` / `#C9CBCA` / `#BAB6A9` |
| Media | `#6EA8FF` / `#142033` / `#1E2E48` | `#194D9F` / `#C7C7C8` / `#AAB3C1` | `#194D9F` / `#C0CCDA` / `#A4B7D0` |
| Jellyfin | `#B49CFF` / `#1E1A2E` / `#3A3354` | `#553AAB` / `#CDC6C9` / `#B9AEC4` | `#553AAB` / `#C6CADC` / `#B4B2D4` |
| Plex | `#E8B04A` / `#241D10` / `#3A2F1C` | `#694B00` / `#CFC7B8` / `#BEB299` | `#694B00` / `#C8CCCA` / `#B8B6A9` |

### Streaming tile hues (`fg` / `tint` / `edge`)

| Tile | Dark | Stone | Mist |
|---|---|---|---|
| Netflix | `#F28BB0` / `#2A1620` / `#3D2230` | `#8D2754` / `#D2C4C0` / `#C7AAAE` | `#8D2754` / `#CCC8D3` / `#C2ADBE` |
| Disney+ | `#6CC4F5` / `#10212C` / `#1A3242` | `#15557D` / `#C6C8C4` / `#A9B5B8` | `#15557D` / `#C0CDD7` / `#A4B9C8` |
| Hulu | `#9AD86A` / `#18230F` / `#263619` | `#385A15` / `#CAC9BA` / `#B2B69E` | `#385A15` / `#C4CDCD` / `#ACBAAE` |
| Prime Video | `#F5A06A` / `#2A1B10` / `#3D2A1A` | `#803E12` / `#D1C6B9` / `#C4AF9E` | `#803E12` / `#CBCACC` / `#BEB3AD` |
| Apple TV+ | `#C7A6FF` / `#211A2E` / `#322845` | `#5F39A0` / `#CEC5C8` / `#BBAEC1` | `#5F39A0` / `#C7CADA` / `#B6B2D1` |
| YouTube TV | `#F58A7A` / `#2A1614` / `#3D221F` | `#8F2D22` / `#D3C4BB` / `#C7ABA2` | `#8F2D22` / `#CCC9CE` / `#C2AFB1` |

Sites added later get the next unused hue, which you can change in Settings › Streaming. Add a `hue` field to `[[streaming]]` for that.

> **Contrast:** every text colour above was checked against its card and tint. The lowest pairing is 4.74:1, above the 4.5:1 minimum. If you add a hue, check it the same way before using it for text.

### Slint shape

```slint
export enum Mode { dark, stone, mist }

export global Theme {
    in-out property <Mode> mode: Mode.dark;
    out property <bool> light: mode != Mode.dark;
    out property <color> bg: mode == Mode.dark ? #0B0D10 : mode == Mode.stone ? #CBC5BA : #C3CBD3;
    // …one property per token in the tables above
}
```

The TV window never reads `Theme.mode`; give it its own fixed dark tokens.

### Follow system

Read the desktop portal's `org.freedesktop.appearance` / `color-scheme` (the `ashpd` crate wraps it and streams changes). `prefer-dark` → Dark; anything else → the light theme picked in Settings (Stone or Mist).

### Done when

- [ ] Theme setting switches the laptop window live, without a restart, and survives a restart.
- [ ] Follow system tracks GNOME's Appearance setting as it changes.
- [ ] The TV window stays dark in every theme.
- [ ] Every widget, tile and source badge uses its role colour in all three themes.

---

## 5. v1.1 — Polish items

| Item | Notes |
|---|---|
| Stale and error states for every widget | Last value shown dimmed with "updated N min ago"; amber dot and a short reason on errors |
| Settings › Display | °F/°C, 12/24 h, seconds on/off |
| Crossfades | ~400 ms when data changes |
| Pixel shift | TV idle screen drifts a few px every few minutes |
| Diagnostics page | Last poll time, last error and log path per service |

---

## 6. v2 — Ambient

| Feature | Approach |
|---|---|
| **Night mode** | Schedule in Settings (e.g. 23:00–06:30). Laptop: dim clock, one line of weather, "Wake for 30 min". TV: idle screen dims further. Any alert restores full brightness. |
| **Alerts strip** | NWS watches and advisories, Plex/Jellyfin unreachable. One line on the laptop footer. |
| **Severe-weather takeover** | NWS `https://api.weather.gov/alerts/active?point={lat},{lon}`. **Warnings** take over the TV full screen and pause mpv; watches and advisories go to the alerts strip. Dismiss on the laptop. Set a `User-Agent` with contact info, as NWS asks. |
| **Headlines** | Read from your Rust news aggregator's SQLite store, or its RSS output. |
| **Up next on TV idle** | Poster art and the next episode from Continue Watching. |

---

## 7. v3 — Control from the couch

| Feature | Approach |
|---|---|
| **Phone remote** | `axum` server inside the app on the LAN, serving one small web page. Pair once by scanning a QR code in Settings (it carries a one-time token). Commands go over a WebSocket to the same command channel the laptop UI uses. |
| **TV remote (HDMI-CEC)** | Most laptop HDMI ports don't carry CEC. Needs a USB-CEC adapter (e.g. Pulse-Eight) and `libcec` / `cec-client`. Check hardware before planning further. |
| **Next episode + skip intro** | Plex: intro/credits markers on the item. Jellyfin: the Intro Skipper plugin's segments. Offer "Skip intro" and an autoplay countdown on the TV overlay. |
| **Tracks and volume** | mpv properties `aid`, `sid`, `volume` over IPC; list tracks with `track-list`. |

---

## 8. v4 — Smart home

| Feature | Approach |
|---|---|
| **"What should I watch?"** | Send your Continue Watching, recent history and library titles to your own Ollama, ask for 3 picks, and show them as cards that go straight to Play or Open on TV. Everything stays on your network. |
| **Watch stats** | Tautulli API for Plex; Jellyfin's Playback Reporting plugin. |
| **Home Assistant** | Call a scene or script when playback starts, pauses or stops (e.g. dim the lights). |
| **Cross-service watchlist** | Add from Where to watch; stored locally in SQLite; each entry shows its current tier. |

---

## 9. Open questions

- [ ] Search entry point: a header search field on the dashboard? (Suggested.)
- [ ] Does your laptop's HDMI port support CEC, or would you buy a USB-CEC adapter?
- [ ] Headlines source: the desktop news aggregator's SQLite database, or MyNewsFeed?
- [ ] Do you run Home Assistant, or is v4's lights idea out?
