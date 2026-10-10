# Effects and time operations: next composition slices

Research date: 2026-10-10. Status: proposed implementation contract; no effects, transition, speed or freeze implementation is claimed. This document changes no dependency, Rust code, project, source media or commercial licence. Sources were checked against current official documentation and upstream source; upstream main is not a substitute for validating the exact deployed binary.

## Recommendation and present boundaries

Keep the authoritative Rust graph and shared service. Add small, complete capability families after layered/image composition, with one evaluated visual plan and the existing shared audio plan serving both preview and export. Colour and timing must never exist only as Studio shader decorations or export-only FFmpeg strings. Preserve the native two-rail interface and current editing/history/import/export behaviour.

The current graph is schema2: one video track, matching CFR clips with integer frame ranges, and independent canonical 48k audio. `model.rs` checks source rate/range; `render.rs` and `lumeryn-preview/src/plan.rs` separately reject gaps/overlaps and require contiguous video. Export computes its final output frame count after timeline evaluation. `audio_plan.rs` maps absolute frame boundaries to 48k sample boundaries and generates the mix used by both clients. `apps/lumeryn-studio/src/gpu.rs` uploads decoded RGBA using eframe's device; it is not a compositing/effect engine. These are inspected implementation boundaries, not defects to bypass with UI-only controls.

Coordinate the new contracts with [composition research](COMPOSITION_FOUNDATION.md), [typography/audio research](TYPOGRAPHY_AUDIO.md), [CC inventory](../roadmap/CLIPCHAMP_FEATURE_INVENTORY.md) and the current root roadmap. Their older baseline observations are historical; schema2 audio is now implemented.

## Official technology findings

