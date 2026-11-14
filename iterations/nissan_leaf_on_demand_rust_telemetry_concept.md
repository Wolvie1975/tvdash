# Concept Document: On-Demand EV Telemetry Service (Rust)

## 1. Project Overview & Goals
* **Objective:** Retrieve the State of Charge ($\text{SoC}\%$), estimated range, and charging status of a **2020 Nissan Leaf SV** (North America / `NNA` region) strictly **on demand** outside of the official MyNISSAN mobile application.
* **Primary Constraint (12V Battery Preservation):** Zero background polling while the vehicle is idle. Every remote status request wakes the vehicle's Telematics Control Unit (TCU), drawing power from the 12V accessory battery. All wake-up calls must be explicitly triggered by user action or guarded by active charging state.
* **Target Stack:** Single-binary Rust application (CLI and/or lightweight HTTP webhook server) suitable for local execution or containerized deployment.

---

## 2. Architecture Options

### Option A: Official OAuth2 Gateway (`smartcar` Crate)
Uses Smartcar's developer API to abstract away Nissan's legacy authentication and asynchronous TCU wake-up cycle.

```text
[Trigger: CLI / HTTP Widget] 
       │
       ▼
[Rust Service (Axum + smartcar crate)] ──(OAuth2 Refresh Token)──► [Smartcar API] ──► [Nissan Cloud / TCU]
       │
       ▼
[JSON / Plaintext Response: SoC %, Range, Plugged-in Status]
```

* **Pros:**
  * Native Rust SDK (`smartcar` crate).
  * Standard OAuth2 token lifecycle; no custom cryptography or reverse-engineered endpoints.
  * Synchronous-feeling single API call (`vehicle.battery()`).
* **Cons:**
  * Relies on a third-party API intermediary (Smartcar free developer tier).

### Option B: Direct NissanConnect / Carwings Protocol (`reqwest` + `blowfish`)
Communicates directly with Nissan North America (`gdcportalgw.its-mo.com`) using the 3-step asynchronous request-and-poll workflow.

```text
[Trigger: CLI / HTTP Widget]
       │
       ▼
[Rust Service (reqwest)]
  ├── 1. POST InitialApp_v2.php ──────────► Obtain dynamic Blowfish key
  ├── 2. POST UserLoginRequest.php ───────► Encrypt password (Blowfish-ECB) -> Get custom_sessionid
  ├── 3. POST BatteryStatusRecordsRequest ► Wake vehicle TCU -> Receive resultKey
  └── 4. Loop BatteryStatusCheckRequest ──► Poll every ~4s (max 45s) until responseFlag == "1"
       │
       ▼
[Compute SoC % & Return Payload]
```

* **Pros:**
  * Zero third-party cloud dependencies; talks directly to Nissan's gateway.
  * Full control over retry timeouts and wake-up cancellation.
* **Cons:**
  * Requires maintaining legacy Blowfish-ECB password encryption and session handling.
  * Susceptible to upstream Nissan endpoint/header changes.

---

## 3. Core Functional Requirements

1. **Strict On-Demand Execution:**
   * No internal `tokio::time::interval` background loops.
   * Execution occurs *only* when invoked via CLI command (e.g., `leaf-soc fetch`) or an incoming authenticated HTTP `POST /refresh` request.
2. **Request Cooldown / Debounce Guard:**
   * Implement a local state cache (in-memory or lightweight JSON/SQLite file) storing `last_polled_at` and `cached_soc`.
   * Enforce a configurable cooldown (e.g., `10 minutes`) with a `--force` override flag to prevent accidental rapid-fire TCU wake-ups.
3. **85% Charge Target Alerting (Optional Hook):**
   * When actively charging (or polled during a charge session), evaluate if $\text{SoC} \ge 85\%$.
   * Dispatch a webhook/notification (e.g., ntfy, Pushover, or Home Assistant webhook) so charging can be stopped manually to preserve pack longevity.
4. **Consumer Integrations:**
   * **Mobile Home Screen:** Triggered via an HTTP Shortcut widget on Android, displaying the returned percentage via toast/notification or widget state.
   * **Terminal / Desktop:** Direct stdout output (plain percentage for status bars like Waybar/Polybar, or `--json` for scripting).

---

## 4. Proposed Rust Crate Stack

| Concern | Recommended Crates | Notes |
| :--- | :--- | :--- |
| **Async Runtime** | `tokio` | Required for async HTTP clients and server handling. |
| **HTTP Server (Webhook)** | `axum`, `tower-http` | Minimal overhead, clean state sharing for cooldown cache. |
| **HTTP Client** | `reqwest` (`rustls-tls`, `json`) | Pure-Rust TLS for clean static/container builds. |
| **Serialization** | `serde`, `serde_json` | Parsing API responses and local state cache. |
| **CLI Parsing** | `clap` (derive) | Subcommands: `serve`, `fetch`, `status` (cached). |
| **Config & Secrets** | `dotenvy` or `config` | Load credentials/tokens via environment variables. |
| **Option A SDK** | `smartcar` | Official OAuth2 vehicle telemetry wrapper. |
| **Option B Crypto** | `blowfish`, `ecb`, `base64` | Required only if implementing direct NNA Carwings login. |

---

## 5. Data Model Sketch

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatteryTelemetry {
    pub soc_percent: f32,
    pub estimated_range_miles: Option<f32>,
    pub is_plugged_in: bool,
    pub is_charging: bool,
    pub fetched_at: DateTime<Utc>,
    pub source: TelemetrySource, // LiveTcuWake | Cached
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TelemetrySource {
    LiveTcuWake,
    Cached,
}
```

---

## 6. Implementation Milestones

- [ ] **Phase 1: Proof of Concept (CLI)**
  - Choose **Option A (Smartcar)** or **Option B (Direct Carwings)**.
  - Build a minimal `fetch` command that authenticates, wakes the TCU once, and prints `SoC %` to stdout.
- [ ] **Phase 2: Cooldown & Local State Persistence**
  - Persist the last successful `BatteryTelemetry` struct to disk (`~/.cache/leaf-telemetry/state.json`).
  - Return cached data if queried within the cooldown window unless `--force` is specified.
- [ ] **Phase 3: On-Demand HTTP Endpoint & Containerization**
  - Add an `axum` HTTP listener (`GET /status` for cached, `POST /refresh` for live wake-up) protected by a bearer token.
  - Package into a minimal multi-stage Docker image (`scratch` or `distroless`).
- [ ] **Phase 4: Client Triggers & Notifications**
  - Wire up an Android home screen shortcut or Waybar module to hit the endpoint.
  - Add optional webhook dispatch when `soc_percent >= 85.0`.