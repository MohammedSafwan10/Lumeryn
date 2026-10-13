# Independent static scene, colour and transform review

Date:2026-10-12. Readonly review of in-progress integration. This reviewer did not run Cargo because root owns the serialized build. Sources continue changing under assigned owners; this report is not final tested-head approval. Animated/time features are explicitly pending and are not treated as defects merely because absent.

## Actionable findings sent to root

1. **P1 — Native Add title uses the wrong line-height unit.** `apps/lumeryn-studio/src/scenes.rs`, initial TextNode construction around line116: font_size is height×0.075 while line_height is1.25. `core/src/scene_text.rs` validates absolute line_height≥font_size and passes Parley `LineHeight::Absolute`. At ordinary720p/1080p, title creation therefore fails. The size slider also changes font_size without maintaining an absolute line-height. Use height×0.075×1.25 at creation and preserve a valid ratio or expose absolute spacing on edit. Test actual native default document validation at normal/compact canvases. Status: root notified; correction/acceptance pending at review time.

2. **P2 — Repeated native overlay removal exhausts hidden scene assets.** `scenes.rs`, Remove overlay branch around line326 emits only DeleteScenePlacement. Creation always appends a new SceneAsset; graph caps8assets. Eight create/remove cycles leave an empty timeline that cannot create again, and native does not expose unplaced asset deletion. If removing the last reference, submit DeleteScenePlacement plus DeleteScene atomically; retain shared assets when other placements refer to them. Test reuse after repeated removal. Status: root notified.

3. **P2 — Connector endpoints are labeled Width/Height.** Inspector passes x1/y1/x2/y2 into the box geometry helper. The last pair are endpoints, not dimensions, so changing a connector using those labels is misleading. Use X1/Y1/X2/Y2 endpoint labels. Status: root notified.

## Lifecycle concern requiring executed evidence

Normal success retains VisualProgram.resources through export encoder/software retry and preview WorkerPipe.finish, then removes private rawRGBA resources. This is correct normal-path ownership. Cancellation/error destruction uses Tokio child kill_on_drop without an awaited exit; the last TempDir lease may drop while Windows FFmpeg still owns an input handle. TempDir destructor cannot report cleanup failure to the caller, potentially leaving files. This is a concrete ordering concern, **not an observed leak**. Backend owner notified to verify cancellation and either retain lease through child kill/wait or demonstrate platform lifecycle behavior. No unrelated cache deletion is authorized by this review.

## Reviewed boundaries without additional blockers

- Scene assets/placements are genuine semantic graph objects with independent stableIDs, schema5-only fields, no dummy media IDs. Same-track spans and total/raster admission include scenes; exact scene endpoints contribute duration. History migration keeps existing IDs/revisions/commands/receipt bytes and empty new fields for older snapshots.
- Reviewed font admission is closed to embeddedInterregular SHA/face; no system fonts or project font paths. Missing glyphs/overflow fail. UTF8/layout raster tests and decoded export evidence remain root's gate. This does not prove multilingual acceptance.
- Cache key includes whole serialized scene/font/output dimensions and policy domain; cache is local to one lowering and discarded afterward, so it cannot mix engine versions across runs. It retains bounded bytes while externally held Arc pixels may exceed retained-cache accounting. It is not a persistent cache or warm-across-seek claim.
- Static scene rawRGBA is rendered at final fitted output size and owned temporary input persists normally. Strict aspect failures for rounded unusual canvases are an explicit current limit; unsupported combinations must remain visible in discovery rather than silently stretching layouts.
- Colour Inspector draft identity includes project/revision/epoch/clip. Reset emits default shared effects and keeps transform semantics separate. Inspector resides inside the right drawer's vertical scroll. Native apply remains one shared edit, with advanced frames instead of claiming professional gesture/keyboard/DPI acceptance.
- Canvas transform draft checks project/revision/epoch/selection/image/focus/Escape and validates a candidate on release. It shows bounds-only geometry with explicit draft label, rather than pretending changed pixels are persisted. Primary-media direct handles and live pixel drafts remain declared limitations.

Root should request final review after fixes, integration and actual native/title/output gates. Passing source inspection is not a bug-free or product-complete claim.
