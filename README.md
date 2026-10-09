# Lumeryn

Proprietary local video editing engine and agent-first Rust CLI, with a planned synchronized Tauri Studio. NEXDARK LABS is a pending business brand. No public source license is granted.

The primary audience is people who brief an external coding agent in ordinary language. The product aims to make original, editable animated explainers and videos through native creative operations, with the same project available to professional human editors. This milestone implements the technical editing/export foundation; creative scene/animation and Studio capabilities remain staged.

Requirements: Rust 1.97.1 for this verified toolchain and locally installed FFmpeg/ffprobe. `cargo run -p lumeryn-cli -- --help` starts command discovery. Install with `cargo install --path crates/lumeryn-cli --locked`.

See [CLI workflows](docs/cli/WORKFLOWS.md), [actual progress](PROGRESS.md), [task board](TASKS.md), [architecture](ARCHITECTURE.md), and the complete [product specification](docs/PRODUCT_SPEC.md). The first slice is a constrained editing/export foundation, not professional-editor parity. No Studio or AI backend is installed automatically.
