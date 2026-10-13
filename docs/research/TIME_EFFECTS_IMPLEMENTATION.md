# Transition, speed and freeze implementation contract

Research/evaluator date:2026-10-12. This report distinguishes implemented headless arithmetic from pending shipped editing actions.

## Implemented isolated API

`PlaybackSpeed::{Half,Normal,Double}` has exact ratio and numeric-only FFmpeg tempo values. `TimeEffect::{Normal,Speed{speed,audio},Freeze{source_frame}}` is strict serializable data. `validate` proves source bounds; `source_at` takes animation::RationalFrame and returns an optional exact source position in the half-open clip; `duration_for_source` demands exact integer duration. `is_silent` explicitly covers muted speed and freezes. Stored speed never silently ripples another object.

`FixedCutTransition::{kind,duration_frames}` provides explicit handles, `validate_sources`, exact `sources_at`, half-open `progress` and checked `incoming_weight` for CrossDissolve/WipeLeft/WipeRight/WipeUp/WipeDown. Incoming and outgoing source positions are independently bound to verified counts. `blend_rgba` is an original CPU pixel reference with premultiplied-alpha interpolation and existing encoded-SDR policy.

## Shared compiler admission

1. Preserve graph/source UUIDs, revision and source proofs. Only verified CFR video at the project's exact rate is admitted initially. PNG freeze and arbitrary VFR/time-warp are unsupported.
2. Speed preserves selected source duration, then computes an exact timeline duration. Reject2xodd ranges; require explicit ripple scope or conflict-free new span. Graph edits changing clip range/duration must validate speed again. CLI dry-run reports the new duration and affected objects.
3. Preserve-pitch audio uses the existing canonical48kdecode/mix contract and a bounded tempo preparation step. Match the exact destination sample range with explicit padding/trim, and prove resampling/tempo tail behaviour. Mute emits exact silence. Freeze is silent and repeats one decoded exact source picture.
4. Transition admits two adjoining normal-speed moving clips only, with explicit source handles on both sides of the fixed cut. Reject overlapping transition windows and compositions exceeding admitted source counts. Keep project duration and cut unchanged. No hidden clip splitting/repetition/source mutation.
5. Sample time is output_index * project_fps/output_fps, represented rationally. Decode picks the admitted source sample according to one documented floor/hold policy; preview and export share it. No intermediate integer timeline quantisation for60FPSexports from30FPSprojects.
6. Generated FFmpeg graphs may include only compiler-owned numeric expressions. Never persist/evaluate arbitrary expression/script text supplied by projects. Lower all surfaces to consistent geometry/timebase/pixel format before any transition.

## Native and agent delivery

A right Speed drawer offers0.5x/1x/2x, Preserve pitch/Mute, resulting duration and explicit affected span. Freeze selects a source frame and hold duration, with a clear silent policy. A left Transitions gallery uses original rendered previews and drag-to-adjacent-cut; the right drawer edits kind/duration, shows missing handles with a remedy, and offers Remove. Controls send the same typed mutation as CLI/batch/schema, one commit per gesture, expected-revision/idempotency/undo. Do not add decorative controls before rendering exists.

## Verification actually executed

`cargo test -p lumeryn-core --locked --test time_effects`:7passed. `cargo clippy -p lumeryn-core --locked --test time_effects -- -D warnings`:passed. Owned module/test rustfmt:passed. Warm target `.lumeryn-scratch/six-transform-target`; no separate target/media pack was created.

Next: full integration plus actual asymmetric frame/flash/pulse/tone generated fixtures, freeze byte equality, decode frame counts24/25/30/60 and rational rates, cross-dissolve/wipe endpoints and chained-cut handle rejection, exact canonical audio length/pitch and bounded drift, Windows cancellation/no-console tests and actual sidebar pointer inspection. Further fixture/build work paused for owner workspace disk review; no unmeasured performance claim.

## Current official research

