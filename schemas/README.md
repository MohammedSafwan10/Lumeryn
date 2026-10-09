# Shared command schemas

`batch.schema.json` is generated from the current compiled `lumeryn schema` result (Schemars/shared Rust Batch/Edit types), draft2020-12. It defines structural atomic editing input. Runtime additionally enforces semantic IDs/ranges/rates/revisions/path/asset constraints. Regenerate after changing command types and review protocol/schema compatibility.

The CLI response wraps schemas under `result`; the saved file contains only the schema. Current edit variants are import, add_clip and trim_clip. Semantic SceneSpec, service envelopes, model manifests and expanded capability schemas are planned, not represented as implemented in this artifact.

