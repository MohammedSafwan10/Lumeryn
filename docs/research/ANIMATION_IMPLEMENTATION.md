# Animation implementation experiment

Date: 2026-10-12. This is a tested headless foundation and original blueprint expansion, **not a shipped project animation/render capability**. Root integration must add persisted commands, schema4 migration, scene adapter, CLI/native discovery, actual preview/export pixels and acceptance before enabling it.

## Decision and sources

Use existing Rust/Serde/Schemars, with no new animation runtime, scripting language or library. [W3C CSS Easing Level1](https://www.w3.org/TR/css-easing-1/) describes pure easing that maps progress to progress. Adopt that seek-independent principle, but do not claim CSS compatibility: initial interpolation is Hold, Linear and explicitly named SmoothStep `p*p*(3-2*p)` for p in [0,1]. This bounded polynomial has endpoint0/1, midpoint0.5 and zero endpoint derivative; arbitrary Bezier/springs/expressions and overshoot remain unsupported. [Khronos smoothstep reference](https://registry.khronos.org/OpenGL-Refpages/gl4/html/smoothstep.xhtml) is the official mathematical reference (web extraction returned unsupported XHTML; the formula is independently implemented/tested here). No reference implementation or artwork is copied.

The mature alternative is a general animation/tween library; another is UI-driven egui animation. Both introduce unnecessary scheduling/state and do not own exact export sampling. An animation library could later supply optional authored curves after profiling and license review; UI timing must never decide project appearance. CPU-only evaluation is platform-neutral and has no native runtime installation. Current checked evaluation is bounded at6channels and128keys/channel, O(totalkeys) plus validation per call; validate once/compile binary-search curves is a later measured improvement. No GPU performance claim or benchmark exists for this experiment.

## Contract

`Animation { channels: Vec<AnimationChannel> }`, channel `property` is x/y/scale_x/scale_y/rotation/opacity, and keys are `{at_frame:u64,value:i64,interpolation}`. Fields reject unknown JSON. Values are integer **milli-pixels**, **millionths** for scales/opacity, and clockwise **millidegrees** for rotation. X/Y±8192pixels, scale0.001..8, rotation±360degrees, opacity0..1. Each property occurs at most once; keys are nonempty, strictly increasing, at most128, and within0..clipduration inclusive. Endpoint keys define a mathematical finish while clip intervals remain half-open. Missing channels preserve static properties; prefirst/postlast samples hold nearest value. A left key selects interpolation toward the right key; exact right-key samples switch to that key's value. Direct rotation interpolation is deliberate; shortest-angle/turn-count behavior is not inferred.

`RationalFrame` uses reduced checked u128 values. `from_output_frame` preserves output index * output denominator * project numerator / (output numerator * project denominator). No frame-flooring or incremental state. `in_clip` subtracts exact clip origin and returns None outside [start,end); overflow is an explicit error. Only interpolation's normalized ratio becomes f64, so equal rational times and backward seeks produce identical results within one build. Crossarchitecture bit-exact output is not claimed; pixel goldens need tolerances. The transform adapter rounds once to integer destination/rotation/opacity fields; basis-point opacity is a separate lower-resolution target, never a persisted-key rewrite.

## Original starters

Five `Starter` blueprints expand into two ordinary editable objects each, with two caller-owned nonnil distinct UUIDs for retry stability. No IDs are allocated on replay. Rectangles and text/media slots are declarative data; font resolution belongs to the audited typography adapter, PIP source resolution belongs to the verified media adapter. There are no baked screenshots, copied stock assets, hidden placeholder assets or executable expressions.

| Starter | Objects | Design |
|---|---|---|
| Clean title | transparent rect + title | generous horizontal margins, high contrast type, quiet fade |
| Lower third | translucent dark rect + title | lower-left label with inset text |
| Callout | mint rect + dark text | upper-right explanatory card |
| Chapter card | dark rect + mint text | large central chapter heading |
| Picture-in-picture | mint backing + required media slot | lower-right contained frame |

The checked builder accepts320..8192 by180..8192 canvas,2..MAX_FRAMES duration and1..4096UTF8byte noncontrol title. Enter opacity eases from0 to1 over min(duration,16)/2 projectframes, then holds. These original native-blueprint designs add no asset license obligation. Project fonts/media still require provenance/licensing checks; no font is silently selected or bundled.

## Executed evidence and limits

Five focused integration tests passed on the local Windows Rust toolchain: exact29.97timeline/60output ratio and backward-seek equivalence; half-open/hold boundaries; bounded smoothstep monotonicity over601samples; rejected order/count/bounds/duplicate/overflow/unknown JSON; repeatable editable expansion of all five starters. No new proptest dependency was introduced; exhaustive finite progress sampling plus targeted rational tests cover this bounded evaluator. Pending: root full gates, scene adapter and command/history tests, actual goldens/export-rate animation60FPS/portraitUHD, font/media failures and native keyframe gestures. This file does not certify those remaining gates.