The official filter reference documents atempo and timebase-oriented setpts, explicit cloned-frame tpad and matching-input xfade requirements. [Maintainer reference](https://raw.githubusercontent.com/FFmpeg/FFmpeg/master/doc/filters.texi). FFmpeg distribution obligations and optional GPL/codec-patent issues remain reviewed separately; no new binary/library is bundled. [Official legal page](https://ffmpeg.org/legal.html). The public rendered filter HTML timed out during this check, so the current maintainer documentation source was read directly.

FFmpeg is already maintained/configured on Windows in this application; staying behind its existing worker factory keeps process startup/cancellation/diagnostics consistent. A wholly new MLT/direct-libav/GPU time engine has no measured benefit at this stage. The evaluator adds no runtime process, mandatory model, unsafe code or new dependency. CPU pixel blending adds per-pixel arithmetic; sustained1080p/UHDthroughput and peak surfaces still need measurement before claiming interactive performance.
Rubber Band is a credible maintained native C++ alternative and offers Windows binaries, but its maintainer specifies GPL or purchased commercial licensing for proprietary redistribution. No library or executable is added and no purchase is authorized. [Official licensing/platform page](https://breakfastquay.com/rubberband/).
## Integration update — 2026-10-12

Schema5 graph integration now carries canonical nonNormal `Project.time_effects` for moving primary/overlay video and `SetTimeEffect` through the existing source-proof/transaction/receipt flow. Changing time policy preserves current duration and placement; clips must explicitly change duration/source ranges in a batch when desired. Duplicate copies the policy, delete removes it, and split rejects time-remapped clips until rebasing is implemented. Root owns CLI/sidebar and clipboard rejection integration.

AudioPlan carries private per-contribution speed and exact source lengths without changing the independent AudioSegment API. Source lengths floor the exact rational source endpoint once, independent of destination placement. atempo0.5/2 is followed by destination-length trim/padding for linked audio; freeze/mute contributes no linked source. Preview/export/seek consume the same plan from origin then trim. This guarantees declared sample length, not perceptually perfect time stretching; atempo tail padding and transient quality require actual tone/pulse/footage acceptance.

VisualPlan Picture carries TimeEffect; source admission validates exact rational consumption, source sampling floors the exact mapped position and retimed clips always route through composition. FFmpeg lower trims ceil(consumed source frames), maps N/speed to global sample timing before output FPS/effects/animation, and holds an exact trimmed frame for freezes. Owner-coordinated builds/tests are pending; no new target, fixture or Cargo invocation was started by this specialist during integration. New time_graph/time_audio/time_composition acceptance tests are ready for serialized root execution. Transition mutation/rendering remains pending.
## Fixed-cut transition integration update — 2026-10-12

A bounded schema5 TransitionEdge graph and SetTransition/DeleteTransition operations are now implemented for one transition between exactly two adjacent primary clips beginning at frame0. Project validation explicitly rejects overlays, transforms/effects, scenes, animation, time effects or additional clips while a transition exists. Both source handles must exist and fit adjoining clip spans; IDs remain stable and total timeline length does not change. Both source digests are checked during preparation and both timestamp/size/graph proofs immediately before commit. Broader combinations remain unsupported rather than rendering incorrectly.

The private-field TransitionPlan validates the project and sources. Its generated single-worker FFmpeg program normalises two source ranges to consistent contain/pad geometry, CFR output and a common rational clock before xfade; a planned clip end stays fixed. Cross dissolve and four directional wipes are implemented. Linked and independent audio retain existing hard-cut scheduling; audio crossfades are not promised. Transition graph and actual decoded colour/direction tests were added for serialized root execution; acceptance remains pending until those tests and shared preview/export/native workflows pass.

[Official xfade implementation/licence](https://raw.githubusercontent.com/FFmpeg/FFmpeg/master/libavfilter/vf_xfade.c) is LGPL2.1-or-later code in the external FFmpeg tool, not copied into Lumeryn. Using that filter does not remove licensing obligations of other enabled build components. The backend has a 3840-pixel surface bound and rejects an unrepresentable common timebase. Memory and UHD throughput still require actual measurements.
## Executed time/transition acceptance update — 2026-10-12

The serialized shared `.lumeryn-scratch/composition-target` run with configured FFmpeg9.0.2 passed15focused tests across time_effects, time_graph, time_audio, time_composition, transition_graph and transition_composition. Actual decoded fixtures prove0.5x/2x/freeze source frame selection at30/60FPS, exact canonical PCM length,440Hzpitch preservation in a stable interior, identical seek suffix and silent freeze. All five transition variants preserve12/24output samples at30/60FPSand match actual red/blue first/last/midpoint/direction pixels with source hashes unchanged.

An actual2xfirst-frame failure identified integer source-timebase rounding (N/2truncated). The shared lowerer now expands its common rational clock for half/double speeds and timestamps source frames exactly before output sampling; the frame-index test passes after correction. Synthetic64x64and8x8fixtures do not establish UHD performance, natural-footage transient quality, Windows speaker/display sync or complete native editing workflows. A further transition transactional proof test was added after this run and awaits the final serialized gate.
