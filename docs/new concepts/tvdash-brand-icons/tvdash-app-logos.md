# tvdash — App logos: YouTube TV, YouTube Music, Tubi, Google Meet (2026-11-22)

Adds or replaces the logos and tile colours for four apps, for every place an app tile appears:

| Surface | UI | Spec it plugs into |
|---|---|---|
| Home **Apps bar** (7 recent + All apps) | Blades / Glass | `tvdash-home-apps-build.md` §7 |
| Home **APPS 2 × 4 grid** | Classic | `tvdash-home-apps-build.md` §14.2 |
| **Apps page** (A–Z, 4 columns) | Blades / Glass | `tvdash-home-apps-build.md` §9 |
| **Apps tab** (A–Z, 5 columns) | Classic | `tvdash-home-apps-build.md` §14.3 |

This doc adds no layout. Tile sizes, focus and ordering stay as in those sections. It only fills in `AppTile { logo, has-logo, color, text-color }` for these four ids and supplies the SVG files.

- Logos come from the Simple Icons project (v16.38.1, the SVG files are CC0). The marks are trademarks of Google (YouTube TV, YouTube Music, Google Meet) and Tubi / Fox. Fine for a personal launcher; check each brand's guidelines before distributing tvdash.
- Tubi's colours are its 2024 identity ("Turple" `#7408FF`, "Hello Yellow" `#FFF200`), which matches the existing Tubi tile in `tvdash-look-and-feel-v2.md`.

---

## 1. Files

```text
assets/logos/
├─ youtube-tv.svg       # replace if present
├─ youtube-music.svg    # new
├─ tubi.svg             # replace if present
└─ google-meet.svg      # new
```

- Each file is **pre-coloured** for its tile (one path; YouTube TV has three layers: white wordmark, red button, white play) and **cropped to the mark** (the viewBox hugs the logo). That matters: the stock icons sit in a 24 × 24 box, so wide wordmarks (YouTube TV, Tubi) would render tiny under `image-fit: contain`. With the cropped viewBox, `contain` fills the logo area as intended.
- The full SVG source for each file is in §5. Copy it in as-is; don't recolour in Slint.
- Bundled at build time like the other logos. **No runtime fetching** (§0.2 H5 of the Home spec).

## 2. Brand table

| id | Display name | Tile `color` | Logo fill (baked into SVG) | `text-color` (fallback name) | Start URL |
|---|---|---|---|---|---|
| `youtube-tv` | YouTube TV | `#000000` | red button `#FF0000`, white play + wordmark `#FFFFFF` | `#FFFFFF` | `https://tv.youtube.com` |
| `youtube-music` | YouTube Music | `#FF0000` | `#FFFFFF` | `#FFFFFF` | `https://music.youtube.com` |
| `tubi` | Tubi | `#7408FF` | `#FFF200` | `#FFF200` | `https://tubitv.com` |
| `google-meet` | Google Meet | `#000000` | `#FBBC04` | `#FBBC04` | `https://meet.google.com` |

- Tile colours are the same in every theme (Dark, Stone, Mist, Kansas Dark, Glass). Brand tiles are the one place strong colour is allowed.
- **YouTube TV vs YouTube vs YouTube Music** must read as three different tiles (Home spec §13): YouTube TV is the black tile with the red button and white wordmark, YouTube Music is the solid red tile with the white ring-and-play glyph, and YouTube keeps its existing tile.
- A–Z order on the Apps pages puts them `YouTube` < `YouTube Music` < `YouTube TV` (already a unit test in the Home spec §10).

## 3. `config.toml`

Add or update these `[[streaming]]` entries. Field names follow the existing entries (H5 in the Home spec); rename `logo` / `color` / `text_color` to whatever the current struct uses. `logo_box` is **new** (§4): optional, default `"glyph"`.

```toml
[[streaming]]
id = "youtube-tv"
name = "YouTube TV"
url = "https://tv.youtube.com"
logo = "youtube-tv.svg"
logo_box = "glyph"
color = "#000000"
text_color = "#FFFFFF"

[[streaming]]
id = "youtube-music"
name = "YouTube Music"
url = "https://music.youtube.com"
logo = "youtube-music.svg"
logo_box = "glyph"
color = "#FF0000"
text_color = "#FFFFFF"

[[streaming]]
id = "tubi"
name = "Tubi"
url = "https://tubitv.com"
logo = "tubi.svg"
logo_box = "wide"
color = "#7408FF"
text_color = "#FFF200"

[[streaming]]
id = "google-meet"
name = "Google Meet"
url = "https://meet.google.com"
logo = "google-meet.svg"
logo_box = "glyph"
color = "#000000"
text_color = "#FBBC04"
```

