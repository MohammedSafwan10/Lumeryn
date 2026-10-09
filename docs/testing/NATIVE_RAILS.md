# Native rail and fullscreen verification

Date: 2026-10-09. CC-01 source handoff: native Lumeryn shell informed by the owner's eighteen Clipchamp screenshots. This document separates source/unit evidence from pending native visual and pointer acceptance. It does not pass the remaining Clipchamp feature backlog.

Lead integration follow-up: native wide/mid/compact workflows and nine final readonly states passed; actual OS fullscreen is captured, gallery/export/clip/canvas controls integrated,87 distinct Rust tests/full gate passed. See [CLIPCHAMP_FOUNDATION.md](CLIPCHAMP_FOUNDATION.md). The handoff statuses below are historical; physical keyboard/pointer/DPI acceptance remains pending.

## Implemented shell at handoff

The existing shared-service editing controls are reorganized into narrow left/right icon rails and independently selected drawers. My media retains actual local import, source thumbnails and append-to-timeline. Clip retains source/duration trim; Project exposes existing settings/manual paths and advanced history/activity/diagnostics. Unsupported recording/text/effect tabs are absent. The lead is separately integrating the real licensed Library and export options.

At wide widths both selected drawers can coexist. Compact layouts give the active side space while preserving context for a later width change. Buttons have native egui interaction, widget labels and hover copy; keyboard/pointer/accessibility acceptance is still required. The lead received a refinement recommendation to keep a closed compact drawer closed rather than automatically revealing the opposite stored drawer.

Five-second seek buttons and Shift+Left/Right convert seconds through the rational project rate, round to the nearest frame and clamp to the valid range. Plain Left/Right retains one-frame stepping. Space retains play/pause. Existing monitor gain, integer clip edits, live CLI events, source-derived asset previews and mint/proportional-clock styling remain.

Full screen is now a native `ViewportCommand::Fullscreen` request on the same window, decoder/session/audio clock and GPU texture/device. It hides editor header/rails/timeline and advanced/export windows while showing preview/transport. F11 or the preview icon enters/exits; Escape restores editor chrome. `Session.native_fullscreen: Option<bool>` records the actual viewport flag for diagnostics and future smoke reports. A requested boolean alone is not proof that the OS entered fullscreen.

## Executed focused checks

After backend transport caught up with the service's new render-options field, this command executed successfully:

```powershell
cargo test -p lumeryn-studio --locked -j4 app::tests -- --nocapture
```

Four tests passed: existing clock parse/rounding, compact drawer toggles/context, preview/timeline/rail bounds at 1440/1280/980 widths, and rational five-second seek/clamping. These are headless logic/layout checks, not physical rendered-pixel or native fullscreen tests. Export-option imports were temporarily unused while the lead integrated that panel; this focused run does not certify the final warning-denied gate. Owned shell files were rustfmt-formatted before source handoff. Final workspace gates belong to the lead after integration settles.

## Native acceptance still required

| Check | Evidence required | Status at handoff |
| --- | --- | --- |
| Wide/mid/compact native layout | Actual 1440×900,1280×800,980×640 captures; inspect rail markers, drawer close/reopen, image aspect, clocks, all trim/import/export controls and linked waveform | Pending |
| Both drawers collapsed | Actual larger preview/timeline with both rails reachable, then reopen selected context | Pending |
| Compact side switch/close | My media↔Clip↔Project, close active drawer, resize wide/compact without lost selection/playhead | Pending |
| Actual fullscreen enter/exit | Report viewport fullscreen=true, native capture dimensions; Escape/fullscreen=false restores editor/window and selected frame | Pending |
| Fullscreen playback continuity | Playback/seek/monitor gain without second decoder/audio session; exit does not reset revision/frame or duplicate device output | Pending |
| Five-second transport | Known rational fixture, expected frame after ±5s with start/end clamp, plus unchanged arrow frame step | Unit conversion passed; native interaction pending |
| Same-app baseline workflow | Actual playback, exact seek/step, Studio trim, live real CLI commit, background decoded export and immutable source integrity | Pending on integrated rail binary; B baseline is historical evidence |
| Keyboard/focus | Tab/Enter rails, Space/frame arrows only outside text fields, Shift seeks, F11/Escape, dialog focus/cancel, reachable drawer close | Pending manual/native input |
| DPI/accessibility | 100%/150%/200% logical bounds, labels/focus visibility and native accessibility inspection | Pending |
| Performance | UI CPU percentiles/first-frame/paused CPU and bounded asset work, comparable timeline benchmark; no FPS claim from UI duration | Pending integrated measurement |

Existing `scripts/native-redesign-smoke.ps1` provides the three-size same-app baseline workflow. New fullscreen/collapsed capture states and native input proof require lead integration; do not pretend unsupported command-line flags already exist. Use new disposable fixtures/artifact directories and consistent backups, never original user media/projects. Coordinate any native build/launch with the open user Studio; no user process termination is authorized by this handoff.

The current source owners are the lead for app/design/backend/main/export/library integration and the specialists for their assigned media/catalog modules. No additional dependencies, graph/schema rewrite, asset download or source publication is introduced by CC-01 shell work.
