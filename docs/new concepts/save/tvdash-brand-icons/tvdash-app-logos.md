# tvdash — App logos for every app tile (2026-11-22)

Logos and tile colours for all 11 apps on the Apps page, for every place an app tile appears:

| Surface | UI | Spec it plugs into |
|---|---|---|
| Home **Apps bar** (7 recent + All apps) | Blades / Glass | `tvdash-home-apps-build.md` §7 |
| Home **APPS 2 × 4 grid** | Classic | `tvdash-home-apps-build.md` §14.2 |
| **Apps page** (A–Z, 4 columns) | Blades / Glass | `tvdash-home-apps-build.md` §9 |
| **Apps tab** (A–Z, 5 columns) | Classic | `tvdash-home-apps-build.md` §14.3 |

This doc adds no layout. Tile sizes, focus and ordering stay as in those sections. It only fills in `AppTile { logo, has-logo, color, text-color, logo-wide }` and supplies the SVG files.

- Logos come from the Simple Icons project (CC0 SVG files): v16.38.1, plus v7.21.0 for Hulu and Prime Video (later removed from the project). The marks are trademarks of their owners (Apple, Disney, Fandango, Google, Hulu/Disney, Netflix, Amazon, Tubi/Fox). Fine for a personal launcher; check each brand's guidelines before distributing tvdash.
- **Disney+** is a transparent PNG cut from the image you supplied (§1.1); everything else is SVG.

### Fixes seen on the current Apps page (screenshot 2026-11-22)

| Problem | Fix |
|---|---|
| Logos are **square app icons with their own background**, boxed inside a tile of a different colour (Apple TV, Disney+, Fandango, Hulu, Netflix, Prime Video, YouTube, YouTube TV) | Replace with the transparent, cropped SVGs here. The tile colour *is* the background now |
| Logos sit in a small fixed square (~130 × 130 on a 456 × 256 tile at 1440p) | Draw in the logo box from §4 with `image-fit: contain` |
| YouTube TV still shows the old white-square icon | Use `youtube-tv.svg` from §5 |
| **Google Meet is missing** from the page | Add its `[[streaming]]` entry (§3); 11 apps total |
| Header count / heading not visible at the top of the page (an `Apps` heading is clipped at the bottom) | Check the page header from Home spec §9 renders above the grid (`Apps` + `11 apps`) |

---

## 1. Files

```text
assets/logos/
├─ apple-tv.svg                 # replace / add
├─ disney-plus.png              # new (from your image, §1.1)
├─ fandango-at-home.svg         # replace / add
├─ google-meet.svg              # replace / add
├─ hulu.svg                     # replace / add
├─ netflix.svg                  # replace / add
├─ prime-video.svg              # replace / add
├─ tubi.svg                     # replace / add
├─ youtube.svg                  # replace / add
├─ youtube-music.svg            # replace / add
└─ youtube-tv.svg               # replace / add
```

- Each file is **transparent, pre-coloured** for its tile and **cropped to the mark** (viewBox hugs the logo), so `image-fit: contain` fills the logo box. YouTube and YouTube TV have extra layers (red button, white play; white wordmark).
- Full SVG source for each file is in §5. Copy as-is; don't recolour in Slint.
- Bundled at build time. **No runtime fetching** (Home spec §0.2 H5).

### 1.1 Disney+

Disney+ is a **PNG**, not an SVG: cut out of the image Jason supplied (2026-11-22), with the blue background removed so the white wordmark and the cyan arc sit straight on the tile.

- File: `assets/logos/disney-plus.png`, transparent, 879 × 489 (3× upscale of a ~293 × 163 source).
- Sharp at 1080p and 1440p. At 4K on the Apps page it's drawn ~2× its source size, so it may look slightly soft next to the SVGs; swap in an official SVG later if that bothers you (same file name with `.svg`, update `logo`).
- Slint loads PNG the same way (`@image-url` / `Image::load_from_path`), so nothing else changes.

## 2. Brand table

