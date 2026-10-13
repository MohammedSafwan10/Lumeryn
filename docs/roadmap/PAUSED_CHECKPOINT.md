# Owner pause — 2026-10-13

The owner requested a temporary project pause, a private Git checkpoint of the current work and removal of local Cargo build caches. This is an **unfinished implementation checkpoint**, not a release or a claim that all six feature areas are complete. Do not automatically migrate owner projects or replace installed binaries when resuming.

## Preserved implementation

- Schema5 graph work adds explicit scene assets/placements, visual properties, clip-local animation and time effects, and bounded two-clip transition edges. Transactions, receipts, source proofs, history and migration remain shared Rust semantics.
- Checked crop/fit/flip/rotation/opacity lowering, native Inspector controls and bounds-only overlay move/resize gestures are present. Draft composite pixels and primary canvas handles remain unfinished.
- Maintained Parley/Swash typography uses the reviewed embedded Inter face. Static text/rectangle/ellipse/connector scenes rasterize at final output resolution and own temporary raw RGBA resources through rendering.
- CLI scene, visual effect, animation, time and transition commands are present. Original editable starters expand to ordinary batches; CLI expansion is read-only and the returned batch must be explicitly dry-run/committed through the shared service.
- Native Text & shapes/Transitions drawers and context controls are present. Physical/native acceptance of the latest integrated UI remains pending.
- Encoded-SDR colour recipes and visual fades are persisted. Independent audio fades are not yet integrated. Speed/freeze use rational source mapping and the shared linked-audio plan; freeze is silent. Initial transitions require exactly two adjacent unstyled primary videos with proven handles; source audio retains hard cuts. Unsupported combinations must fail explicitly.
- A bounded process-local static-scene cache and matching experiment exist. No across-seek performance improvement or sustained UHD60 playback is claimed. The slow GEQ scale implementation is a conformance reference. The proposed faster native scale adapter has not completed root acceptance or product integration; graph scale rejection remains intentional until that work passes.

## Evidence and outstanding gates

Focused media checks executed before the pause include CLI transform export/reset/history/source immutability; exact UHD/portrait static-scene reference pixels; colour/audio fade reference checks; 30→60 and off-grid24→25 motion/opacity checks; and fifteen speed/freeze/transition checks including actual decoded pixels, PCM pitch/length/seek and silent freezes. These checks do not replace the full gate on the final combined source.

Formatting and the locked workspace compile check including all targets passed for the paused source. The last strict Clippy run failed on large enum variants in service `Request::Create` and preview audio `Source::Timeline` after the graph/plan grew. Final full tests, cargo-deny, native smoke, revised native gesture tests and visual inspections remain pending. Existing installed CLI/normal editor binaries are the earlier verified schema3 milestone; this checkpoint has not been rolled out.

## Resume order

1. Read PRODUCT_SPEC, AGENTS, PROGRESS, TASKS and this checkpoint. Inspect Git status and use one shared target directory; serialize Cargo builds.
2. Resolve strict Clippy findings, run the newly strengthened egui pointer tests, and finish the fast scale adapter's pixel/reconfiguration/resource checks before wiring it into the product or removing graph rejection.
3. Run all new CLI/schema/template/animation/time/transition tests and the full `scripts/check.ps1 -Media -Deny` gate. Correct stale schema/clipboard expectations without weakening semantic assertions.
4. Generate disposable media/projects and run real native acceptance at multiple window sizes/DPI. Inspect new drawers, title creation/edit/removal, original starters, colour/fades, canvas gestures, conflicts and cleanup/cancellation.
5. Verify genuine UHD/portrait exports containing the complete admitted editable timeline: requested FPS, frame count/duration, audio synchronization and actual visual correctness. Complete independent review, accurate capabilities/docs and verified install/rollout afterward.

Build caches are reproducible and intentionally removed for the pause. Source, Git history, reviewed assets, owner projects/media, tools, local test evidence and the last verified normal editor/CLI executables are preserved. The next source build will be a cold rebuild; do not recreate separate per-agent target directories.
