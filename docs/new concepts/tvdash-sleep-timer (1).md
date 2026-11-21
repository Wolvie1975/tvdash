# tvdash — Sleep Timer Build Spec (2026-11-20)

A sleep timer that saves your place, stops playback and closes tvdash after a set time or at the end of the current episode. It works in all three dashboard UIs (Classic, Blades, Glass) and in both layouts.

Builds on: `tvdash-blades-glass-build-spec.md` (tokens, components, HUD, Up Next card), `tvdash-tv-only-build.md` §6 (`home_cmd.rs`, local server on `127.0.0.1:47800`, Unix socket, Brave + pill extension), `tvdash-auto-layout-and-server.md` A5 (`state.json` re-exec) and M2's Plex `/:/timeline` reporting.

---

## 1. Decisions

| Topic | Decision |
|---|---|
| Presets | 15 · 30 · 45 · 60 · 90 · 120 min, **End of episode / End of movie**, Off |
| Presets count from | **Now** (picking 30 min while 12 min are left gives 30 min from now) |
| Paused playback | The timer keeps running, like a TV's sleep timer |
| Warning | **60 s** before: "Still watching?" card on the TV with **Keep watching (+30 min)** focused, **Sleep now**, **Turn off timer** |
| No answer | Fade out mpv audio (20 s), save the Plex resume point, close mpv and Brave, quit tvdash |
| After quitting | **Quit only.** No screen-off or suspend option: tvdash releases its idle inhibit and GNOME's own *Screen Blank* setting turns the monitor off |
| TV power | **No HDMI-CEC on the laptop** (handoff decision), so tvdash doesn't turn the TV off. Once GNOME blanks the screen the HDMI signal stops, and most TVs go to standby on their own after a while with no signal |
| Keep watching | **+30 min** from now |
| End of item | **End of this episode / movie only** (no "in 2 episodes") |
| Calls | **Held while Google Meet is open** (any app with `hold_sleep_timer = true`): no warning and no shutdown during a call. If the time runs out mid-call, the full warning shows once you're back in tvdash (§5.1) |
| Survives | Layout switches (re-exec), laptop suspend, and the screensaver |
| Remote control | `tvdash --sleep …` (socket) and `GET/POST /sleep` (local server), for F-keys, scripts and the v3 phone remote |

---

## 2. Where it shows up

### Blades and Glass

| Place | Change |
|---|---|
| **Home › Quick actions** | New row **Sleep timer** between Screensaver and Settings. Right hint: "Off", "42 min", or "End of episode". OK opens the picker (§3). The card goes from 5 to **6 rows**. Check it still fits at 1080p; if it doesn't, shrink the row height to 56 rather than dropping a row |
| **Panel header status** | While a timer is on, a chip appears **before** the weather: moon icon + "42 min" (24 / 700, `accentInk`). It isn't a focus target. Shown on every tab and sub-page |
| **Video player HUD** | Controls row, right side, after Audio: **Sleep · Off** / **Sleep · 42 min**. OK opens the picker over the HUD |
| **Ambient screensaver** | Footer left adds " · Sleep in 42 min" after the library counts |
| **Settings › Display** | One new row under Clock: **Sleep warning** [30 s · 60 s · 2 min] |

### Classic (TV only)

- Home: a fourth `IconTile` **Sleep** next to Search, Library and Screensaver. Its label reads "Sleep · 42 min" while a timer is on.
- Player HUD: a square **Sleep** control tile at the end of the row.
- Settings: the same row as a `ValueTile` (OK cycles the value).

### Two screens (laptop as controller)

- Home banner top row: the same chip, left of the weather; **tapping it opens the picker on the laptop**.
- Home transport row: **Sleep** button after Full screen (76 px).
- The warning card shows on **both** screens; answering on either one closes both.

---

## 3. Picker overlay

Same pattern as the Watching now overlay (`blades-glass` §5.4):

- Scrim `rgba(5,6,8,.72)`. Dialog 960 wide, centred, radius 28, `panel`, padding 34×40.
- Header: moon icon, **Sleep timer** 36 / 900, status line 21 `sub`: "Off" or "Ends at 11:42 PM · 42 min left". Close button with ESC hint, on the right.
- **Option grid**, 4 columns, gap 14. Tiles are 120 tall, radius 18, `raised`. Value 34 / 900, caption 17 / 700 `sub`:

