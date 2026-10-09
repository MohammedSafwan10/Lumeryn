# LUMERYN — CRITICAL PRODUCT VISION UPDATE

Continue developing the existing Lumeryn project. Do not restart or discard completed work.

This instruction clarifies our PRIMARY product purpose and must be incorporated into PRODUCT_SPEC.md, ARCHITECTURE.md, TASKS.md, AGENTS.md, and the implementation roadmap.

## 1. The primary user is NOT a professional editor

Lumeryn is being designed primarily for people who have little or no video editing knowledge.

The user should be able to describe what they want in natural language to an external coding agent such as Codex.

The agent should then autonomously plan, create, edit, preview, review, refine, and export a professionally presented video using Lumeryn's CLI.

The user may provide their own clips, images, audio, references, or merely a topic.

The external agent may use its own available web search, file download, image generation, and research tools to obtain additional authorized assets.

Lumeryn is responsible for professional video creation and editing operations through its Rust engine, CLI, and synchronized desktop Studio.

Do not embed an unnecessary competing AI agent or MCP layer into Lumeryn.

The desktop editor must remain fully usable by professional human editors, but expert editing knowledge must NEVER be a prerequisite for using Lumeryn through a capable external agent.

## 2. Autonomous creative direction is a first-class use case

Support agent-directed production workflows covering:

- Understanding the creative brief.
- Defining the audience and video objective.
- Planning the narrative.
- Writing or refining scripts.
- Creating a scene-by-scene storyboard.
- Choosing suitable editing and animation techniques.
- Planning narration, dialogue, music and sound effects.
- Creating and importing visual assets.
- Assembling editable scenes.
- Synchronizing animations with narration.
- Rendering previews.
- Reviewing actual visual and audio output.
- Correcting mistakes.
- Exporting a polished final video.

The agent should make sensible creative decisions without asking the user about every technical detail.

Ask the user only when an essential preference, permission or decision cannot reasonably be inferred.

Provide understandable defaults and allow users to request changes in ordinary language.

## 3. Animated explainers are a flagship product category

One of Lumeryn's primary purposes is creating highly polished educational and explanatory videos for YouTube, Instagram Reels, Shorts and other platforms.

Prioritize the ability to create:

- Animated diagrams.
- Technical concept explanations.
- Mathematical visualizations.
- Physics demonstrations.
- Science animations.
- Programming and AI explanations.
- Infographics.
- Animated timelines.
- Comparison visuals.
- Kinetic typography.
- 2D animated illustrations.
- Animated charts and graphs.
- Procedural shapes and connectors.
- Arrows and flow diagrams.
- Step-by-step visual tutorials.
- Screen recording annotations.
- Product and architecture explainers.
- 2.5D parallax and camera movements.
- 3D teaching demonstrations using Blender.
- After Effects-style cinematic motion graphics.

These must be generated programmatically by agents through structured Lumeryn CLI operations.

Develop native Rust scene, animation and compositing primitives for long-term control and consistency.

Research **Manim Community Edition** as an optional external renderer for mathematical and educational animations.

Research **Motion Canvas** as an optional external renderer for TypeScript-driven vector animation and voiceover synchronization.

Research SVG, Lottie/dotLottie, vector graphics and formula-rendering formats as compatible input or interchange mechanisms.

Review licensing and commercial-distribution implications before adopting any external backend.

Do not make every backend mandatory.

Avoid creating a second independent project/timeline system for external animations. Generated animation assets, editable parameters and source specifications must integrate with Lumeryn's project architecture.

## 4. Build a semantic creative scene API

Agents should not need to manually calculate every animation keyframe for common creative tasks.

Provide typed high-level creative operations and reusable scene specifications.

Examples:

- Create an animated concept explanation.
- Reveal a graph step by step.
- Animate arrows between architecture components.
- Create a comparison layout.
- Animate a timeline of events.
- Build kinetic typography.
- Add cinematic 3D titles.
- Highlight a specific object.
- Synchronize a graphic with a spoken phrase.
- Apply a coordinated motion-design preset.
- Adapt a composition for portrait or landscape.

These should generate real editable timeline and scene objects.

Support low-level keyframe control as well, so external agents can create original effects instead of being restricted to templates.

Prefer a declarative, versioned scene specification with stable object IDs.

## 5. Professional motion design principles

Professional-looking results must be treated as a design objective, not a claim automatically guaranteed by the engine.