> Google Meet opens like the other apps (Brave full screen). Camera/mic permission and the call flow belong to `tvdash-google-meet.md`; this doc only adds its tile.

## 4. How each surface draws them

The logo always goes in the tile's logo box with `image-fit: contain`, centred. Because the SVGs are cropped, the two shapes behave differently, which is expected:

| Logo shape | Apps | Fills the box by |
|---|---|---|
| Wide wordmark (≈ 5.4 : 1 and 3.5 : 1) | YouTube TV, Tubi | **width** |
| Square-ish glyph (1 : 1 and 1.2 : 1) | YouTube Music, Google Meet | **height** |

**Optical balance.** At the same box size, wordmarks look much lighter than solid glyphs. So each app has a `logo_box` of `wide` or `glyph`, and the two kinds get different boxes (below): wide logos get more width, glyphs a little less height. **Tubi uses `wide`; YouTube TV stays on `glyph`** (its original size: 70% of the tile width), by choice. Other apps keep the current box (treat them as `glyph`, or tune later).

```slint
// AppTile gains:
logo-wide: bool,   // from config `logo_box = "wide"`
```

### 4.1 Blades / Glass — Home Apps bar (§7)

| Part | Value at 1920 × 1080 |
|---|---|
| Tile | ≈ 152 × 84, radius 16, fill = tile `color` |
| Logo box, wide | **88% × 66%** of the tile (≈ 134 × 55), centred |
| Logo box, glyph | **70% × 54%** (≈ 106 × 45), centred |
| Result | YouTube TV ≈ 106 × 20, Tubi ≈ 134 × 38, YouTube Music ≈ 45 × 45, Meet ≈ 45 × 37 |
| No name label | `accessible-label` = display name |

### 4.2 Blades / Glass — Apps page (§9)

| Part | Value |
|---|---|
| Tile | ≈ 308 × 173, radius 20 |
| Logo box, wide | **88% × 62%** (≈ 271 × 107), centred |
| Logo box, glyph | **70% × 50%** (≈ 216 × 87), centred |
| Name | display name under the tile, 20 / 800 `text` (unchanged) |

### 4.3 Classic — Home APPS grid (§14.2) and Apps tab (§14.3)

Classic tiles carry the **name label at the bottom-left**, so the logo must stay clear of it:

| Part | Value |
|---|---|
| Tile | Classic's existing tile size and radius (2 × 4 on Home, 5 columns on the Apps tab) |
| Logo box | **Wide: 88% of the tile width × 50% of its height. Glyph: 70% × 42%.** Centred in the area above the label row |
| Label | Display name in the tile's `text-color` (as in §2: Tubi and Google Meet yellow, the rest white) |
| Check | Classic uses `text-color`, not a fixed white, for the label |

### 4.4 Missing logo fallback (unchanged)

If a file fails to load, `has-logo = false` and the tile shows the display name, 21 / 900, in `text-color` on the tile `color`.

### 4.5 Done when

- [ ] All four tiles show their logo in the Blades Home bar, Blades Apps page, Classic Home grid and Classic Apps tab.
- [ ] The Tubi wordmark spans its wide box and looks about as heavy as the Music and Meet glyphs; YouTube TV spans 70% of the tile width.
- [ ] YouTube, YouTube Music and YouTube TV are clearly different at a glance.
- [ ] Classic labels use each tile's `text-color`; no logo overlaps a Classic label.
- [ ] Each opens its start URL full screen and moves to the front of the Apps bar next time.

---

## 5. SVG sources

Save each block as the file named in its heading.