| Tile | Value | Caption |
|---|---|---|
| 1–6 | 15 min · 30 min · 45 min · 1 h · 1 h 30 · 2 h | "Ends 11:15 PM" (live) |
| 7 | **End of episode** (TV), **End of movie** (movie) | "S2 · E7 · 18 min left" |
| 8 | **Off** | "No timer" |

- Tile 7 is available only while **Plex is playing in mpv**. With Brave or nothing playing, it is dimmed (50%) with caption "Plex only" and can't be selected. Focus skips it.
- **Focus starts on** the current choice; with the timer off, it starts on the last-used preset (default 1 h).
- The selected tile gets `accent` fill and `accentText`, like a selected pill. OK sets the timer, closes the dialog and shows a toast: "Sleep in 1 h · ends 11:42 PM" or "Sleeping after this episode".
- Back/Esc closes without changing anything.

---

## 4. Warning card ("Still watching?")

It looks like the Up Next card (`blades-glass` §5.15): bottom-right, above the HUD, 470 wide, radius 24, `#14171C`, padding 16.

- Kicker **SLEEP TIMER · 0:58** (16 / 900, accent, tabular numbers, counting down every second).
- Title **Still watching?** (22 / 900). Body 18 `sub`: "tvdash will save your place and close."
- Countdown bar (6 px, accent on `track`), emptying over the warning time.
- Buttons, in this order: **Keep watching · +30 min** (accent, **focused**), **Sleep now**, **Turn off timer** (secondary, small).
- Back/Esc = Keep watching. Arrow keys move focus as usual. **No key dismisses the card by itself**, so a stray press on the remote can't accidentally cancel the timer or sleep early.
- *End of episode* mode: when the episode ends, this card **replaces the Up Next card**, with the title "Episode finished" and **Keep watching** as **Play next** (it plays the next episode and turns the timer off).

### What's on top of the TV decides where the card is drawn

| On the TV | Card shown by |
|---|---|
| tvdash window (Home, HUD, screensaver, any page) | tvdash, as above |
| **Brave** (streaming) | The **pill extension**: same card in its shadow DOM (§7.3) |
| **mpv's own full-screen window** (if the HUD isn't drawn over it in the current build) | mpv OSD + temporary key bindings (§7.4) |

> **Check first (S1):** in the current build, which window has keyboard focus during Plex playback, tvdash's HUD layer or mpv? This decides whether §7.4 is needed.

---

## 5. Timer logic

```rust
// crates/app/src/sleep.rs
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum SleepTimer {
    #[default]
    Off,
    /// Unix seconds. Wall clock, so time spent suspended and layout re-execs both count.
    At { deadline: i64 },
    /// Plex in mpv only: sleep when this item plays to the end.
    EndOfItem { rating_key: String },
}

#[derive(Debug, PartialEq)]
pub enum Tick { Idle, Counting(i64), Warn(i64), Fire }

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Sleep {
    pub timer: SleepTimer,
    pub warned: bool,
}

impl Sleep {
    pub fn set_minutes(&mut self, minutes: i64, now: i64) {
        self.timer = SleepTimer::At { deadline: now + minutes * 60 };
        self.warned = false;
    }

    pub fn set_end_of_item(&mut self, rating_key: &str) {
        self.timer = SleepTimer::EndOfItem { rating_key: rating_key.to_owned() };
        self.warned = false;
    }

    pub fn off(&mut self) { *self = Sleep::default(); }

    /// Call once a second.
    pub fn tick(&mut self, now: i64, warn: i64) -> Tick {
        let SleepTimer::At { deadline } = self.timer else { return Tick::Idle };
        let left = deadline - now;
        if left > warn {
            Tick::Counting(left)
        } else if !self.warned {
            // Always give the full warning, even when waking from suspend past the deadline.
            self.warned = true;
            self.timer = SleepTimer::At { deadline: now + warn };
            Tick::Warn(warn)
        } else if left > 0 {
            Tick::Warn(left)
        } else {
            Tick::Fire
        }
    }

    /// mpv `end-file` with reason `eof`. Returns true if Up Next should be replaced by the warning.
    pub fn item_finished(&mut self, rating_key: &str, now: i64, warn: i64) -> bool {
        match &self.timer {
            SleepTimer::EndOfItem { rating_key: k } if k == rating_key => {
                self.timer = SleepTimer::At { deadline: now + warn };
                self.warned = true;
                true
            }
            _ => false,
        }
    }

    /// mpv `end-file` with reason `stop` / `quit` (you pressed Stop): an end-of-item timer has nothing left to wait for.
    pub fn item_stopped(&mut self) -> bool {
        let was = matches!(self.timer, SleepTimer::EndOfItem { .. });
        if was { self.off(); }
        was   // true → toast "Sleep timer off · playback stopped"
    }
}

pub fn label(left: i64) -> String {
    match left {
        3600.. => format!("{} h {:02} min", left / 3600, (left % 3600) / 60),
        60.. => format!("{} min", (left + 59) / 60),
        _ => format!("0:{:02}", left.max(0)),
    }
}
```

