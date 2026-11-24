# tvdash — Multiview Spike 1: Bringing a Brave window to the front

Test 1 from `tvdash-multiview.md` §7. **Question:** can tvdash bring one of several full-screen Brave windows to the front on the TV, with the laptop panel on, on GNOME Wayland?

This is the riskiest assumption in warm standby. If it fails, multiview falls back to **Concept A (quick switch, cold)**.

---

## 0. Pass criteria

| # | Check | Pass |
|---|---|---|
| 1 | The extension lists every Brave window, with its ID, PID and monitor | Both test windows listed |
| 2 | Each window can be sent full screen to the **TV** output | Both go full screen on the TV, never on the laptop |
| 3 | `Activate` brings the window behind to the front | It comes to the front **every time**, with no "is ready" notification instead |
| 4 | Switching speed | It feels instant (< 300 ms), with no black flash |
| 5 | Keyboard focus follows | After switching, keys go to the window now in front |
| 6 | Works for Wayland-native Brave (1A) | Checks 1–5 pass |
| 7 | Works for XWayland Brave (1B) | Checks 1–5 pass (fallback only) |

> **The failure to watch for:** Mutter's focus-stealing protection shows a "Brave is ready" notification instead of bringing the window to the front. The extension runs inside GNOME Shell, so it shouldn't trigger that. If you see the notification at any point, check 3 fails.

---

## 1. Check the environment

```bash
gnome-shell --version          # note the major version, e.g. 50
echo $XDG_SESSION_TYPE         # must say: wayland
brave-browser --version
```

The `shell-version` in step 2 must include the major version reported here.

---

## 2. Create the extension

It's a tiny GNOME Shell extension that exports one D-Bus object, `org.tvdash.Windows`, on the session bus.

| Method | In | Out | Does |
|---|---|---|---|
| `List` | | JSON | Normal windows: `id`, `pid`, `wmClass`, `title`, `monitor`, `fullscreen` |
| `Monitors` | | JSON | Monitors: `index`, `x`, `y`, `width`, `height`, `primary` |
| `Activate` | `id` (t) | bool | Brings the window to the front and gives it keyboard focus |
| `Fullscreen` | `id` (t), `monitor` (i) | bool | Moves the window to a monitor and makes it full screen |

```bash
EXT="$HOME/.local/share/gnome-shell/extensions/tvdash-windows@jcarpio"
mkdir -p "$EXT"
```

### 2.1 `metadata.json`

Replace `"50"` with your version from step 1.

```bash
cat > "$EXT/metadata.json" <<'EOF'
{
  "uuid": "tvdash-windows@jcarpio",
  "name": "tvdash windows",
  "description": "D-Bus window control for tvdash multiview",
  "shell-version": ["50"]
}
EOF
```

### 2.2 `extension.js`

```bash
cat > "$EXT/extension.js" <<'EOF'
import Gio from 'gi://Gio';
import Meta from 'gi://Meta';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';

const IFACE = `
<node>
  <interface name="org.tvdash.Windows">
    <method name="List"><arg type="s" direction="out" name="json"/></method>
    <method name="Monitors"><arg type="s" direction="out" name="json"/></method>
    <method name="Activate">
      <arg type="t" direction="in" name="id"/>
      <arg type="b" direction="out" name="ok"/>
    </method>
    <method name="Fullscreen">
      <arg type="t" direction="in" name="id"/>
      <arg type="i" direction="in" name="monitor"/>
      <arg type="b" direction="out" name="ok"/>
    </method>
  </interface>
</node>`;

const normalWindows = () =>
    global.get_window_actors()
        .map(a => a.get_meta_window())
        .filter(w => w.get_window_type() === Meta.WindowType.NORMAL);

const findWindow = id => normalWindows().find(w => w.get_id() === Number(id));

class Service {
    List() {
        return JSON.stringify(normalWindows().map(w => ({
            id: w.get_id(),
            pid: w.get_pid(),
            wmClass: w.get_wm_class(),
            title: w.get_title(),
            monitor: w.get_monitor(),
            fullscreen: w.is_fullscreen(),
        })));
    }

    Monitors() {
        return JSON.stringify(Main.layoutManager.monitors.map(m => ({
            index: m.index, x: m.x, y: m.y, width: m.width, height: m.height,
            primary: m.index === Main.layoutManager.primaryIndex,
        })));
    }

    Activate(id) {
        const w = findWindow(id);
        if (!w) return false;
        Main.activateWindow(w);          // raise + focus, from inside the shell
        return true;
    }

    Fullscreen(id, monitor) {
        const w = findWindow(id);
        if (!w) return false;
        if (w.is_fullscreen()) w.unmake_fullscreen();
        w.move_to_monitor(monitor);
        w.make_fullscreen();
        return true;
    }
}

export default class TvdashWindows extends Extension {
    enable() {
        this._dbus = Gio.DBusExportedObject.wrapJSObject(IFACE, new Service());
        this._dbus.export(Gio.DBus.session, '/org/tvdash/Windows');
        this._name = Gio.bus_own_name(Gio.BusType.SESSION, 'org.tvdash.Windows',
            Gio.BusNameOwnerFlags.NONE, null, null, null);
    }

    disable() {
        this._dbus?.unexport();
        this._dbus = null;
        if (this._name) Gio.bus_unown_name(this._name);
        this._name = 0;
    }
}
EOF
```

