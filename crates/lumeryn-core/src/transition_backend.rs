//! Bounded two-clip fixed-cut video transitions. Linked audio retains hard cuts.
use crate::{
    Error, Project, Result,
    media::{content_hash, local_file},
    time::FrameRate,
    time_effects::{FixedCutTransition, TransitionKind},
    visual_backend::VisualProgram,
};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};
#[derive(Debug, Clone)]
pub struct TransitionPlan {
    sources: [PathBuf; 2],
    digests: [String; 2],
    rate: FrameRate,
    source_in: [u64; 2],
    durations: [u64; 2],
    before: u64,
    after: u64,
    kind: TransitionKind,
    source_sizes: [(u32, u32); 2],
}
impl TransitionPlan {
    pub async fn from_project(project: &Project, database: &Path) -> Result<Self> {
        project.validate()?;
        let edge = project
            .transitions
            .first()
            .ok_or_else(|| Error::NotFound("transition".into()))?;
        let outgoing = project
            .clips
            .iter()
            .find(|clip| clip.id == edge.outgoing_clip_id)
            .ok_or_else(|| Error::NotFound("outgoing clip".into()))?;
        let incoming = project
            .clips
            .iter()
            .find(|clip| clip.id == edge.incoming_clip_id)
            .ok_or_else(|| Error::NotFound("incoming clip".into()))?;
        let parent = database
            .parent()
            .ok_or_else(|| Error::Invalid("project parent missing".into()))?;
        let source =
            |id| -> Result<(PathBuf, String, (u32, u32))> {
                let media = project
                    .media
                    .iter()
                    .find(|media| media.id == id)
                    .ok_or_else(|| Error::NotFound("transition media".into()))?;
                let video = media
                    .metadata
                    .video
                    .as_ref()
                    .ok_or_else(|| Error::Invalid("transition video missing".into()))?;
                if video.width > 3840 || video.height > 3840 {
                    return Err(Error::Unsupported(
                        "transition source exceeds bounded UHD surface".into(),
                    ));
                }
                let path = local_file(&parent.join(&media.path))?;
                if std::fs::metadata(&path)?.len() != media.metadata.size_bytes {
                    return Err(Error::Invalid("transition source size changed".into()));
                }
                Ok((
                    path,
                    media.metadata.blake3.clone().ok_or_else(|| {
                        Error::Invalid("transition requires source digest".into())
                    })?,
                    (video.width, video.height),
                ))
            };
        let a = source(outgoing.media_id)?;
        let b = source(incoming.media_id)?;
        let handles = FixedCutTransition {
            kind: edge.kind,
            duration_frames: edge.duration_frames,
        }
        .handles()?;
        let plan = Self {
            sources: [a.0, b.0],
            digests: [a.1, b.1],
            rate: project.frame_rate,
            source_in: [outgoing.source_in_frame, incoming.source_in_frame],
            durations: [outgoing.duration_frames, incoming.duration_frames],
            before: handles.before_cut,
            after: handles.after_cut,
            kind: edge.kind,
            source_sizes: [a.2, b.2],
        };
        plan.verify_sources().await?;
        Ok(plan)
    }
    pub async fn verify_sources(&self) -> Result<()> {
        for (path, digest) in self.sources.iter().zip(&self.digests) {
            if content_hash(path).await? != *digest {
                return Err(Error::Invalid(
                    "transition source changed during rendering".into(),
                ));
            }
        }
        Ok(())
    }
}
pub fn lower(
    plan: &TransitionPlan,
    width: u32,
    height: u32,
    output_rate: FrameRate,
) -> Result<VisualProgram> {
    if width < 2 || height < 2 || width > 3840 || height > 3840 {
        return Err(Error::Unsupported(
            "transitions require bounded 2..3840 output dimensions".into(),
        ));
    }
    let mut a = u64::from(plan.rate.numerator());
    let mut b = u64::from(output_rate.numerator());
    while b != 0 {
        (a, b) = (b, a % b);
    }
    let clock = u64::from(plan.rate.numerator()) / a * u64::from(output_rate.numerator());
    if clock > i32::MAX as u64 {
        return Err(Error::Unsupported(
            "transition common timebase exceeds backend bound".into(),
        ));
    }
    let output_tick =
        u64::from(output_rate.denominator()) * clock / u64::from(output_rate.numerator());
    let mut input_args = Vec::<OsString>::new();
    let mut filters = Vec::new();
    for (index, path) in plan.sources.iter().enumerate() {
        input_args.extend([
            "-protocol_whitelist".into(),
            "file".into(),
            "-i".into(),
            path.as_os_str().to_owned(),
        ]);
        let start = if index == 0 {
            plan.source_in[0]
        } else {
            plan.source_in[1] - plan.before
        };
        let length = if index == 0 {
            plan.durations[0] + plan.after
        } else {
            plan.durations[1] + plan.before
        };
        let end = start
            .checked_add(length)
            .ok_or_else(|| Error::Invalid("transition source endpoint overflow".into()))?;
        let (sw, sh) = plan.source_sizes[index];
        if sw == 0 || sh == 0 {
            return Err(Error::Invalid("invalid transition source geometry".into()));
        }
        filters.push(format!("[{index}:v:0]trim=start_frame={start}:end_frame={end},settb=expr={}/{},setpts=N,scale={width}:{height}:force_original_aspect_ratio=decrease:flags=lanczos,pad={width}:{height}:(ow-iw)/2:(oh-ih)/2:color=black,setsar=1,fps={output_rate}:round=up:eof_action=pass,settb=expr=1/{clock},setpts=N*{output_tick},format=gbrp[lumeryn_transition_{index}]",plan.rate.denominator(),plan.rate.numerator()));
    }
    let name = match plan.kind {
        TransitionKind::CrossDissolve => "fade",
        TransitionKind::WipeLeft => "wipeleft",
        TransitionKind::WipeRight => "wiperight",
        TransitionKind::WipeUp => "wipeup",
        TransitionKind::WipeDown => "wipedown",
    };
    let duration = plan.rate.seconds(plan.before + plan.after);
    let offset = plan.rate.seconds(plan.durations[0] - plan.before);
    let total = crate::animation_backend::output_frames(
        plan.durations[0] + plan.durations[1],
        plan.rate,
        output_rate,
    )?;
    filters.push(format!("[lumeryn_transition_0][lumeryn_transition_1]xfade=transition={name}:duration={duration}:offset={offset},trim=end_frame={total},format=rgba,setsar=1[lumeryn_visual_output]"));
    Ok(VisualProgram {
        input_args,
        filters: filters.join(";"),
        output_label: "lumeryn_visual_output",
        input_count: 2,
        resources: None,
    })
}