The following compact filter inventory derives from the [official FFmpeg filter manual](https://ffmpeg.org/ffmpeg-filters.html). It identifies candidates, not endorsed parameter mappings or finished rendering support.

| Requirement | Existing FFmpeg facility | Important boundary |
| --- | --- | --- |
| Exposure | `exposure` | EV correction |
| Temperature | `colortemperature` | Kelvin, mix and lightness controls |
| Contrast/saturation | `eq`; RGB matrix/LUT alternatives | Do not equate brightness with exposure |
| Opacity/blend | Alpha-aware composition; `blend` | Define alpha and colour space explicitly |
| Visual/audio fades | `fade`, `afade` | Frame/sample parameters; video can fade alpha |
| Transitions | `xfade`, `acrossfade` | Video inputs require equal CFR, size, format and timebase |
| Speed | `setpts`, `atempo` | `atempo` spans0.5–100; factors above2 skip samples; chain moderate factors |
| Hold a frame | `freezeframes`, selected-frame hold or `tpad` | Edge cloning alone cannot select an arbitrary source frame |
| Original looks | `lut3d` or matrix/curve operations | Audit LUT provenance and define interpolation |

Licences are specific to components and build configuration. The upstream [eq source](https://raw.githubusercontent.com/FFmpeg/FFmpeg/master/libavfilter/vf_eq.c) is GPL-2.0-or-later. Do not copy it into proprietary Rust or make it a required bundled convenience filter. Upstream [xfade](https://raw.githubusercontent.com/FFmpeg/FFmpeg/master/libavfilter/vf_xfade.c), [exposure](https://raw.githubusercontent.com/FFmpeg/FFmpeg/master/libavfilter/vf_exposure.c), [atempo](https://raw.githubusercontent.com/FFmpeg/FFmpeg/master/libavfilter/af_atempo.c), [video fade](https://raw.githubusercontent.com/FFmpeg/FFmpeg/master/libavfilter/vf_fade.c) and [audio fade/acrossfade](https://raw.githubusercontent.com/FFmpeg/FFmpeg/master/libavfilter/af_afade.c) carry LGPL-2.1-or-later headers. That does not make every binary containing them LGPL. FFmpeg explains that enabled GPL components change the overall build licence and gives a redistribution compliance checklist. Exact source, configure flags, notices and codec/patent review are required before bundling; linking and subprocess distribution need separate review. [FFmpeg legal guidance](https://ffmpeg.org/legal.html).

Rubber Band is a potential advanced time-stretch alternative, with a Windows utility available. Upstream offers GPL and a commercial proprietary-distribution licence. Do not purchase, bundle or enable it as a required runtime in this slice; first measure quality/latency of the existing FFmpeg tempo path. A commercial Rubber Band licence does not relicense the rest of a GPL-configured FFmpeg build. [Official Rubber Band product/licensing page](https://breakfastquay.com/rubberband/).

The existing wgpu stack has first-class Windows Vulkan/DX12 backends and permissive MIT/Apache2.0 licensing. It is a suitable GPU boundary, but does not automatically implement video colour management, device loss recovery, compositing or encoder interoperability. Keep current tested pins; upstream or latest-release compatibility alone is insufficient reason for an upgrade. [wgpu platform documentation](https://github.com/gfx-rs/wgpu#supported-platforms), [MIT licence](https://raw.githubusercontent.com/gfx-rs/wgpu/trunk/LICENSE.MIT), [Apache licence](https://raw.githubusercontent.com/gfx-rs/wgpu/trunk/LICENSE.APACHE).

Local read-only availability inspection on2026-10-10: installed FFmpeg reports8.1.1, `--enable-gpl --enable-version3`, and lists the candidate filters above. This is an independently installed tool, not a distributed Lumeryn component. No filter render, Windows GPU effect benchmark, speed quality test or preview/export comparison was executed for this research.

## Shared renderer choice and colour contract

Proposed architecture: Rust evaluates typed effects and exact timing into one window-independent composition plan. Backend selection is explicit and capability-gated. Preserve the current proven FFmpeg path for unchanged contiguous projects. Start new composition with a bounded CPU/reference path or a shared FFmpeg plan compiler only where both preview and export consume the identical generated semantics. A shared FFmpeg compiler may reuse mature filtering without writing a second broad decoder; colour mappings still require proof. Reserve GPU acceleration for measured bottlenecks after the reference images exist.

| Path | Benefit | Cost and acceptance condition |
| --- | --- | --- |
| Shared FFmpeg plan compiler | Reuses media processing; no new runtime now | CPU copies and filter availability vary. Both clients must compile the same evaluated plan, not parallel ad-hoc filter lists |
| Shared Rust CPU compositor | Deterministic offline reference; graph-independent headless tests | Pixel processing/upload costs must be measured; prefer reviewed optimized libraries over speculative unsafe loops |
| Window-independent wgpu compositor | Same shader kernels can serve preview and offline render | Requires headless adapter setup, final-frame readback/encoder handoff, bounded memory and CPU fallback; no zero-copy claim |
| Separate Studio shader and export filters | Quick isolated demo | Reject as product architecture unless a defined conformance suite proves their exact semantics and permitted error |

For the initial SDR release, choose and version a known working-space contract: declared input range/matrix/transfer → normalized RGB → effects → alpha composition → declared SDR output conversion. The professional reference target uses premultiplied alpha; an initial measured FFmpeg reference may use its explicitly versioned straight-alpha representation. Adapter representations must match the stored policy, never silently mix conventions. Vello premultiplied rasters require explicit conversion at a straight-alpha FFmpeg input, with defined alpha-zero RGB and no gamma transform of alpha. Decide where operations occur in linear versus encoded space, including exposure, contrast pivot, saturation luminance coefficients, transition blending and original-look interpolation. Unknown colour metadata needs an explicit documented assumption or an unsupported error; HDR cannot silently enter the SDR path. A valid8-bit MP4 is not proof that colours or alpha are correct.

Proposed persisted effect values use bounded finite units: exposure in milli-EV, opacity/intensity in0..1000, temperature in Kelvin or a versioned warm/cool mapping, positive contrast/saturation scales, and enumerated blend modes. Do not allow NaN, infinity, unchecked floats, raw expressions, GLSL/WGSL or arbitrary FFmpeg strings in project commands. Renderer-local floats derive from validated values. Select colour limits with fixtures before schema release; FFmpeg's permissive parameter ranges are not UX defaults.

Define the operation order once: decode/normalise → source crop → colour/look processing → spatial transform/resampling → masks/clip opacity/fades → transition/layer composition → output transfer/range/format. A later different order needs a versioned contract. Applying temperature before exposure and afterwards must not vary by backend. Start Normal alpha-over; add Multiply and Screen only with alpha/colour tests. Original LUT looks should interpolate original versus looked RGB while preserving alpha, rather than hiding a collection of undocumented parameter changes.

Original first look candidates: Neutral, Warm daylight, Cool evening, Soft monochrome and Clear contrast. These are proposals for original Lumeryn parameter recipes, not copied Clipchamp names/LUTs/previews. Store presetID/version and inspectable expanded values; edits remain independent of the catalogue. Use actual selected-footage thumbnails keyed by source digest, graph revision, effect version and preview time. No fake preview tiles.

## Exact visual fades and independent audio fades

All visual spans are half-open `[at_frame, at_frame + duration_frames)`. Store fade lengths in project frames, independent of export FPS. Zero disables a fade; a nonzero fade needs at least 2 rendered timeline samples under the proposed endpoint policy. Within a fade of D frames, define ramp positions explicitly as `k/(D-1)` for `k=0..D-1`, so the first/last frames reach their declared endpoints. A backend's similarly named duration parameter must be adapted/tested, not assumed equivalent.

For overlays, fade clip alpha to reveal lower layers. Fading toward black is a different operation and should be named accordingly. Initially reject overlapping in/out fade spans rather than silently choosing a multiplication/minimum rule. Changing clip length must report fades that no longer fit; no hidden truncation or re-timing. Split behaviour is explicit: preserve the evaluated visual where possible and report any rewritten envelopes in the dry-run result.

Audio fades belong to the shared AudioPlan. Map absolute timeline boundaries through existing `sample_boundary`, then use their difference for a clip's sample envelope; do not accumulate rounded per-frame sample durations. Independent gain/mute remains distinct from monitor gain. Define linear-amplitude fades initially and add equal-power crossfade as an explicit curve choice. Keep unclipped intermediate mixing and the existing documented final clipping policy; do not silently normalize overlaps. Test abrupt trim/fade edges and short inputs before advertising click-free output.

## Transitions without hidden source or duration changes

Represent a transition as its own stableID linking adjacent visual items on the same track, with type, duration, cut boundary, alignment, handle policy and audio policy. It is not a free-form effect attached to unrelated clips. The first release should provide Cross dissolve and a small set of directional wipes, with original native thumbnails and parameterized duration. Blur/burn/custom programmable transitions follow after baseline parity/performance tests.

Recommended initial policy is a fixed cut with verified handles. For cutC, choose L frames before and R after, `D=L+R`, and evaluate transition interval `[C-L,C+R)`. The outgoing source needs R additional tail frames; incoming needs L pre-roll frames. Validate both against their actual verified source ranges, including rate mapping. Resolve odd centred lengths deterministically, e.g. `L=floor(D/2), R=D-L`. The graph's total length and unrelated placements remain unchanged. A missing handle produces `insufficient_transition_handles` with available/required amounts; never clone edge frames or shorten the transition silently. Static image holds may have a separately defined reusable-source policy.

Example: a 30-frame centred dissolve at frame 300 covers `[285,315)`, needs 15 outgoing tail frames and 15 incoming pre-roll frames. Its first evaluated frame is fully outgoing and its last fully incoming under the declared D-sample endpoint rule. Trim/reorder/delete must remove or invalidate the associated transition atomically with a visible dry-run explanation. Reject intersecting transition spans on a middle clip until multiway composition is implemented.

An alternative explicit overlap-ripple mode can consume existing outgoing/incoming ends and shorten a sequence by D frames. It changes subsequent positions and possibly independent audio/story cues, so it requires an explicit ripple scope and a reviewed dry-run. It is not an automatic substitute for missing handles. Defer this mode in the first transition slice.

Audio defaults to cut unless the user chooses linked crossfade. Linked crossfades use the same declared span and verified audio handles. Equal-gain avoids boosting identical/correlated contributions; equal-power is a useful selectable choice for independent signals. Missing embedded audio follows the existing declared silence policy; independent music must not unexpectedly fade because a video transition was added. FFmpeg's graph duration/PTS behaviour must be conformed to this Rust contract, particularly when several transitions are chained.

## Rational speed, linked audio and freeze frames

Speed is a positive reduced rational p/q, stored separately from FPS and source range. Proposed initial speeds: 0.5×,1×,2× plus bounded numeric rational input after tests; no reverse, ramping or optical-flow promise. A speed edit states its anchor and ripple scope. Default beginner action preserves the source selection and starts at the same timeline frame, recalculates duration, and requires an explicit ripple choice if subsequent placements would collide. Return the planned duration and all moved IDs before writing. Do not implement speed by changing project FPS.

For project FPS Np/Dp, source FPS Ns/Ds and local timeline frame k, the proposed hold-sampling offset is:

`floor(k * Ns * Dp * p / (Ds * Np * q))`.

For S selected source frames, proposed derived duration is:

`ceil(S * Np * Ds * q / (Ns * Dp * p))`.

Use checked integer arithmetic and return exact source/target ranges. These integer-k formulas describe timeline-grid samples; authoritative output evaluation takes exact rational local time, retaining that fraction until the final source-frame floor. Fade ramps similarly use clamped rational progress with a defined endpoint policy, not an early integer-frame rounding. Test a30FPS timeline exported at60FPS with an animated midpoint. Evaluate the final duration and selected frames once in Rust; do not infer it from rounded FFmpeg timestamps. Other samplers can follow under separate capability IDs. Separating selected source length from timeline duration is essential: today'sClip assumes they are equal. Mixed-source FPS requires this mapping/proven source timestamps and its own acceptance; adding a speed slider must not bypass current matching-CFR validation.

Linked audio has an explicit policy: preserve pitch, change pitch or mute; initial default preserve pitch, with a clear error if unavailable. Independent music stays at its own rate unless explicitly included. Use the existing controlled audio decoder and generated moderate `atempo` factors rather than a new broad decoding stack. Count/trim produced output samples to the exact allocated half-open span, measure algorithm delay and transient behaviour, and reject significant shortages instead of hiding them with arbitrary silence. Realtime speed scrubbing is a later quality/performance gate. A simple resample is only valid for the explicitly requested pitch-changing policy.

Freeze is a distinct source-frame hold item: source mediaID/digest, exact verified source frame, timeline start/duration, transform/effects and explicit audio policy. Initially create a silent hold; offer keep-background-audio separately. Source frame must be inside verified source extent. Do not fake a freeze by speed 0 or import an untracked screenshot. Derived caches are optional keyed results, not authoritative assets. Freeze insertion/split/ripple is an atomic shared service operation and undo restores the previous timeline. Test the exact selected image for every output frame.

## Studio and agent contract

Right contextual drawers: Adjust colour with grouped sliders/numeric values/reset; Filters with searchable original thumbnails and one Intensity control; Fade with separate visual/audio controls and clear units; Speed with multiplier/pitch policy/resulting duration; Transition with type/duration/handle diagnostics. A selected transition opens its properties; a selected clip shows clip properties. Do not show every advanced field simultaneously. Preserve plain readable numerals, icon tooltips, keyboard operation and usable compact/high-DPI layouts.

Slider drags evaluate one cancellable transient plan locally for visual feedback and commit one authoritative transaction on release. The transient plan must use the same evaluator and visibly remain uncommitted; cancel/stale revision/Escape discard it. Avoid one history row per pointer move, blocking UI decoding or silently overwriting an agent edit. Render/thumbnail workers retain revision+epoch tags and bounded queues; obsolete work is discarded.

Proposed CLI operation names are design examples, not current commands: `set_clip_colour`, `set_clip_fades`, `apply_look`, `add_transition`, `set_clip_speed`, `insert_freeze`. Ship each with strict schema, capability IDs/limits, inspect output, dry-run, expected revision, idempotency, undo/redo, events and native controls in the same milestone. Reports must identify effect order, source range, calculated duration, moved/deleted transition IDs and runtime requirements. Omitted fields cannot mean accidental reset; use deliberate full replacement or a typed patch with explicit reset fields. No semantic-only CLI capability that export ignores.

## Milestones and acceptance

| Slice | Deliverable | Required acceptance before completion |
| --- | --- | --- |
| ET-01, after layers/images | Normal opacity and visual/audio fades through shared plans | Half-open endpoints, transparent-edge fixtures, exact sample envelopes, dry-run/conflicts/history/migration, both clients and decoded output |
| ET-02 | SDR exposure/contrast/saturation/temperature/reset plus 5 original looks | Versioned formula/order, identity no-op, CPU/backend pixel tolerance, source-specific thumbnail invalidation, UHD output and native slider interaction |
| ET-03 | Fixed-cut Cross dissolve and directional wipes | Verified handles, odd durations, source-bound validation, chained cut boundaries, no unexpected length change, optional linked-audio markers |
| ET-04 | Rational 0.5×/1×/2× speed and explicit linked-audio pitch policy | Source mapping/source length vs output length, duration/ripple dry-run, repeated seek/preview/export equivalence, actual tempo quality and A/V markers |
| ET-05 | Exact-source freeze insertion | Selected frame equality, silence/background-audio policy, hold boundaries, trim/reorder/undo, real native preview and fully decoded export |
| ET-06, benchmark-gated | GPU effects/composition, expanded blends/looks/transitions and speed range | Headless/native/CPU parity, bounded copies/memory/device recovery; quantified improvement on agreed workloads |

Do not block early animated explainers on all ET families: layered images and editable text/scenes have earlier product priority. Conversely, do not declare complete-timeline4K support for newly added families until their actual composition exports pass.

Tests must include:

- Pure graph properties: valid ranges, overflow, no NaN, endpoint identity, positive duration, stableIDs, source immutability, full-history migration/rollback and old binary rejection.
- Synthetic visual frames with known colour patches, alpha/checkerboards, asymmetric corner markers and frame numbers; compare pre-encode frames exactly or within a documented backend tolerance. Lossy encoded checks need separate thresholds. Alpha haloes, gamma shifts or missing layers cannot pass merely through average similarity.
- Flash/click/impulse/tone fixtures around cuts/fades/transitions/speed boundaries. Compare shared pre-device/pre-encode PCM plus decoded AAC with known priming/alignment tolerance; measure drift at fractional FPS and long durations. Speaker/display latency is a separate physical test.
- Actual 3840×2160 and 2160×3840 exports at 24/25/30/60 FPS, plus exact rational29.97/59.94 timing fixtures. Verify frame count, duration, dimensions, colour samples, complete composition, audio extent and source hashes. A software path must work without a GPU; unavailable hardware must give an honest fallback/error.
- Native pointer and keyboard transactions at wide/compact/high-DPI sizes, sidebar scrolling, cancellation mid-render, stale inspector revision, two clients and undo/redo. No launch-only acceptance or claim of zero bugs.

Measure cold/warm 1080p/UHD scenes with 1/2/4 layers, identity versus colour/transition, CPU reference versus GPU, active waveform/import jobs and repeated seek. Record tool/driver/hardware, copies/allocations/RAM/VRAM, render throughput, seek-to-present, underruns/dropped frames and p50/p95/p99 UI timing. Set budgets after the baseline; none are achieved by this plan. Proxy effects render at preview size, while export evaluates originals and resolution-dependent operations at final size.

## Code ownership and safety

Implementation hotspots: core model/time (versioned typed graph/ranges), core storage (migration/history), shared visual plan/evaluator (new cohesive module only when needed), core render and preview plan/decode (shared-plan adapters), core audio_plan (fades/tempo/transition policies), service contract/jobs/events, CLI schema/commands, Studio app/timeline/gpu. Assign explicit owners and merge schema work first; avoid concurrent edits to coremodel/render/audio_plan across specialists.

Use enum variants/newtypes for mutually exclusive speed/freeze/transition policies and distinct source/timeline/sample units. Return explicit errors for missing sources, unsupported colours/filter/backend, invalid parameters, insufficient handles, source changes, stale revisions and budget exhaustion. Preserve current subprocess argv isolation, file-only protocols, bounded logs/timeouts, generation cancellation, create-new publication and full-decode checks. Fonts/LUTs/preset assets are local audited data; never evaluate embedded scripts or user expressions. Do not log private filenames/text/source paths unnecessarily.

Advance capability/version descriptors only with the executable shared slice. Re-run repository fmt/strictClippy/lockedtests/media/deny and the relevant real native acceptance before local milestone commit. No new purchase, mandatory model, unsafe optimisation or bundled GPL runtime is required by this proposal.