### `assets/logos/youtube-tv.svg`

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 9.79 24 4.42"><path fill="#FFFFFF" d="M21.794 10.609h-.618v3.286h-.607v-3.286h-.617v-.504h1.842v.504zm.135-.504h.649l.202 1.703c.057.488.11.908.172 1.484h.026c.057-.576.114-.986.17-1.484l.208-1.703H24l-.623 3.784h-.82l-.628-3.784zm-15.758.38a.796.796 0 0 0-.555-.561c-.493-.13-2.465-.13-2.465-.13s-1.973 0-2.466.13a.787.787 0 0 0-.555.56C0 10.977 0 12 0 12s0 1.028.13 1.516a.794.794 0 0 0 .56.56c.488.13 2.46.13 2.46.13s1.973 0 2.466-.13a.806.806 0 0 0 .56-.555c.13-.493.13-1.516.13-1.516s-.005-1.028-.135-1.52zm-3.649 2.46v-1.89l1.64.945-1.64.945zm6.629.867a.652.652 0 0 1-.265-.395 2.703 2.703 0 0 1-.078-.706v-.4a2.383 2.383 0 0 1 .093-.716.724.724 0 0 1 .28-.394.943.943 0 0 1 .504-.125c.203 0 .369.042.493.125.13.098.229.233.27.394.062.234.094.472.083.716v.4c.005.239-.02.478-.083.711a.7.7 0 0 1-.27.395c-.124.083-.296.124-.508.124-.218-.005-.395-.046-.52-.13zm.7-.436c.037-.089.052-.24.052-.442v-.856a1.376 1.376 0 0 0-.052-.436.189.189 0 0 0-.181-.14.183.183 0 0 0-.177.14c-.041.14-.057.29-.052.436v.861c0 .203.016.353.052.442.02.083.094.14.182.135a.183.183 0 0 0 .176-.14zm8.88-.66v.14c0 .135.006.265.016.4a.373.373 0 0 0 .068.192.2.2 0 0 0 .156.062.2.2 0 0 0 .192-.109c.036-.072.052-.192.057-.363l.534.036a.56.56 0 0 1 .006.1c0 .253-.068.445-.208.57-.14.125-.337.187-.592.187-.306 0-.519-.099-.643-.29s-.187-.489-.187-.894v-.482c0-.416.062-.717.192-.909.13-.192.348-.285.654-.285.213 0 .379.041.493.12.12.088.207.217.244.363.052.223.078.456.067.685v.472l-1.048.005zm.079-1.162a.367.367 0 0 0-.063.192 5.48 5.48 0 0 0-.015.405v.197h.457v-.197c0-.135-.006-.27-.021-.405a.373.373 0 0 0-.068-.192.174.174 0 0 0-.145-.057.169.169 0 0 0-.145.057zm-11.175 1.11l-.706-2.559h.618l.249 1.158c.062.285.109.53.14.732h.02c.022-.146.068-.384.14-.727l.26-1.163h.618l-.716 2.56v1.23H7.64v-1.23h-.005zm4.905-1.536v2.767h-.488L12 13.557h-.016c-.13.255-.332.384-.596.384-.182 0-.317-.062-.405-.181-.089-.12-.13-.307-.13-.566v-2.066h.623v2.03a.659.659 0 0 0 .041.264.137.137 0 0 0 .135.078.28.28 0 0 0 .156-.052.281.281 0 0 0 .109-.124v-2.196h.623zm3.192 0v2.767h-.488l-.052-.338h-.016c-.13.255-.332.384-.596.384-.182 0-.317-.062-.405-.181-.089-.12-.13-.307-.13-.566v-2.066h.623v2.03a.659.659 0 0 0 .041.264.137.137 0 0 0 .135.078.28.28 0 0 0 .156-.052.281.281 0 0 0 .109-.124v-2.196h.623zm-1.5-.52h-.618v3.287h-.607v-3.286h-.618v-.504h1.843v.504zm3.56.961c-.036-.171-.098-.3-.181-.379a.508.508 0 0 0-.348-.12.56.56 0 0 0-.322.1.635.635 0 0 0-.228.254h-.005V9.965h-.597v3.93h.514l.062-.26h.015a.531.531 0 0 0 .218.223.69.69 0 0 0 .322.083.502.502 0 0 0 .467-.29c.099-.192.15-.499.15-.909v-.436a4.861 4.861 0 0 0-.067-.737zm-.57 1.127c0 .155-.006.316-.026.472a.508.508 0 0 1-.084.244.195.195 0 0 1-.155.072.257.257 0 0 1-.14-.036.3.3 0 0 1-.104-.109v-1.567a.425.425 0 0 1 .109-.187.224.224 0 0 1 .166-.073c.057 0 .114.026.145.073.042.078.068.16.073.249.015.166.02.332.02.498l-.005.364z"/><path fill="#FF0000" d="m 6.171,10.485 a 0.796,0.796 0 0,0 -0.555,-0.561 c -0.493,-0.13 -2.465,-0.13 -2.465,-0.13 s -1.973,0 -2.466,0.13 a 0.787,0.787 0 0,0 -0.555,0.56 C 0,10.977 0,12 0,12 s 0,1.028 0.13,1.516 a 0.794,0.794 0 0,0 0.56,0.56 c 0.488,0.13 2.46,0.13 2.46,0.13 s 1.973,0 2.466,-0.13 a 0.806,0.806 0 0,0 0.56,-0.555 c 0.13,-0.493 0.13,-1.516 0.13,-1.516 s -0.005,-1.028 -0.135,-1.52 z"/><path fill="#FFFFFF" d="m 2.522,12.945 l 0,-1.89 l 1.64,0.945 l -1.64,0.945 z"/></svg>
```

### `assets/logos/youtube-music.svg`

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path fill="#FFFFFF" d="M12 0C5.376 0 0 5.376 0 12s5.376 12 12 12 12-5.376 12-12S18.624 0 12 0zm0 19.104c-3.924 0-7.104-3.18-7.104-7.104S8.076 4.896 12 4.896s7.104 3.18 7.104 7.104-3.18 7.104-7.104 7.104zm0-13.332c-3.432 0-6.228 2.796-6.228 6.228S8.568 18.228 12 18.228s6.228-2.796 6.228-6.228S15.432 5.772 12 5.772zM9.684 15.54V8.46L15.816 12l-6.132 3.54z"/></svg>
```

