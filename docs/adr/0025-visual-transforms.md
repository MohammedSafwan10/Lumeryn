# ADR0025: Checked fixed-surface visual transforms

Date: 2026-10-12. Status: accepted reference contract, isolated helper verification complete; integrated delivery pending.

## Decision

Store crop, contain/cover, clockwise millidegree rotation, normalized fixed-point pivot, flips and Normal opacity as typed non-executable settings on stable visual placement IDs. Primary and overlay placements must share one contract. Source crop endpoints are validated against verified media dimensions. Parent schema4 integration must retain all historical IDs/revisions/command bytes and require explicit migration.

Transform order is crop→flip→premultiplied fit→fixed-rectangle rotation→straight-alpha Normal opacity. Rotation clips at the destination rectangle; no silent rectangle enlargement or timing/audio change. Preserve the existing identity contain route exactly. Explicit transforms use premultiplied bilinear encoded-SDR interpolation; source/target surfaces remain3840-per-dimension bounded.

Use existing external FFmpeg filters. Center quarter turns use transpose/flips; centered arbitrary angles use native rotate. Noncenter inverse-affine geq is a bounded correctness reference capped720per target dimension because the measured UHD cost is unacceptable for unrestricted preview. Return a concrete unsupported error above that cap. Rendering at a lower proxy resolution cannot establish unrestricted final-resolution support.

## Reasons and alternatives

Per-frame expression sampling is flexible and safe but measured63.4times slower than the native centered rotation in the isolatedUHD3frame total-cost fixture. A fast centered path is justified by evidence. Expanded intermediate surfaces violate the budget; separate Studio shaders risk preview/export divergence. Future GPU/remap work must preserve pixel-center/pivot/alpha policy and demonstrate total-cost gains before replacing the reference.

No new transform dependency, unsafe optimization or mandatory optional runtime is added. External FFmpeg binary redistribution/configuration and codec rights retain their separate license review; consult [official FFmpeg filters](https://www.ffmpeg.org/ffmpeg-filters.html) and [official legal documentation](https://ffmpeg.org/legal.html). No proprietary Clipchamp code or preset assets are used.

## Evidence and limits

[Research, exact contract and measurements](../research/TRANSFORM_IMPLEMENTATION.md) records the generated actual pixel tests, strict focused Clippy and current bounds. Native handles, CLI discovery, project persistence/migration, full preview/export/UHD parity, physical DPI and independent integrated review are required before reportingV2done. Arbitrary offcenterUHD, general SAR, HDR and realtime guarantees remain unimplemented/unverified.