| id | Display name | Tile `color` | Logo fill (baked into SVG) | `text-color` | `logo_box` | Start URL |
|---|---|---|---|---|---|---|
| `apple-tv` | Apple TV | `#1C1C1E` | `#FFFFFF` | `#FFFFFF` | `glyph` | `https://tv.apple.com` |
| `disney-plus` | Disney+ | `#0B2E6B` | white wordmark + cyan arc (PNG, §1.1) | `#FFFFFF` | `wide` | `https://www.disneyplus.com` |
| `fandango-at-home` | Fandango at Home | `#1B2A5C` | `#FF7300` | `#FF7300` | `glyph` | `https://athome.fandango.com` |
| `google-meet` | Google Meet | `#000000` | `#FBBC04` | `#FBBC04` | `glyph` | `https://meet.google.com` |
| `hulu` | Hulu | `#16191E` | `#1CE783` | `#1CE783` | `glyph` | `https://www.hulu.com` |
| `netflix` | Netflix | `#121212` | `#E50914` | `#E50914` | `glyph` | `https://www.netflix.com` |
| `prime-video` | Prime Video | `#1A8CFF` | `#FFFFFF` | `#FFFFFF` | `glyph` | `https://www.primevideo.com` |
| `tubi` | Tubi | `#7408FF` | `#FFF200` | `#FFF200` | `wide` | `https://tubitv.com` |
| `youtube` | YouTube | `#F1F1F1` | red button `#FF0000`, white play | `#1F1F1F` | `glyph` | `https://www.youtube.com` |
| `youtube-music` | YouTube Music | `#FF0000` | `#FFFFFF` | `#FFFFFF` | `glyph` | `https://music.youtube.com` |
| `youtube-tv` | YouTube TV | `#000000` | red button `#FF0000`, white play + wordmark | `#FFFFFF` | `glyph` | `https://tv.youtube.com` |

- Tile colours are the same in every theme (Dark, Stone, Mist, Kansas Dark, Glass). Brand tiles are the one place strong colour is allowed.
- **YouTube / YouTube Music / YouTube TV** read as three different tiles: light tile with the red button; solid red with the white ring glyph; black with the red button and white wordmark.
- YouTube TV and Google Meet are both black tiles; their logos (red/white wordmark vs yellow camera) keep them apart.
- A–Z: `Apple TV, Disney+, Fandango at Home, Google Meet, Hulu, Netflix, Prime Video, Tubi, YouTube, YouTube Music, YouTube TV`.

## 3. `config.toml`

Field names follow the existing entries (Home spec H5); rename `logo` / `color` / `text_color` to whatever the current struct uses. `logo_box` is **new** (§4): optional, default `"glyph"`.

```toml
[[streaming]]
id = "apple-tv"
name = "Apple TV"
url = "https://tv.apple.com"
logo = "apple-tv.svg"
logo_box = "glyph"
color = "#1C1C1E"
text_color = "#FFFFFF"

[[streaming]]
id = "disney-plus"
name = "Disney+"
url = "https://www.disneyplus.com"
logo = "disney-plus.png"
logo_box = "wide"
color = "#0B2E6B"
text_color = "#FFFFFF"

[[streaming]]
id = "fandango-at-home"
name = "Fandango at Home"
url = "https://athome.fandango.com"
logo = "fandango-at-home.svg"
logo_box = "glyph"
color = "#1B2A5C"
text_color = "#FF7300"

[[streaming]]
id = "google-meet"
name = "Google Meet"
url = "https://meet.google.com"
logo = "google-meet.svg"
logo_box = "glyph"
color = "#000000"
text_color = "#FBBC04"

[[streaming]]
id = "hulu"
name = "Hulu"
url = "https://www.hulu.com"
logo = "hulu.svg"
logo_box = "glyph"
color = "#16191E"
text_color = "#1CE783"

[[streaming]]
id = "netflix"
name = "Netflix"
url = "https://www.netflix.com"
logo = "netflix.svg"
logo_box = "glyph"
color = "#121212"
text_color = "#E50914"

[[streaming]]
id = "prime-video"
name = "Prime Video"
url = "https://www.primevideo.com"
logo = "prime-video.svg"
logo_box = "glyph"
color = "#1A8CFF"
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
id = "youtube"
name = "YouTube"
url = "https://www.youtube.com"
logo = "youtube.svg"
logo_box = "glyph"
color = "#F1F1F1"
text_color = "#1F1F1F"

[[streaming]]
id = "youtube-music"
name = "YouTube Music"
url = "https://music.youtube.com"
logo = "youtube-music.svg"
logo_box = "glyph"
color = "#FF0000"
text_color = "#FFFFFF"

[[streaming]]
id = "youtube-tv"
name = "YouTube TV"
url = "https://tv.youtube.com"
logo = "youtube-tv.svg"
logo_box = "glyph"
color = "#000000"
text_color = "#FFFFFF"
```