### `assets/logos/tubi.svg`

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 8.56 24 6.88"><path fill="#FFF200" d="M16.696 15.272v-.752c.4.548 1.107.917 1.934.917 1.475 0 2.28-.956 2.28-2.865 0-1.714-.893-2.858-2.235-2.858-.851 0-1.55.347-1.979.908v-2.06h-2.674v6.71zm1.57-2.614c0 .827-.337 1.275-.827 1.275-.486 0-.837-.452-.837-1.275s.342-1.28.837-1.28c.495 0 .828.452.828 1.28zM6.94 9.988v3.6c0 1.236.754 1.841 1.955 1.841.959 0 1.625-.396 2.028-1.064v.91h2.597V9.989h-2.675v3.14c0 .493-.346.693-.666.693-.321 0-.568-.192-.568-.655V9.989Zm14.39 0H24v5.276h-2.67ZM6.553 11.136c0 .781-.635 1.415-1.42 1.415-.783 0-1.419-.634-1.419-1.415 0-.782.636-1.415 1.42-1.415.784 0 1.42.633 1.42 1.415zM3.49 9.702v2.668c.005.653.327.924.976.924.225 0 .526-.053.672-.166v1.931c-.49.243-.869.378-1.535.378 0 0-.069 0-.18-.006l-.003.006c-1.614 0-2.51-1.035-2.482-2.686v-.47H0V9.99h.92V8.563h2.569Z"/></svg>
```

### `assets/logos/google-meet.svg`

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 2.11 24 19.78"><path fill="#FBBC04" d="M5.53 2.13 0 7.75h5.53zm.398 0v5.62h7.608v3.65l5.47-4.45c-.014-1.22.031-2.25-.025-3.46-.148-1.09-1.287-1.47-2.236-1.36zM23.1 4.32c-.802.295-1.358.995-2.047 1.49-2.506 2.05-4.982 4.12-7.468 6.19 3.025 2.59 6.04 5.18 9.065 7.76 1.218.671 1.428-.814 1.328-1.64v-13a.828.828 0 0 0-.877-.825zM.038 8.15v7.7h5.53v-7.7zm13.577 8.1H6.008v5.62c3.864-.006 7.737.011 11.58-.009 1.02-.07 1.618-1.12 1.468-2.07v-2.51l-5.47-4.68v3.65zm-13.577 0c.02 1.44-.041 2.88.033 4.31.162.948 1.158 1.43 2.047 1.31h3.464v-5.62z"/></svg>
```

### Official full-colour sources (optional upgrades)

| App | Where |
|---|---|
| YouTube TV, YouTube Music | Google Partner Marketing Hub (`partnermarketinghub.withgoogle.com`) and `youtube.com/howyoutubeworks/resources/brand-resources/` |
| Google Meet (four-colour camera) | Google Workspace branding ZIP: `support.google.com/a/answer/9212588`. It's multicolour, so it would need a white tile instead of black |
| Tubi | Tubi press / corporate site (`corporate.tubitv.com`) |

## 6. Change log

| Date | Change |
|---|---|
| 2026-11-22 | Google Meet: black tile, yellow `#FBBC04` icon |
| 2026-11-22 | YouTube TV tile black (red button, white wordmark) and back to its original size (`glyph` box); Google Meet tile yellow `#FBBC04` with a dark `#202124` glyph |
| 2026-11-22 | Wordmarks (YouTube TV, Tubi) bumped up: `logo_box = wide` gets a larger box; glyphs slightly smaller, for equal visual weight |
| 2026-11-22 | Logos, tile colours, config entries and per-surface sizing for YouTube TV, YouTube Music, Tubi and Google Meet across Blades/Glass Home bar, Apps page, Classic Home grid and Classic Apps tab |