Provide systems for:

- Consistent typography.
- Layout constraints.
- Spacing and alignment.
- Visual hierarchy.
- Theme palettes.
- Animation easing.
- Motion timing.
- Reusable transitions.
- Composition balance.
- Safe areas.
- Audio synchronization.
- Scene continuity.
- Professional title treatments.
- Deliberate use of visual effects.

Avoid overloading videos with unnecessary transitions, motion, excessive captions, random animations or repetitive templates.

Design animation primitives to help an agent make thoughtful and purposeful creative choices.

## 6. Creative quality assurance

Develop a CLI-accessible quality inspection workflow.

The external agent must be able to:

1. Render low-resolution previews.
2. Extract key frames and scene contact sheets.
3. Inspect actual rendered results.
4. Read timing and composition metadata.
5. Detect missing assets and render failures.
6. Check subtitle timing and safe-area placement.
7. Detect text overflow and overlapping elements.
8. Check audio peaks and narration intelligibility.
9. Identify black frames, unexpected gaps and broken transitions.
10. Revise selected scenes without regenerating the entire project.

Expose structured quality reports.

Allow an agent with vision capabilities to inspect preview images and clips and decide which artistic improvements are necessary.

Use bounded review iterations and stop conditions to avoid infinite revision loops.

Do not fabricate subjective quality scores or claim that automated metrics prove professional artistic quality.

## 7. Different levels of user involvement

Support three product workflows.

**Autonomous**

The user supplies a brief and optionally assets. The agent creates the video and reports the completed result, requesting intervention only when required.

**Review and Refine**

The agent prepares a storyboard or preview, the user gives feedback in ordinary language, and the agent revises the relevant editable scenes.

**Professional Manual Editing**

The human edits the same project directly through Lumeryn Studio, with normal professional controls.

All three operate on the same Rust project engine and timeline.

These are workflows enabled by external agents and Lumeryn capabilities; they do not require a built-in chat agent.

## 8. Original content and monetization support

Design the product to support original YouTube and short-form educational content.

Provide:

- 16:9, 9:16 and square project presets.
- Portrait-safe layout adaptation.
- Readable subtitles and graphics.
- Video hooks and opening sequences.
- Optional closing cards and calls to action.
- Visual consistency across series.
- Reusable brand kits.
- Intro/outro presets.
- Thumbnail frame extraction.
- Export presets.
- Asset provenance and licensing metadata.

Do not automatically reuse copyrighted footage or music without appropriate authorization.

The agent should distinguish verified educational facts from uncertain claims.

Do not promise YouTube monetization, audience growth or human-indistinguishable output.

## 9. Product acceptance demonstrations

Beyond the existing technical milestones, add three end-to-end creative acceptance projects:

**Demo A — Animated educational explainer**

An agent creates a 60-second original explanation of how a neural network works, with synchronized narration, animated nodes, connecting lines, labels, diagrams, typography and sound.

**Demo B — Cinematic travel Reel**

An agent imports supplied footage, selects useful shots, creates smooth transitions, applies color treatments, adds music, narration and captions, and exports a 9:16 video.

**Demo C — 3D technical explanation**

An agent creates an animated 3D product or mechanical visualization through the optional Blender backend and composites it into a Lumeryn timeline with narration and graphic labels.

Each demo must remain editable, not merely produce a flattened final video.

Measure actual completion time, required manual intervention, visual correctness, hardware consumption and rendering reliability.

## 10. Priority update

Keep the current engineering foundation intact.

But update development prioritization so that after a reliable timeline, rendering pipeline, CLI and live Studio synchronization are working, we quickly deliver:

1. Native scene composition.
2. Professional text and vector graphics.
3. Animation/keyframe primitives.
4. Reusable explanatory animation components.
5. Narration synchronization.
6. Scene-level preview and correction.
7. A complete automated animated-explainer demo.

Do not postpone meaningful creative capabilities until every traditional professional editor feature is implemented.

The ability for an agent to create an impressive, original, editable educational video is a major early product milestone.

The long-term vision is:

**Lumeryn is a professional video editing and motion-design engine built for humans, but designed so AI coding agents can do the complete creative production work for users who do not know how to edit.**

Delegate independent research and architecture tasks to subagents where appropriate. Update the specifications, roadmap and tasks, document significant technology decisions, then continue development from the current state.
