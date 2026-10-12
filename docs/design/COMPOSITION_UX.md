# Native composition UI extension

Date: 2026-10-10. Status: V1 named layers/video/PNG range/contain controls implemented; actual captures and tested states are recorded in COMPOSITION_ACCEPTANCE. V2–V7 remain interaction contracts. Extend approved [B Studio](STUDIO_REDESIGN.md), existing two rails, mint accent and Inter; no new theme-selection loop. Follow [delivery stages](../roadmap/COMPOSITION_DELIVERY_PLAN.md).

## Organisation

Header keeps project, undo/redo, quiet committed-save state and Export. Left rail creates/imports: Media, Library, Text/Shapes, later Transitions. Left drawer has search/category/style previews and one obvious insertion action. Right rail changes the selection: Clip/Transform, Text/Animation, Colour/Filters, Fade, Speed; Transition appears for a selected transition. One contextual drawer at a time. Advanced settings collapse. Actual native vector icons and tooltips; no glyph substitutes that resemble errors or slashed UI numerals.

Preview remains central and aspect-correct. Direct handles belong to the selected item's mode, not every object. Floating toolbar offers Select, Crop, Fit/Fill, Rotate/Flip and reset only when implemented. Handles are editor chrome and never exported. Timeline grows named visual rows above audio, with text/scene clips visibly distinct, real thumbnails/waveforms, accessible mute/visibility/lock controls and vertical scrolling. Layer order is explicit; hiding visuals must not implicitly mute audio. Track lock's editing meaning must be defined in Rust before exposure.

At1440x900 show both drawers when practical; at1280x800 narrow them; at980x640 show one secondary drawer and preserve preview/transport/timeline. Scroll long properties intentionally. Keep14â€“16px body text and36px primary controls, readable plain clocks, stable hover/selection/focus states and visible primary actions.100/150/200%DPI requires actual pointer/keyboard tests, not inference from logical size.

## Screen and state checklist

V1 layer/media/hold interactions are implemented. Mark visual acceptance only from actual native captures; later rows remain proposed.

| State | Main action and feedback | Stage |
|---|---|---|
| Empty project | Import or Add reviewed library asset; no seeded demo/layers. | Preserve existing |
| Import/preparation | Per-card Importing/Cancel; unverified asset cannot place; unrelated editing remains usable. | Preserve existing |
| Media-to-layer drag | Real thumbnail ghost, target row, exact insertion/placement time and snapped boundary; Escape cancels. | V1 |
| Layered timeline | Named visual rows, selected mint outline, free move distinct from ripple reorder, scrollbar/zoom/Fit. | V1 |
| Image hold | Select verified image, choose explicit duration/track; Inspector explains still duration without fake FPS. | V1 |
| Overlay/PIP transform | Drag body/corners; corner preset plus precise position/size; proportional lock and Reset. | V2 |
| Source crop | Separate crop mode with source-relative bounds, shaded excluded area, Apply/Cancel. | V2 |
| Rotation/flip | Simple90-degree controls first; tested arbitrary angle/pivot later. | V2 |
| Text creation | Plain text plus original style tiles; insertion creates real editable nodes. | V3 |
| Text editing | Content, font/size/alignment/fill; font/glyph issues identify remedy; no machine-dependent silent fallback. | V3 |
| Animation | Property-specific keyframe toggle, nearby keys and concise easing controls; timeline keys editable. | V4 |
| Template insertion | Real rendered previews, editable parameter defaults, one atomic expansion and undo. | V4 |
| Colour/looks | Grouped sliders with numbers and Reset; selected-footage look previews and single Intensity. | V5 |
| Fade | Separate visual and audio rows, duration units, invalid lengths explained before apply. | V5 |
| Transition drag/selection | Valid cut highlight, type/duration and actual handle availability; no hidden shortening. | V6 |
| Speed | Multiplier, pitch policy, resulting duration and explicit ripple impact before commit. | V6 |
| Freeze | Source-frame preview and duration; initial hold is silent, independent background sound remains. Additional linked-audio choices require later typed support. | V6 |
| Stale gesture | Draft discarded with concise retry/reload explanation; committed selection remains inspectable. | Every slice |
| Render/progress/error | Background job, cancellation, verified completion, actionable unsupported-object/encoder errors. | Every slice |

## Gesture and concurrency rules

Capture project UUID/revision/schema/colour policy/epoch and grabbed offset on primary press. Show drafts with the shared evaluator, not an independent approximate shader. Coalesce preview requests and discard stale generations; do not seek synchronously from UI update. One accepted primary release commits one undoable edit. Slider drags follow the same rule; numeric input commits deliberately. Escape, focus loss, overlay, project switch, changed revision/schema or unsupported target cancels. Keyboard placement/move/trim and precise Inspector alternatives accompany pointer features. Local monitor isolation and persisted export-visible track enable are separate; never let an ambiguous eye icon silently alter output.

Snap to nearby playhead/clip endpoints with a documented pixel threshold and bypass modifier. Crop/trim/body handles have separate hit regions. Edge scrolling occurs only for accepted targets. Invalid drops show the reason and change no graph. Draft previews must visibly remain uncommitted; Saving is reserved for the service commit. No hidden silent rebase when CLI/Studio compete.

## Prebuilt style policy and acceptance

Starter pack: Clean title, Lower third, Callout, Chapter card and Picture-in-picture. Build original parameter recipes from editable nodes, existing audited fonts/art and actual renderer previews. Every style exposes inspectable values/version/provenance through CLI too. No proprietary Clipchamp stock/style artwork, premium/cloud badges, fake previews or inert controls.

For each new screen capture actual populated/empty/loading/error/conflict/wide/compact states next to the approved reference and inspect spacing, hierarchy, clipping, focus and full-path leakage. Pointer, keyboard, IME, high-DPI, device loss and physical A/V remain separate checks. This document contains no raster mockups or shipping placeholders; future captures record actual working functionality.

V1 uses timeline placement markers and genuine asset drag ghosts; the composed monitor updates on committed revisions. A new layer preserves primary/source/independent-audio drag and trim paths. Track labels have native context menus for moving up/down and deleting empty tracks; the authoritative service rejects populated track deletion. Time edits show readable elapsed values with exact frames in a collapsed secondary section. Position/size is a precise contain rectangle plus PIP preset; direct preview handles and uncommitted composite drafts are V2. V1 still has a three-placement admission cap, and the UI surfaces service errors instead of ignoring rejected items.