- A `tokio::time::interval(1 s)` on the runtime calls `tick` and sends `AppEvent::Sleep(Tick)` to the UI thread. `Counting` updates the chip/hints (only when the minute label changes). `Warn` shows or updates the card. `Fire` runs §6.
- `Keep watching` = `set_minutes(extend_by, now)` (default 30).
- **Re-exec (auto layout):** add `"sleep": Sleep` to `$XDG_RUNTIME_DIR/tvdash/state.json` (A5 step 1) and restore it on startup. Because the deadline is wall clock, no time is lost.
- **Suspend:** `Instant` doesn't count time asleep on Linux, which is why the deadline is unix seconds. If the deadline passes while the laptop sleeps, the first tick after waking shows the full warning instead of quitting straight away.

### 5.1 Hold during calls

`tvdash-google-meet.md` adds Meet as an app with `hold_sleep_timer = true` in its `[[streaming]]` entry. While that app is open in Brave, the timer keeps counting, but it never warns or fires:

```rust
impl Sleep {
    /// Call before `tick` while an app with hold_sleep_timer is open.
    /// Parks the deadline just outside the warning window, so nothing shows during the call.
    pub fn hold(&mut self, now: i64, warn: i64) -> bool {
        if let SleepTimer::At { deadline } = self.timer {
            if deadline - now <= warn {
                self.timer = SleepTimer::At { deadline: now + warn + 1 };
                self.warned = false;
                return true;   // chip reads "Paused · on a call"
            }
        }
        false
    }
}
```

- After the call (F12, the pill, or Meet closed), the next tick goes straight into the **full** warning, so you get 60 s to choose.
- *End of episode* doesn't apply: Meet isn't Plex.
- The pill extension never shows the sleep card on a held app.

### Unit tests (warn = 60)

- [ ] `set_minutes(30)` at t = 0: `tick(1739)` → `Counting(61)`; `tick(1740)` → `Warn(60)` (first warning); `tick(1741)` → `Warn(59)`; `tick(1800)` → `Fire`.
- [ ] Deadline 10 min in the past and not warned → `Warn(60)`, not `Fire`; 60 s later → `Fire`.
- [ ] `item_finished` with a different `rating_key` → false, timer unchanged; with the same one → true, then `Warn(…)` down to `Fire`.
- [ ] `item_stopped` on `At` → false, timer unchanged; on `EndOfItem` → true, `Off`.
- [ ] `label`: 3600 → "1 h 00 min", 2521 → "43 min", 59 → "0:59".
- [ ] `Sleep` round-trips through `serde_json` (state.json).
- [ ] `hold` with 20 s left → deadline becomes now + 61, `warned` false; next `tick` without `hold` → `Warn(60)`.
- [ ] `hold` with 10 min left → no change.

---

## 6. Going to sleep