> **Security note:** any program in your session can call this object. That's fine for a single-user TV laptop. For the real build, consider checking the caller, or exposing only windows whose `wmClass` is Brave.

---

## 3. Load and enable it

On Wayland, GNOME Shell only finds a new extension after you **log out and back in**.

```bash
# 1. log out and back in, then:
gnome-extensions enable tvdash-windows@jcarpio
gnome-extensions info tvdash-windows@jcarpio      # State: ACTIVE
```

Watch for errors in a second terminal:

```bash
journalctl -f -o cat /usr/bin/gnome-shell
```

Quick check that it's on the bus:

```bash
gdbus call --session --dest org.tvdash.Windows \
  --object-path /org/tvdash/Windows \
  --method org.tvdash.Windows.Monitors
```

> **Tip:** if you edit `extension.js` later, run `gnome-extensions disable` and then `enable`. If the change still doesn't show, log out and back in again.

---

## 4. Helper aliases

Paste into your terminal. `jq` makes the JSON readable (`sudo dnf install -y jq` if needed).

```bash
tvw() { gdbus call --session --dest org.tvdash.Windows --object-path /org/tvdash/Windows --method "org.tvdash.Windows.$@"; }
tvjson() { tvw "$@" | sed -E "s/^\('(.*)',\)$/\1/" | jq; }

# usage:
#   tvjson List
#   tvjson Monitors
#   tvw Activate 123456
#   tvw Fullscreen 123456 1
```

Find the TV's monitor index from `tvjson Monitors`. It's usually the one that isn't `primary`, or the one at 1920×1080 or 3840×2160. Write it down:

```bash
TV=1     # ← your TV monitor index
```

---

## 5. Test 1A — Wayland-native Brave

Use **throwaway profiles** in `/tmp`, so your real tvdash profile isn't touched.

```bash
brave-browser --ozone-platform=wayland --user-data-dir=/tmp/tvdash-a \
  --kiosk "https://www.youtube.com" >/dev/null 2>&1 &

brave-browser --ozone-platform=wayland --user-data-dir=/tmp/tvdash-b \
  --kiosk "https://tv.apple.com" >/dev/null 2>&1 &
```

1. **List the windows** and note the two Brave IDs:

```bash
tvjson List
```

2. **Send both to the TV, full screen** (pass criterion 2):

```bash
A=<id of window A>; B=<id of window B>
tvw Fullscreen $A $TV
tvw Fullscreen $B $TV
tvjson List      # both: monitor = $TV, fullscreen = true
```

3. **Switch between them** (pass criteria 3–4):

```bash
tvw Activate $A     # A in front
tvw Activate $B     # B in front
```

4. **Stress it**: 20 switches, one every 1.5 s. Watch the TV for flashes, a wrong window or a notification.

```bash
for i in $(seq 1 10); do tvw Activate $A >/dev/null; sleep 1.5; tvw Activate $B >/dev/null; sleep 1.5; done
```

5. **Keyboard focus** (pass criterion 5): activate A, press Space, and check that the YouTube video in A plays or pauses, not B.

6. **Clean up:**

```bash
pkill -f /tmp/tvdash-a; pkill -f /tmp/tvdash-b
```

> **Note on PIDs:** `brave-browser` is a wrapper script, so the PID in `List` may not be the `$!` you got at launch. For the real build, match windows to slots by **PID → process group** or by the `--user-data-dir` in `/proc/<pid>/cmdline`. Record what you see in the results table below.

---

## 6. Test 1B — XWayland Brave (fallback)

Same steps as 1A, launched under X11:

```bash
brave-browser --ozone-platform=x11 --user-data-dir=/tmp/tvdash-a \
  --kiosk "https://www.youtube.com" >/dev/null 2>&1 &
brave-browser --ozone-platform=x11 --user-data-dir=/tmp/tvdash-b \
  --kiosk "https://tv.apple.com" >/dev/null 2>&1 &
```

Then repeat steps 1–6 from §5.

You can also try the same thing without the extension, to see whether it's needed under X11:

```bash
sudo dnf install -y wmctrl
wmctrl -lp                       # note the hex IDs of the two Brave windows
wmctrl -i -a 0x0400000a          # bring one to the front
```

If `wmctrl` alone raises the windows reliably, the XWayland route works without a shell extension, but it's still X11-only.

---

## 7. Results

| Check | 1A Wayland | 1B XWayland | Notes |
|---|---|---|---|
| 1. Windows listed (ID, PID, monitor) | | | |
| 2. Full screen on the TV | | | |
| 3. Activate raises every time | | | |
| 4. Instant, no black flash | | | |
| 5. Keyboard focus follows | | | |
| PID in `List` matches launch PID? | | | |
| `wmctrl` alone works? | n/a | | |

**Decision:**

- **1A passes:** build on the extension with Wayland-native Brave. This is the preferred route.
- **Only 1B passes:** run multiview Brave under XWayland.
- **Neither passes:** use Concept A (quick switch, cold).

## Change log

| Date | Change |
|---|---|
| 2026-11-21 | First version |