> Google Meet opens like the other apps (Brave full screen). Camera/mic and the call flow belong to `tvdash-google-meet.md`.

## 4. How each surface draws them

The logo goes in the tile's logo box with `image-fit: contain`, centred. Two box sizes:

| `logo_box` | Apps | Why |
|---|---|---|
| `glyph` (default) | everything except Tubi and Disney+ | Standard box |
| `wide` | Tubi, Disney+ | Bigger box so these lighter wordmarks match the others' weight |

```slint
// AppTile gains:
logo-wide: bool,   // from config `logo_box = "wide"`
```

### 4.1 Blades / Glass — Home Apps bar (§7)

| Part | Value at 1920 × 1080 |
|---|---|
| Tile | ≈ 152 × 84, radius 16, fill = tile `color` |
| Logo box, glyph | **70% × 54%** (≈ 106 × 45), centred |
| Logo box, wide | **88% × 66%** (≈ 134 × 55), centred |
| No name label | `accessible-label` = display name |

### 4.2 Blades / Glass — Apps page (§9)

| Part | Value |
|---|---|
| Tile | ≈ 308 × 173 at 1080p (≈ 456 × 256 at 1440p), radius 20 |
| Logo box, glyph | **70% × 50%**, centred |
| Logo box, wide | **88% × 62%**, centred |
| Name | display name under the tile, 20 / 800 `text` (unchanged) |

### 4.3 Classic — Home APPS grid (§14.2) and Apps tab (§14.3)

Classic tiles carry the **name label at the bottom-left**, so the logo stays clear of it:

| Part | Value |
|---|---|
| Tile | Classic's existing tile size and radius |
| Logo box | **Glyph: 70% of tile width × 42% of height. Wide: 88% × 50%.** Centred in the area above the label row |
| Label | Display name in the tile's `text-color` (§2) |
| Check | Classic uses `text-color`, not a fixed white (YouTube's light tile needs `#1F1F1F`) |

### 4.4 Missing logo fallback (unchanged)

If a file fails to load, `has-logo = false` and the tile shows the display name, 21 / 900, in `text-color` on the tile `color`.

### 4.5 Done when

- [ ] All 11 apps appear on the Apps page, A–Z, Google Meet included.
- [ ] No logo shows its own square background; every logo sits directly on its tile colour.
- [ ] Logos fill the §4 boxes (not a small fixed square).
- [ ] YouTube, YouTube Music and YouTube TV are clearly different at a glance.
- [ ] Classic labels use each tile's `text-color`; no logo overlaps a Classic label.
- [ ] Each opens its start URL full screen and moves to the front of the Apps bar next time.

---

## 5. SVG sources

Save each block as the file named in its heading. (Disney+ is a PNG: take it from the zip's `logos/` folder.)

### `assets/logos/apple-tv.svg`

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 6.12 24 11.76"><path fill="#FFFFFF" d="M20.57 17.735h-1.815l-3.34-9.203h1.633l2.02 5.987c.075.231.273.9.586 2.012l.297-.997.33-1.006 2.094-6.004H24zm-5.344-.066a5.76 5.76 0 0 1-1.55.207c-1.23 0-1.84-.693-1.84-2.087V9.646h-1.063V8.532h1.121V7.081l1.476-.602v2.062h1.707v1.113H13.38v5.805c0 .446.074.75.214.932.14.182.396.264.75.264.207 0 .495-.041.883-.115zm-7.29-5.343c.017 1.764 1.55 2.358 1.567 2.366-.017.042-.248.842-.808 1.658-.487.71-.99 1.418-1.79 1.435-.783.016-1.03-.462-1.93-.462-.89 0-1.17.445-1.913.478-.758.025-1.344-.775-1.838-1.484-.998-1.451-1.765-4.098-.734-5.88.51-.89 1.426-1.451 2.416-1.46.75-.016 1.468.512 1.93.512.461 0 1.327-.627 2.234-.536.38.016 1.452.157 2.136 1.154-.058.033-1.278.743-1.27 2.219M6.468 7.988c.404-.495.685-1.18.61-1.864-.585.025-1.294.388-1.723.883-.38.437-.71 1.138-.619 1.806.652.05 1.328-.338 1.732-.825Z"/></svg>
```

### `assets/logos/fandango-at-home.svg`

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0.05 24 23.9"><path fill="#FF7300" d="M13.664 6.956L8.05 8.496 9.19 12.72l5.615-1.54L15.95 15.4l-5.615 1.49 1.093 4.224-5.615 1.49L4.42 17.54c.846-.995 1.194-2.386.846-3.728-.398-1.342-1.392-2.385-2.584-2.832L1.29 5.763 12.57 2.78zm7.106-.198L18.932.05 0 5.068l1.838 6.758c1.093.2 2.087 1.043 2.385 2.236.348 1.193-.1 2.385-.944 3.18l1.788 6.708L24 18.882l-1.79-6.708c-1.142-.2-2.086-1.043-2.434-2.236-.298-1.193.1-2.435.994-3.18z"/></svg>
```

### `assets/logos/google-meet.svg`

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 2.11 24 19.78"><path fill="#FBBC04" d="M5.53 2.13 0 7.75h5.53zm.398 0v5.62h7.608v3.65l5.47-4.45c-.014-1.22.031-2.25-.025-3.46-.148-1.09-1.287-1.47-2.236-1.36zM23.1 4.32c-.802.295-1.358.995-2.047 1.49-2.506 2.05-4.982 4.12-7.468 6.19 3.025 2.59 6.04 5.18 9.065 7.76 1.218.671 1.428-.814 1.328-1.64v-13a.828.828 0 0 0-.877-.825zM.038 8.15v7.7h5.53v-7.7zm13.577 8.1H6.008v5.62c3.864-.006 7.737.011 11.58-.009 1.02-.07 1.618-1.12 1.468-2.07v-2.51l-5.47-4.68v3.65zm-13.577 0c.02 1.44-.041 2.88.033 4.31.162.948 1.158 1.43 2.047 1.31h3.464v-5.62z"/></svg>
```

### `assets/logos/hulu.svg`

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 8.04 24 7.92"><path fill="#1CE783" d="M14.707 15.957h1.912V8.043h-1.912zm-3.357-2.256a.517.517 0 01-.512.511H9.727a.517.517 0 01-.512-.511v-3.19H7.303v3.345c0 1.368.879 2.09 2.168 2.09h1.868c1.189 0 1.912-.856 1.912-2.09V10.51h-1.912c.01 0 .01 3.09.01 3.19zm10.75-3.19v3.19a.517.517 0 01-.512.511h-1.112a.517.517 0 01-.511-.511v-3.19h-1.912v3.345c0 1.368.878 2.09 2.167 2.09h1.868c1.19 0 1.912-.856 1.912-2.09V10.51zm-18.32 0H2.557c-.434 0-.645.11-.645.11V8.044H0v7.903h1.9v-3.179c0-.278.234-.511.512-.511h1.112c.278 0 .511.233.511.511v3.19h1.912v-3.446c0-1.445-.967-2-2.167-2Z"/></svg>
```

### `assets/logos/netflix.svg`

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="5.39 0 13.22 24"><path fill="#E50914" d="m5.398 0 8.348 23.602c2.346.059 4.856.398 4.856.398L10.113 0H5.398zm8.489 0v9.172l4.715 13.33V0h-4.715zM5.398 1.5V24c1.873-.225 2.81-.312 4.715-.398V14.83L5.398 1.5z"/></svg>
```

### `assets/logos/prime-video.svg`

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 8.31 24 7.38"><path fill="#FFFFFF" d="M0 9.508c0-.043.01-.073.028-.09.018-.017.047-.025.086-.025h.329c.07 0 .112.034.127.101l.032.119c.091-.088.202-.159.33-.21a1.04 1.04 0 0 1 .396-.079c.294 0 .528.109.7.326.171.217.257.51.257.88 0 .254-.042.475-.127.665-.086.19-.201.335-.347.437a.85.85 0 0 1-.502.154c-.125 0-.243-.02-.355-.06a.857.857 0 0 1-.288-.164v1.003c0 .043-.008.073-.025.09-.017.016-.046.025-.09.025H.115c-.04 0-.068-.009-.086-.025-.019-.017-.028-.047-.028-.09zm1.113.32a.868.868 0 0 0-.447.124v1.206a.834.834 0 0 0 .447.124c.17 0 .296-.058.376-.174.081-.117.121-.3.121-.55 0-.254-.04-.439-.118-.555-.08-.116-.206-.174-.379-.174zm2.248-.087c.121-.134.236-.23.344-.286a.733.733 0 0 1 .345-.085h.063c.043 0 .073.009.092.025.018.017.027.047.027.09v.385c0 .04-.008.068-.025.087-.017.018-.046.027-.089.027a.923.923 0 0 1-.082-.004 1.369 1.369 0 0 0-.383.025c-.1.02-.186.045-.256.076v1.54c0 .04-.008.069-.025.087-.016.018-.046.028-.089.028h-.437c-.04 0-.069-.01-.087-.028-.018-.018-.028-.047-.028-.087V9.508c0-.043.01-.073.028-.09.018-.017.047-.025.087-.025h.328c.07 0 .112.034.128.1zm1.526-.71a.396.396 0 0 1-.278-.096.338.338 0 0 1-.105-.262c0-.11.035-.197.105-.26a.395.395 0 0 1 .278-.097c.116 0 .208.032.278.096.07.064.105.151.105.261a.34.34 0 0 1-.105.262.396.396 0 0 1-.278.096zm-.333.477c0-.043.01-.073.027-.09.019-.017.048-.025.087-.025h.438c.043 0 .072.008.089.025s.025.047.025.09v2.113c0 .04-.008.069-.025.087-.017.018-.046.028-.09.028h-.437c-.04 0-.068-.01-.087-.028-.018-.018-.027-.047-.027-.087zm1.837.11c.161-.107.306-.183.435-.227.13-.045.263-.067.4-.067.273 0 .466.098.579.294.155-.104.3-.18.438-.225.137-.046.278-.069.424-.069.213 0 .377.06.495.179.117.12.175.286.175.5v1.618c0 .04-.008.069-.025.087-.017.019-.046.027-.089.027h-.438c-.04 0-.068-.008-.086-.027-.018-.018-.028-.047-.028-.087V10.15c0-.208-.092-.312-.278-.312-.164 0-.33.04-.497.119v1.664c0 .04-.008.069-.025.087-.017.019-.046.027-.09.027h-.437c-.04 0-.068-.008-.086-.027-.019-.018-.028-.047-.028-.087V10.15c0-.208-.093-.312-.278-.312-.17 0-.337.04-.502.123v1.66c0 .04-.008.069-.025.087-.017.019-.046.027-.089.027h-.438c-.039 0-.068-.008-.086-.027-.018-.018-.027-.047-.027-.087V9.508c0-.043.009-.073.027-.09.018-.017.047-.025.086-.025h.329c.07 0 .112.034.128.101zm4.387 1.16a1.81 1.81 0 0 1-.451-.05c.018.204.08.35.185.44.105.088.263.132.476.132.085 0 .168-.005.249-.016a3.08 3.08 0 0 0 .362-.078.143.143 0 0 1 .023-.002c.052 0 .078.035.078.105v.211c0 .049-.007.083-.02.103a.169.169 0 0 1-.08.053 1.953 1.953 0 0 1-.708.128c-.377 0-.666-.103-.868-.312-.203-.207-.304-.505-.304-.893 0-.398.104-.71.31-.935.207-.227.494-.34.862-.34.283 0 .504.069.664.206a.69.69 0 0 1 .24.55c0 .23-.087.403-.258.52-.172.119-.425.177-.76.177zm.064-.99c-.292 0-.46.18-.506.54.122.025.257.037.406.037.155 0 .267-.024.337-.071.07-.047.105-.12.105-.218 0-.193-.114-.289-.342-.289zm2.948 1.946a.21.21 0 0 1-.075-.011.119.119 0 0 1-.05-.037.274.274 0 0 1-.038-.071l-.777-2.04a1.863 1.863 0 0 1-.023-.063.162.162 0 0 1-.009-.05c0-.047.03-.07.091-.07h.454c.049 0 .084.01.107.028.023.018.04.049.052.092l.468 1.622.477-1.622a.175.175 0 0 1 .052-.092c.023-.018.058-.027.107-.027h.44c.061 0 .091.022.091.068a.16.16 0 0 1-.009.05l-.022.065-.777 2.039a.274.274 0 0 1-.039.07.122.122 0 0 1-.047.038.207.207 0 0 1-.078.01zm2.02-2.703a.393.393 0 0 1-.277-.097.338.338 0 0 1-.105-.26c0-.11.035-.198.105-.262a.393.393 0 0 1 .277-.096c.115 0 .207.032.277.096.07.064.104.151.104.261 0 .11-.034.197-.104.261a.393.393 0 0 1-.277.097zm-.218 2.703c-.04 0-.068-.01-.086-.028-.019-.018-.028-.047-.028-.087V9.507c0-.043.01-.072.028-.09.018-.016.047-.024.086-.024h.436c.042 0 .072.008.089.025.016.017.024.046.024.09v2.111c0 .04-.008.07-.024.087-.017.019-.047.028-.09.028zm1.948.05a.869.869 0 0 1-.513-.153.97.97 0 0 1-.334-.426 1.6 1.6 0 0 1-.116-.63c0-.38.09-.682.268-.91a.856.856 0 0 1 .709-.341.98.98 0 0 1 .622.206V8.458c0-.043.01-.073.027-.09.018-.016.047-.025.087-.025h.436c.042 0 .071.009.088.025.017.017.025.047.025.09v3.161c0 .04-.008.07-.025.087-.017.019-.046.028-.088.028h-.364a.135.135 0 0 1-.084-.023.137.137 0 0 1-.043-.078l-.027-.105a.958.958 0 0 1-.668.256zm.218-.504a.762.762 0 0 0 .418-.128v-1.21a.872.872 0 0 0-.45-.114c-.16 0-.28.06-.358.18-.08.121-.118.304-.118.548 0 .245.041.426.124.546.084.119.212.178.384.178zm2.588-.51c-.169 0-.315-.016-.44-.05.018.201.078.345.18.432.103.087.257.13.465.13.083 0 .164-.005.242-.016a2.997 2.997 0 0 0 .354-.076.135.135 0 0 1 .022-.002c.05 0 .075.035.075.103v.207c0 .048-.007.082-.02.101a.165.165 0 0 1-.077.052 1.895 1.895 0 0 1-.69.126c-.367 0-.65-.102-.846-.306-.197-.204-.296-.496-.296-.876 0-.39.1-.695.302-.917.202-.222.482-.333.84-.333.276 0 .492.068.647.203a.678.678 0 0 1 .234.539c0 .225-.084.395-.251.51-.168.115-.415.173-.74.173zm.063-.97c-.285 0-.45.176-.494.53.119.024.25.036.396.036.15 0 .26-.024.329-.07.068-.046.102-.117.102-.213 0-.19-.111-.284-.333-.284zm2.442 2.003c-.36 0-.642-.11-.845-.328-.203-.218-.304-.523-.304-.914 0-.388.101-.691.304-.91.203-.218.485-.327.845-.327s.642.109.845.327c.203.219.304.522.304.91 0 .39-.101.696-.304.914-.203.218-.485.328-.845.328zm0-.514c.318 0 .477-.242.477-.728 0-.483-.16-.724-.477-.724-.318 0-.477.241-.477.724 0 .486.16.728.477.728zm-6.844 1.886c.405-.306.944-.408 1.39-.408.418 0 .756.09.828.185.15.2-.039 1.584-.775 2.244-.112.102-.22.047-.17-.087.166-.442.536-1.436.36-1.677-.175-.242-1.158-.115-1.6-.058-.068.008-.107-.02-.112-.061v-.023c.004-.036.03-.078.079-.115zm-10.184-.172a.105.105 0 0 1 .106-.091c.027 0 .057.009.089.028a11.778 11.778 0 0 0 6.194 1.772c1.52 0 3.19-.34 4.726-1.043.232-.105.426.164.2.346-1.371 1.09-3.359 1.67-5.07 1.67-2.397 0-4.557-.956-6.191-2.547a.173.173 0 0 1-.054-.097Z"/></svg>
```

### `assets/logos/tubi.svg`

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 8.56 24 6.88"><path fill="#FFF200" d="M16.696 15.272v-.752c.4.548 1.107.917 1.934.917 1.475 0 2.28-.956 2.28-2.865 0-1.714-.893-2.858-2.235-2.858-.851 0-1.55.347-1.979.908v-2.06h-2.674v6.71zm1.57-2.614c0 .827-.337 1.275-.827 1.275-.486 0-.837-.452-.837-1.275s.342-1.28.837-1.28c.495 0 .828.452.828 1.28zM6.94 9.988v3.6c0 1.236.754 1.841 1.955 1.841.959 0 1.625-.396 2.028-1.064v.91h2.597V9.989h-2.675v3.14c0 .493-.346.693-.666.693-.321 0-.568-.192-.568-.655V9.989Zm14.39 0H24v5.276h-2.67ZM6.553 11.136c0 .781-.635 1.415-1.42 1.415-.783 0-1.419-.634-1.419-1.415 0-.782.636-1.415 1.42-1.415.784 0 1.42.633 1.42 1.415zM3.49 9.702v2.668c.005.653.327.924.976.924.225 0 .526-.053.672-.166v1.931c-.49.243-.869.378-1.535.378 0 0-.069 0-.18-.006l-.003.006c-1.614 0-2.51-1.035-2.482-2.686v-.47H0V9.99h.92V8.563h2.569Z"/></svg>
```

### `assets/logos/youtube.svg`

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 3.54 24 16.92"><path fill="#FF0000" d="M23.498 6.186a3.016 3.016 0 0 0-2.122-2.136C19.505 3.545 12 3.545 12 3.545s-7.505 0-9.377.505A3.017 3.017 0 0 0 .502 6.186C0 8.07 0 12 0 12s0 3.93.502 5.814a3.016 3.016 0 0 0 2.122 2.136c1.871.505 9.376.505 9.376.505s7.505 0 9.377-.505a3.015 3.015 0 0 0 2.122-2.136C24 15.93 24 12 24 12s0-3.93-.502-5.814zM9.545 15.568V8.432L15.818 12l-6.273 3.568z"/><path fill="#FFFFFF" d="M 9.545,15.568 L 9.545,8.432 L 15.818,12 l -6.273,3.568 z"/></svg>
```

### `assets/logos/youtube-music.svg`

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path fill="#FFFFFF" d="M12 0C5.376 0 0 5.376 0 12s5.376 12 12 12 12-5.376 12-12S18.624 0 12 0zm0 19.104c-3.924 0-7.104-3.18-7.104-7.104S8.076 4.896 12 4.896s7.104 3.18 7.104 7.104-3.18 7.104-7.104 7.104zm0-13.332c-3.432 0-6.228 2.796-6.228 6.228S8.568 18.228 12 18.228s6.228-2.796 6.228-6.228S15.432 5.772 12 5.772zM9.684 15.54V8.46L15.816 12l-6.132 3.54z"/></svg>
```

### `assets/logos/youtube-tv.svg`

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 9.79 24 4.42"><path fill="#FFFFFF" d="M21.794 10.609h-.618v3.286h-.607v-3.286h-.617v-.504h1.842v.504zm.135-.504h.649l.202 1.703c.057.488.11.908.172 1.484h.026c.057-.576.114-.986.17-1.484l.208-1.703H24l-.623 3.784h-.82l-.628-3.784zm-15.758.38a.796.796 0 0 0-.555-.561c-.493-.13-2.465-.13-2.465-.13s-1.973 0-2.466.13a.787.787 0 0 0-.555.56C0 10.977 0 12 0 12s0 1.028.13 1.516a.794.794 0 0 0 .56.56c.488.13 2.46.13 2.46.13s1.973 0 2.466-.13a.806.806 0 0 0 .56-.555c.13-.493.13-1.516.13-1.516s-.005-1.028-.135-1.52zm-3.649 2.46v-1.89l1.64.945-1.64.945zm6.629.867a.652.652 0 0 1-.265-.395 2.703 2.703 0 0 1-.078-.706v-.4a2.383 2.383 0 0 1 .093-.716.724.724 0 0 1 .28-.394.943.943 0 0 1 .504-.125c.203 0 .369.042.493.125.13.098.229.233.27.394.062.234.094.472.083.716v.4c.005.239-.02.478-.083.711a.7.7 0 0 1-.27.395c-.124.083-.296.124-.508.124-.218-.005-.395-.046-.52-.13zm.7-.436c.037-.089.052-.24.052-.442v-.856a1.376 1.376 0 0 0-.052-.436.189.189 0 0 0-.181-.14.183.183 0 0 0-.177.14c-.041.14-.057.29-.052.436v.861c0 .203.016.353.052.442.02.083.094.14.182.135a.183.183 0 0 0 .176-.14zm8.88-.66v.14c0 .135.006.265.016.4a.373.373 0 0 0 .068.192.2.2 0 0 0 .156.062.2.2 0 0 0 .192-.109c.036-.072.052-.192.057-.363l.534.036a.56.56 0 0 1 .006.1c0 .253-.068.445-.208.57-.14.125-.337.187-.592.187-.306 0-.519-.099-.643-.29s-.187-.489-.187-.894v-.482c0-.416.062-.717.192-.909.13-.192.348-.285.654-.285.213 0 .379.041.493.12.12.088.207.217.244.363.052.223.078.456.067.685v.472l-1.048.005zm.079-1.162a.367.367 0 0 0-.063.192 5.48 5.48 0 0 0-.015.405v.197h.457v-.197c0-.135-.006-.27-.021-.405a.373.373 0 0 0-.068-.192.174.174 0 0 0-.145-.057.169.169 0 0 0-.145.057zm-11.175 1.11l-.706-2.559h.618l.249 1.158c.062.285.109.53.14.732h.02c.022-.146.068-.384.14-.727l.26-1.163h.618l-.716 2.56v1.23H7.64v-1.23h-.005zm4.905-1.536v2.767h-.488L12 13.557h-.016c-.13.255-.332.384-.596.384-.182 0-.317-.062-.405-.181-.089-.12-.13-.307-.13-.566v-2.066h.623v2.03a.659.659 0 0 0 .041.264.137.137 0 0 0 .135.078.28.28 0 0 0 .156-.052.281.281 0 0 0 .109-.124v-2.196h.623zm3.192 0v2.767h-.488l-.052-.338h-.016c-.13.255-.332.384-.596.384-.182 0-.317-.062-.405-.181-.089-.12-.13-.307-.13-.566v-2.066h.623v2.03a.659.659 0 0 0 .041.264.137.137 0 0 0 .135.078.28.28 0 0 0 .156-.052.281.281 0 0 0 .109-.124v-2.196h.623zm-1.5-.52h-.618v3.287h-.607v-3.286h-.618v-.504h1.843v.504zm3.56.961c-.036-.171-.098-.3-.181-.379a.508.508 0 0 0-.348-.12.56.56 0 0 0-.322.1.635.635 0 0 0-.228.254h-.005V9.965h-.597v3.93h.514l.062-.26h.015a.531.531 0 0 0 .218.223.69.69 0 0 0 .322.083.502.502 0 0 0 .467-.29c.099-.192.15-.499.15-.909v-.436a4.861 4.861 0 0 0-.067-.737zm-.57 1.127c0 .155-.006.316-.026.472a.508.508 0 0 1-.084.244.195.195 0 0 1-.155.072.257.257 0 0 1-.14-.036.3.3 0 0 1-.104-.109v-1.567a.425.425 0 0 1 .109-.187.224.224 0 0 1 .166-.073c.057 0 .114.026.145.073.042.078.068.16.073.249.015.166.02.332.02.498l-.005.364z"/><path fill="#FF0000" d="m 6.171,10.485 a 0.796,0.796 0 0,0 -0.555,-0.561 c -0.493,-0.13 -2.465,-0.13 -2.465,-0.13 s -1.973,0 -2.466,0.13 a 0.787,0.787 0 0,0 -0.555,0.56 C 0,10.977 0,12 0,12 s 0,1.028 0.13,1.516 a 0.794,0.794 0 0,0 0.56,0.56 c 0.488,0.13 2.46,0.13 2.46,0.13 s 1.973,0 2.466,-0.13 a 0.806,0.806 0 0,0 0.56,-0.555 c 0.13,-0.493 0.13,-1.516 0.13,-1.516 s -0.005,-1.028 -0.135,-1.52 z"/><path fill="#FFFFFF" d="m 2.522,12.945 l 0,-1.89 l 1.64,0.945 l -1.64,0.945 z"/></svg>
```

### Official full-colour sources (optional upgrades)

| App | Where |
|---|---|
| YouTube, YouTube TV, YouTube Music, Google Meet | Google Partner Marketing Hub (`partnermarketinghub.withgoogle.com`); YouTube brand site; Workspace branding ZIP (`support.google.com/a/answer/9212588`) |
| Disney+ | Disney press site (an SVG would replace the PNG) |
| Others | Each service's press / brand page |

## 6. Change log

| Date | Change |
|---|---|
| 2026-11-22 | Disney+ logo: transparent PNG cut from the supplied image, `wide` box, tile `#0B2E6B` |
| 2026-11-22 | Extended to all 11 apps: Apple TV, Fandango at Home, Hulu, Netflix, Prime Video, YouTube logos added; Disney+ keeps its asset (no vector); fixes list from the Apps page screenshot (boxed icons, missing Meet, old YouTube TV icon) |
| 2026-11-22 | Google Meet: black tile, yellow `#FBBC04` icon |
| 2026-11-22 | YouTube TV tile black (red button, white wordmark), original size (`glyph` box) |
| 2026-11-22 | Tubi bumped up with `logo_box = "wide"` |
| 2026-11-22 | First version: YouTube TV, YouTube Music, Tubi, Google Meet |