```rust
// crates/app/src/sleep.rs (continued)
const PAUSE_MEDIA_JS: &str =
    "document.querySelectorAll('video,audio').forEach(m => m.pause())";

pub async fn sleep_now(app: &AppHandle, cfg: &SleepConfig) {
    app.toast("Going to sleep · saving your place");

    // 1. Plex in mpv: fade, pause, report the resume point, quit.
    if let Some(p) = app.player.as_ref() {
        let vol = p.get_f64("volume").await.unwrap_or(100.0);
        let _ = p.fade_volume(0.0, cfg.fade).await;           // set_property volume in ~20 steps
        let _ = p.set("pause", true).await;
        if let (Ok(pos_s), Some(item)) = (p.get_f64("time-pos").await, p.current_item()) {
            let ms = (pos_s * 1000.0) as u64;
            let _ = tokio::time::timeout(
                std::time::Duration::from_secs(3),
                app.plex.timeline(&item, TimelineState::Stopped, ms),  // M2 reporter
            ).await;
        }
        let _ = p.set("volume", vol).await;                   // so a saved mpv volume isn't 0 next time
        p.quit_or_kill(std::time::Duration::from_secs(5)).await;  // `quit`, SIGTERM after 5 s
    }

    // 2. Brave: pause media so the site saves its position, then close it like go_home().
    if let Some(b) = app.browser.take() {
        let _ = b.eval_all_pages(PAUSE_MEDIA_JS).await;      // CDP Runtime.evaluate per page target
        tokio::time::sleep(std::time::Duration::from_millis(800)).await;
        b.close().await;                                      // CDP Browser.close, wait for exit
    }

    // 3. Clean up and leave.
    app.idle_inhibit.release();                               // GNOME's Screen Blank takes over from here
    let _ = std::fs::remove_file(runtime_state_path());       // nothing to reconnect to next start
    tracing::info!("sleep timer fired");
    app.quit();
}
```

The same function runs for **Sleep now** (card, picker, CLI, HTTP).

### Monitor off

tvdash doesn't blank the screen itself. Releasing the idle inhibit hands control back to GNOME, which blanks the monitor after **Settings › Power › Screen Blank**.

> **Check:** that Screen Blank isn't set to *Never* on this laptop, and how long the TV takes to go to standby with no signal (often a TV setting, 5–15 min).

---

## 7. Control from outside

### 7.1 CLI → socket

Same tiny-client pattern as `tvdash --home`:

| Command | Socket line | Effect |
|---|---|---|
| `tvdash --sleep 60` | `sleep 60` | Timer 60 min from now (1–480) |
| `tvdash --sleep end` | `sleep end` | End of the current Plex item (error if not in mpv) |
| `tvdash --sleep now` | `sleep now` | Runs §6 straight away |
| `tvdash --sleep off` | `sleep off` | Cancels |
| `tvdash --sleep status` | `sleep status` | Prints e.g. `42 min · ends 23:42` and exits |

```rust
// main.rs, next to --home
let args: Vec<String> = std::env::args().collect();
if let Some(i) = args.iter().position(|a| a == "--sleep") {
    let arg = args.get(i + 1).map(String::as_str).unwrap_or("status");
    let path = std::env::var("XDG_RUNTIME_DIR")? + "/tvdash.sock";
    let mut s = std::os::unix::net::UnixStream::connect(path)?;
    writeln!(s, "sleep {arg}")?;
    if arg == "status" {
        let mut out = String::new();
        std::io::Read::read_to_string(&mut s, &mut out)?;
        print!("{out}");
    }
    return Ok(());
}
```

The socket handler splits on the first space: `home` stays as is, and `sleep <arg>` parses `<arg>` as minutes or `end | now | off | status`. For `status` it writes one line back and closes.

### 7.2 Local server (`127.0.0.1:47800`, loopback only, as `/home`)

| Method | Path | Body | Reply |
|---|---|---|---|
| `GET` | `/sleep` | none | `{"phase":"off|counting|warn|end_of_item","left":2521,"ends_at":"23:42"}` |
| `POST` | `/sleep` | `{"minutes":60}` or `{"action":"extend|now|off|end_of_item"}` | Same as GET, after the change; `400` for minutes outside 1–480 or `end_of_item` with no Plex playback |

```rust
#[derive(serde::Deserialize)]
#[serde(untagged)]
enum SleepReq { Minutes { minutes: i64 }, Action { action: SleepAction } }

#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum SleepAction { Extend, Now, Off, EndOfItem }

// router: .route("/sleep", get(sleep_status).post(sleep_set))
```

### 7.3 Pill extension: warning card in Brave

The worker does the fetches (no CORS issues), as it already does for `home`:

```js
// extras/back-pill/worker.js
const BASE = 'http://127.0.0.1:47800';
chrome.runtime.onMessage.addListener((msg, _sender, reply) => {
  if (msg === 'home') { fetch(BASE + '/home', { method: 'POST' }).catch(() => {}); return; }
  if (msg?.t === 'sleep?') {
    fetch(BASE + '/sleep').then(r => r.json()).then(reply, () => reply(null));
    return true;   // keep the channel open for the async reply
  }
  if (msg?.t === 'sleep') {
    fetch(BASE + '/sleep', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ action: msg.action }),
    }).catch(() => {});
  }
});
```

```js
// extras/back-pill/pill.js: add inside the same IIFE, using the same closed shadow root
const card = document.createElement('div');
card.className = 'sleep';
card.hidden = true;
card.innerHTML = `
  <div class="k">SLEEP TIMER · <span class="t">1:00</span></div>
  <div class="h">Still watching?</div>
  <div class="b">tvdash will save your place and close.</div>
  <div class="row">
    <button data-a="extend" class="pri">Keep watching · +30 min</button>
    <button data-a="now">Sleep now</button>
    <button data-a="off">Turn off timer</button>
  </div>`;
root.appendChild(card);
root.querySelector('style').textContent += `
  .sleep[hidden] { display: none; }
  .sleep { position: fixed; right: 96px; bottom: 54px; z-index: 2147483647; width: 470px; padding: 20px;
    border-radius: 24px; background: #14171C; color: #E8EAED; font: 700 18px system-ui, sans-serif;
    box-shadow: 0 30px 80px rgba(0,0,0,.5); }
  .sleep .k { color: #F5A524; font-weight: 900; font-size: 16px; letter-spacing: 1.5px; font-variant-numeric: tabular-nums; }
  .sleep .h { font-size: 22px; font-weight: 900; margin: 6px 0 4px; }
  .sleep .b { color: #A3ABB6; margin-bottom: 14px; }
  .sleep .row { display: flex; gap: 10px; flex-wrap: wrap; }
  .sleep button { position: static; height: 52px; padding: 0 18px; border-radius: 26px; border: 0;
    background: #252A31; color: #E8EAED; font: 800 18px system-ui, sans-serif; opacity: 1; pointer-events: auto; }
  .sleep button.pri { background: #F5A524; color: #1A1200; }
  .sleep button:focus { outline: 4px solid #F5A524; outline-offset: 4px; }`;

let left = 0, tick = null;
const t = card.querySelector('.t');
const draw = () => { t.textContent = `${Math.floor(left / 60)}:${String(left % 60).padStart(2, '0')}`; };
const poll = () => chrome.runtime.sendMessage({ t: 'sleep?' }, (s) => {
  const warn = s && s.phase === 'warn';
  if (!warn) { card.hidden = true; clearInterval(tick); tick = null; return; }
  if (card.hidden) { card.hidden = false; card.querySelector('.pri').focus(); }
  left = s.left; draw();
  if (!tick) tick = setInterval(() => { if (left > 0) { left--; draw(); } }, 1000);
});
card.addEventListener('click', (e) => {
  const a = e.target.closest('button')?.dataset.a;
  if (!a) return;
  chrome.runtime.sendMessage({ t: 'sleep', action: a });
  card.hidden = true;
});
setInterval(poll, 5000);
poll();
```

- The 5 s poll means the card can appear up to 5 s late. That's fine with a 60 s warning. Each poll resyncs `left`.
- The pill's existing `button` rule (fixed position, auto-hide opacity) would also hit the card's buttons. The `.sleep button` rule above overrides it. If that gets messy, give the pill its own class.
- As the TV-only guide already notes, a site's own full-screen player can cover page overlays. If a service hides the card, the timer still fires on time; note the site.

### 7.4 mpv on top (only if S1 finds that mpv has focus)

```rust
// on Warn (first time)
p.command(json!(["keybind", "ENTER", "script-message tvdash-sleep extend"])).await?;
p.command(json!(["keybind", "ESC",   "script-message tvdash-sleep extend"])).await?;
p.command(json!(["keybind", "s",     "script-message tvdash-sleep now"])).await?;
// each second while warning
p.command(json!(["show-text", format!("Sleep timer · {}\nEnter: keep watching (+30 min) · S: sleep now", label(left)), 1500])).await?;
// on extend / off: restore the normal bindings (re-send the defaults from input.conf)
```

- tvdash already reads mpv IPC events: handle `client-message` with args `["tvdash-sleep", "extend"|"now"]`.
- **Check** the mpv version's `keybind` command and how it restores a key (rebinding to the original command from your `input.conf` is the simplest).

---

## 8. Config

`~/.config/tvdash/config.toml`:

```toml
[sleep]
presets = [15, 30, 45, 60, 90, 120]   # minutes, picker tiles 1–6
default = 60                          # focus when the timer is off and there is no last-used value
remember_last = true                  # focus the last-used preset next time
warn_before = "60s"                   # 30s | 60s | 2m (Settings › Display › Sleep warning)
extend_by = "30m"                     # Keep watching
fade = "20s"                          # mpv audio fade before stopping
```

- The timer itself is **never** saved in `config.toml`, only in the runtime `state.json`. A fresh start of tvdash always begins with the timer off.

---

## 9. Files

```text
crates/app/
├─ src/sleep.rs              # Sleep, tick, label, sleep_now (§5–6)
├─ src/home_cmd.rs           # + `sleep …` socket lines, GET/POST /sleep (§7.1–7.2)
├─ src/player.rs             # + fade_volume, quit_or_kill, end-file reason → sleep hooks, client-message
├─ src/browser.rs            # + eval_all_pages
├─ src/main.rs               # + --sleep client
└─ ui/
   ├─ state.slint            # + global SleepState { active, label, ends-at, phase, left, end-of-item-ok }
   ├─ shared/sleep.slint     # SleepChip, SleepPicker, SleepWarning (styled by the active Dashboard UI)
   ├─ v3/…                   # Blades/Glass: quick-actions row, header chip, HUD control, ambient footer
   └─ tv/…                   # Classic: IconTile, HUD tile, ValueTiles
extras/back-pill/            # worker.js + pill.js changes (§7.3)
```

- Add the picker tiles and warning-card buttons to `tile_rects()` in `tv_nav.rs` while they're open (they trap focus; Back closes).

---

## 10. Milestones

| M | Goal | Done when |
|---|---|---|
| S1 | Logic + check | `sleep.rs` with the §5 tests passing; answer the §4 focus question (tvdash HUD vs mpv window) |
| S2 | Picker + chip | Picker from Quick actions and the HUD in Blades, Glass and Classic; chip/hints update each minute; End of episode dims without Plex |
| S3 | Warning + sleep | Warning card in the tvdash window; Keep watching / Sleep now / Turn off; `sleep_now` saves the Plex position (it shows in Continue Watching with the right time) and closes mpv + Brave |
| S4 | Outside control | `--sleep` CLI, `/sleep` HTTP, pill-extension card in Brave; mpv OSD path if S1 needs it |
| S5 | Edges | Survives a lid-close layout switch and a suspend/resume; End of episode replaces Up Next; two-screen shows the card on both screens; Sleep warning setting works |

## 11. Done when

- [ ] Every preset works from Home and from the HUD in all three UIs, by remote alone.
- [ ] The chip shows the time left on every tab; the ambient footer shows it too.
- [ ] 60 s before the end, the card appears over tvdash, over Brave (pill extension), and over Plex playback.
- [ ] Doing nothing: audio fades, the Plex resume point is saved within ~5 s of where it stopped, mpv and Brave close, tvdash quits, and GNOME blanks the screen on its own schedule.
- [ ] Keep watching adds 30 min from now; Turn off cancels; Back = Keep watching; no single stray key sleeps or cancels.
- [ ] End of episode: no Up Next autoplay; the card says "Episode finished"; Play next turns the timer off; pressing Stop cancels the timer with a toast.
- [ ] A timer set before closing the lid is still counting after the layout switch, with the same end time.
- [ ] Waking the laptop after the deadline shows the full warning instead of quitting at once.
- [ ] `tvdash --sleep 1` closes everything about a minute later; `tvdash --sleep status` prints the time left.
- [ ] With a timer running out during a Google Meet call, nothing happens until you leave Meet; then the 60 s warning shows.

## 12. Change log

| Date | Change |
|---|---|
| 2026-11-20 | Google Meet: the timer is held while an app with `hold_sleep_timer = true` is open (§5.1) |
| 2026-11-20 | Spec written. Decided: quit only (GNOME's Screen Blank turns the monitor off; screen-off and suspend options dropped); Keep watching = +30 min; End of episode only, no multi-episode option; no remote-specific shortcut for the picker |
