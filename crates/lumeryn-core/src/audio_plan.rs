//! Bounded, window-independent audio scheduling foundation.
//!
//! Independent tracks use verified decoded sample counts; historical embedded audio
//! uses an explicit padding policy. Preview and export consume the same generated mix.
use crate::{
    Error, Project, Result,
    media::{content_hash, local_file},
    model::MAX_FRAMES,
    time::FrameRate,
};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

pub const MIX_SAMPLE_RATE: u32 = 48_000;
pub const MIX_CHANNELS: u16 = 2;
pub const MAX_AUDIO_INPUTS: usize = 64;
/// Source counting and subsequent range decoding must use this identical policy.
pub const CANONICAL_DECODE_FILTER: &str =
    "aresample=48000:async=0,aformat=sample_fmts=fltp:channel_layouts=stereo";

/// Immutable local source with an externally verified decoded sample-frame count.
///
/// The count refers to stereo 48 kHz after the canonical resampling policy, not bytes,
/// interleaved scalar samples, container duration or the source's native sample rate.
/// `None` describes historical embedded audio with explicit silence padding. The
/// public independent-plan constructor never accepts an unproven contribution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioSource {
    pub path: PathBuf,
    pub digest: String,
    pub canonical_samples: Option<u64>,
}

/// One independent sound contribution. Frames belong to the project; source samples
/// belong to the canonical decoded source. Gain uses millidecibels: zero is unity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioSegment {
    pub source: usize,
    pub at_frame: u64,
    pub source_in_sample: u64,
    pub duration_frames: u64,
    pub gain_millidecibels: i32,
    pub muted: bool,
}

#[derive(Debug, Clone)]
pub struct AudioPlan {
    rate: FrameRate,
    total_frames: u64,
    total_samples: u64,
    sources: Vec<AudioSource>,
    segments: Vec<AudioSegment>,
    /// Only embedded video contributions may pad beyond unproven/short audio bounds.
    embedded_padding: Vec<bool>,
}

/// Generated labels and numeric filter parameters only. Paths are separate argv values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioFilter {
    pub filters: String,
    pub output_label: &'static str,
}

/// Map an absolute project boundary to canonical sample frames, using checked arithmetic.
pub fn sample_boundary(rate: FrameRate, frame: u64) -> Result<u64> {
    if frame > MAX_FRAMES {
        return Err(Error::Invalid(
            "audio frame boundary exceeds project limit".into(),
        ));
    }
    let samples = u128::from(frame)
        .checked_mul(u128::from(rate.denominator()))
        .and_then(|value| value.checked_mul(u128::from(MIX_SAMPLE_RATE)))
        .ok_or_else(|| Error::Invalid("audio sample boundary overflow".into()))?
        / u128::from(rate.numerator());
    let samples = u64::try_from(samples)
        .map_err(|_| Error::Invalid("audio sample boundary overflow".into()))?;
    if samples > i64::MAX as u64 {
        return Err(Error::Invalid(
            "audio sample boundary exceeds backend limit".into(),
        ));
    }
    Ok(samples)
}

impl AudioPlan {
    pub fn new(
        rate: FrameRate,
        total_frames: u64,
        sources: Vec<AudioSource>,
        segments: Vec<AudioSegment>,
    ) -> Result<Self> {
        if sources
            .iter()
            .any(|source| source.canonical_samples.is_none())
        {
            return Err(Error::Invalid(
                "independent audio requires verified canonical source sample counts".into(),
            ));
        }
        let embedded_padding = vec![false; segments.len()];
        Self::build(rate, total_frames, sources, segments, embedded_padding)
    }

    fn build(
        rate: FrameRate,
        total_frames: u64,
        sources: Vec<AudioSource>,
        segments: Vec<AudioSegment>,
        embedded_padding: Vec<bool>,
    ) -> Result<Self> {
        if total_frames == 0
            || sources.len() > MAX_AUDIO_INPUTS
            || segments.len() > MAX_AUDIO_INPUTS
            || embedded_padding.len() != segments.len()
        {
            return Err(Error::Invalid(
                "audio plan requires positive visual duration and at most 64 sources/contributions"
                    .into(),
            ));
        }
        let total_samples = sample_boundary(rate, total_frames)?;
        if total_samples == 0 {
            return Err(Error::Invalid(
                "audio plan has no complete sample frames".into(),
            ));
        }
        for source in &sources {
            if !source.path.is_absolute()
                || source
                    .canonical_samples
                    .is_some_and(|count| count == 0 || count > i64::MAX as u64)
                || source.digest.len() != 64
                || !source
                    .digest
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            {
                return Err(Error::Invalid(
                    "audio source requires an absolute local path, BLAKE3 digest and positive verified canonical sample count"
                        .into(),
                ));
            }
        }
        for (segment, padding) in segments.iter().zip(&embedded_padding) {
            let source = sources.get(segment.source).ok_or_else(|| {
                Error::Invalid("audio contribution source index is invalid".into())
            })?;
            let end = segment
                .at_frame
                .checked_add(segment.duration_frames)
                .ok_or_else(|| Error::Invalid("audio timeline range overflow".into()))?;
            if segment.duration_frames == 0 || end > total_frames {
                return Err(Error::Invalid(
                    "audio contribution extends beyond visual duration; shorten/move audio or extend the visual timeline"
                        .into(),
                ));
            }
            if !(-60_000..=12_000).contains(&segment.gain_millidecibels) {
                return Err(Error::Invalid(
                    "audio gain must be between -60 and +12 dB".into(),
                ));
            }
            let length = sample_boundary(rate, end)? - sample_boundary(rate, segment.at_frame)?;
            let source_end = segment
                .source_in_sample
                .checked_add(length)
                .ok_or_else(|| Error::Invalid("audio source range overflow".into()))?;
            if length == 0
                || (!padding
                    && source
                        .canonical_samples
                        .is_none_or(|count| source_end > count))
            {
                return Err(Error::Invalid(
                    "audio contribution exceeds its verified canonical source sample range".into(),
                ));
            }
        }
        Ok(Self {
            rate,
            total_frames,
            total_samples,
            sources,
            segments,
            embedded_padding,
        })
    }

    /// Evaluate the authoritative graph outside the writer lock. An absent source
    /// count is accepted only for historical embedded audio, never independent clips.
    pub async fn from_project(project: &Project, database: &Path) -> Result<Self> {
        project.validate()?;
        let parent = database
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let mut sources = Vec::new();
        let mut segments = Vec::new();
        let mut embedded_padding = Vec::new();
        let mut clips = project.clips.iter().collect::<Vec<_>>();
        clips.sort_by_key(|clip| clip.at_frame);
        let contributions = project.audio_clips.len()
            + clips
                .iter()
                .filter(|clip| {
                    project
                        .media
                        .iter()
                        .find(|media| media.id == clip.media_id)
                        .is_some_and(|media| media.metadata.audio.is_some())
                })
                .count();
        if contributions > MAX_AUDIO_INPUTS {
            return Err(Error::Invalid(
                "audio plan supports at most 64 embedded/independent contributions".into(),
            ));
        }
        for clip in clips {
            let media = project
                .media
                .iter()
                .find(|media| media.id == clip.media_id)
                .ok_or_else(|| Error::NotFound("audio clip media".into()))?;
            if media.metadata.audio.is_some() {
                sources.push(project_source(media, parent)?);
                segments.push(AudioSegment {
                    source: sources.len() - 1,
                    at_frame: clip.at_frame,
                    source_in_sample: sample_boundary(project.frame_rate, clip.source_in_frame)?,
                    duration_frames: clip.duration_frames,
                    gain_millidecibels: 0,
                    muted: false,
                });
                embedded_padding.push(true);
            }
        }
        for clip in &project.audio_clips {
            let media = project
                .media
                .iter()
                .find(|media| media.id == clip.media_id)
                .ok_or_else(|| Error::NotFound("independent audio clip media".into()))?;
            sources.push(project_source(media, parent)?);
            segments.push(AudioSegment {
                source: sources.len() - 1,
                at_frame: clip.at_frame,
                source_in_sample: clip.source_in_sample,
                duration_frames: clip.duration_frames,
                gain_millidecibels: clip.gain_millidecibels,
                muted: clip.muted,
            });
            embedded_padding.push(false);
        }
        let plan = Self::build(
            project.frame_rate,
            project.visual_duration_frames(),
            sources,
            segments,
            embedded_padding,
        )?;
        plan.verify_sources().await?;
        Ok(plan)
    }

    pub fn total_samples(&self) -> u64 {
        self.total_samples
    }

    /// Immutable identities for a streaming adapter's pre/post decoding checks.
    pub fn output_sources(&self) -> Vec<(PathBuf, String)> {
        self.sources
            .iter()
            .map(|source| (source.path.clone(), source.digest.clone()))
            .collect()
    }

    /// Validate every declared source identity outside the project writer lock.
    /// Call before and after decoding. A changed-and-restored race remains possible.
    pub async fn verify_sources(&self) -> Result<()> {
        for source in &self.sources {
            let path = local_file(&source.path)?;
            if content_hash(&path).await? != source.digest {
                return Err(Error::Invalid(
                    "audio source changed; reimport media".into(),
                ));
            }
        }
        Ok(())
    }

    /// One decoder input per contribution, bounded to 64. This avoids ambiguous stream
    /// fan-out; reusing a decoded source is a later measured optimization.
    pub fn input_args(&self) -> Vec<OsString> {
        self.segments
            .iter()
            .flat_map(|segment| {
                [
                    OsString::from("-threads"),
                    OsString::from("2"),
                    OsString::from("-protocol_whitelist"),
                    OsString::from("file"),
                    OsString::from("-i"),
                    self.sources[segment.source].path.as_os_str().to_owned(),
                ]
            })
            .collect()
    }

    /// Exact canonical mix, optionally sliced at a project frame for preview seeking.
    ///
    /// All earlier samples are evaluated before slicing, preserving converter state and
    /// sequential/seek parity. This has a known decode cost; no fast-seek claim is made.
    /// The output is additive with stable contribution order and a deterministic hard
    /// clamp to [-1, 1]. No adaptive normalization or automatic gain pumping is applied.
    pub fn filter_program(&self, input_offset: usize, start_frame: u64) -> Result<AudioFilter> {
        if start_frame >= self.total_frames {
            return Err(Error::Invalid(
                "audio seek must precede visual timeline end".into(),
            ));
        }
        input_offset
            .checked_add(self.segments.len())
            .filter(|end| *end <= i32::MAX as usize)
            .ok_or_else(|| Error::Invalid("audio adapter input index overflow".into()))?;
        let mut filters = Vec::with_capacity(self.segments.len() + 2);
        let mut labels = String::new();
        for (index, segment) in self.segments.iter().enumerate() {
            let at = sample_boundary(self.rate, segment.at_frame)?;
            let end = sample_boundary(self.rate, segment.at_frame + segment.duration_frames)?;
            let source_end = segment.source_in_sample + (end - at);
            let gain = if segment.muted {
                "0".to_owned()
            } else {
                format!(
                    "{:.12}",
                    10_f64.powf(f64::from(segment.gain_millidecibels) / 20_000.0)
                )
            };
            let padding = if self.embedded_padding[index] {
                format!(",apad=whole_len={},atrim=end_sample={}", end - at, end - at)
            } else {
                String::new()
            };
            filters.push(format!(
                "[{}:a:0]{CANONICAL_DECODE_FILTER},atrim=start_sample={}:end_sample={source_end},asetpts=PTS-STARTPTS{padding},volume={gain}:precision=float,adelay={at}S:all=1[lumeryn_audio_{index}]",
                input_offset + index, segment.source_in_sample,
            ));
            labels.push_str(&format!("[lumeryn_audio_{index}]"));
        }
        if self.segments.is_empty() {
            filters.push(format!(
                "anullsrc=r={MIX_SAMPLE_RATE}:cl=stereo,atrim=end_sample={}[lumeryn_audio_sum]",
                self.total_samples,
            ));
        } else {
            filters.push(format!(
                "{labels}amix=inputs={}:duration=longest:dropout_transition=0:normalize=0[lumeryn_audio_sum]",
                self.segments.len(),
            ));
        }
        let start_sample = sample_boundary(self.rate, start_frame)?;
        filters.push(format!(
            "[lumeryn_audio_sum]aeval='clip(val(0),-1,1)|clip(val(1),-1,1)':c=stereo,apad=whole_len={},atrim=end_sample={},atrim=start_sample={start_sample},asetpts=PTS-STARTPTS,aformat=sample_fmts=fltp:channel_layouts=stereo[lumeryn_audio_output]",
            self.total_samples, self.total_samples,
        ));
        Ok(AudioFilter {
            filters: filters.join(";"),
            output_label: "lumeryn_audio_output",
        })
    }

    /// Standalone bounded-worker arguments for f32 stereo PCM. The caller supplies the
    /// existing timeout, output/queue bounds, cancellation and source checks.
    pub fn pcm_args(&self, start_frame: u64) -> Result<Vec<OsString>> {
        self.pcm_args_for_device(start_frame, MIX_SAMPLE_RATE, MIX_CHANNELS)
    }

    /// Device conversion follows the canonical stereo mix; it never changes graph time.
    pub fn pcm_args_for_device(
        &self,
        start_frame: u64,
        sample_rate: u32,
        channels: u16,
    ) -> Result<Vec<OsString>> {
        if sample_rate == 0 || !(1..=8).contains(&channels) {
            return Err(Error::Invalid(
                "audio output requires a positive rate and 1..8 device channels".into(),
            ));
        }
        let filter = self.filter_program(0, start_frame)?;
        let mut args = [
            "-hide_banner",
            "-v",
            "error",
            "-nostdin",
            "-filter_complex_threads",
            "2",
        ]
        .into_iter()
        .map(OsString::from)
        .collect::<Vec<_>>();
        args.extend(self.input_args());
        args.extend([
            "-filter_complex".into(),
            filter.filters.into(),
            "-map".into(),
            format!("[{}]", filter.output_label).into(),
            "-vn".into(),
            "-ar".into(),
            sample_rate.to_string().into(),
            "-ac".into(),
            channels.to_string().into(),
            "-f".into(),
            "f32le".into(),
            "pipe:1".into(),
        ]);
        Ok(args)
    }
}

fn project_source(media: &crate::model::Media, parent: &Path) -> Result<AudioSource> {
    let audio = media
        .metadata
        .audio
        .as_ref()
        .ok_or_else(|| Error::Unsupported("audio stream required".into()))?;
    if audio.start_time_micros != 0 || ![1, 2].contains(&audio.channels) {
        return Err(Error::Unsupported(
            "audio composition requires zero-origin mono/stereo sources".into(),
        ));
    }
    let path = local_file(&parent.join(&media.path))?;
    if std::fs::metadata(&path)?.len() != media.metadata.size_bytes {
        return Err(Error::Invalid(
            "audio source size changed; reimport media".into(),
        ));
    }
    let digest = media
        .metadata
        .blake3
        .as_ref()
        .ok_or_else(|| {
            Error::Unsupported("audio composition requires verified source digest".into())
        })?
        .clone();
    Ok(AudioSource {
        path,
        digest,
        canonical_samples: audio.canonical_samples_48k,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MediaTools;
    use std::{fs::File, io::Write, path::Path};

    fn source(path: PathBuf, canonical_samples: u64) -> AudioSource {
        AudioSource {
            path,
            digest: "a".repeat(64),
            canonical_samples: Some(canonical_samples),
        }
    }
    fn segment(source: usize, at_frame: u64, duration_frames: u64) -> AudioSegment {
        AudioSegment {
            source,
            at_frame,
            source_in_sample: 0,
            duration_frames,
            gain_millidecibels: 0,
            muted: false,
        }
    }

    #[test]
    fn absolute_rational_boundaries_partition_without_drift() -> Result<()> {
        for rate in [FrameRate::new(30_000, 1001)?, FrameRate::new(24_000, 1001)?] {
            let lengths = (0..10_000)
                .map(|frame| Ok(sample_boundary(rate, frame + 1)? - sample_boundary(rate, frame)?))
                .collect::<Result<Vec<_>>>()?;
            assert_eq!(lengths.iter().sum::<u64>(), sample_boundary(rate, 10_000)?);
        }
        assert_eq!(sample_boundary(FrameRate::new(30_000, 1001)?, 30)?, 48_048);
        assert!(sample_boundary(FrameRate::new(30, 1)?, MAX_FRAMES + 1).is_err());
        Ok(())
    }

    #[test]
    fn rejects_invalid_sources_ranges_gain_caps_and_seeks() -> Result<()> {
        let rate = FrameRate::new(30, 1)?;
        let path = std::env::current_dir()?.join("unused.wav");
        let make = |sources, segments| AudioPlan::new(rate, 30, sources, segments);
        let audio = source(path, 48_000);
        let valid = segment(0, 0, 30);
        let plan = make(vec![audio.clone()], vec![valid])?;
        assert_eq!(plan.total_samples(), 48_000);
        assert!(plan.filter_program(0, 30).is_err());
        assert!(plan.filter_program(usize::MAX, 0).is_err());
        assert!(
            make(
                vec![audio.clone()],
                vec![AudioSegment { source: 1, ..valid }]
            )
            .is_err()
        );
        assert!(
            make(
                vec![audio.clone()],
                vec![AudioSegment {
                    duration_frames: 0,
                    ..valid
                }]
            )
            .is_err()
        );
        assert!(
            make(
                vec![audio.clone()],
                vec![AudioSegment {
                    at_frame: 1,
                    ..valid
                }]
            )
            .is_err()
        );
        assert!(
            make(
                vec![audio.clone()],
                vec![AudioSegment {
                    at_frame: u64::MAX,
                    ..valid
                }]
            )
            .is_err()
        );
        assert!(
            make(
                vec![audio.clone()],
                vec![AudioSegment {
                    source_in_sample: 1,
                    ..valid
                }]
            )
            .is_err()
        );
        assert!(
            make(
                vec![audio.clone()],
                vec![AudioSegment {
                    source_in_sample: u64::MAX,
                    ..valid
                }]
            )
            .is_err()
        );
        for gain_millidecibels in [-60_001, 12_001] {
            assert!(
                make(
                    vec![audio.clone()],
                    vec![AudioSegment {
                        gain_millidecibels,
                        ..valid
                    }]
                )
                .is_err()
            );
        }
        assert!(make(vec![audio.clone(); 65], vec![valid]).is_err());
        assert!(make(vec![audio.clone()], vec![valid; 65]).is_err());
        assert!(
            make(
                vec![AudioSource {
                    digest: "?".repeat(64),
                    ..audio.clone()
                }],
                vec![valid]
            )
            .is_err()
        );
        assert!(
            make(
                vec![AudioSource {
                    path: "relative.wav".into(),
                    ..audio.clone()
                }],
                vec![valid]
            )
            .is_err()
        );
        assert!(
            make(
                vec![AudioSource {
                    canonical_samples: Some(0),
                    ..audio
                }],
                vec![valid]
            )
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn filter_paths_stay_in_argv_and_parameters_are_generated() -> Result<()> {
        let path = std::env::current_dir()?.join("quote'%;[injection].wav");
        let plan = AudioPlan::new(
            FrameRate::new(30, 1)?,
            30,
            vec![source(path.clone(), 48_000)],
            vec![segment(0, 0, 30)],
        )?;
        let filter = plan.filter_program(7, 0)?;
        assert!(filter.filters.starts_with("[7:a:0]"));
        assert!(filter.filters.contains("dropout_transition=0:normalize=0"));
        assert!(!filter.filters.contains("injection"));
        assert!(plan.input_args().iter().any(|arg| arg == path.as_os_str()));
        Ok(())
    }

    /// Generate tiny lossless IEEE float WAV fixtures without adding an audio library.
    fn write_wav(path: &Path, rate: u32, samples: &[[f32; 2]]) -> Result<()> {
        let bytes = u32::try_from(samples.len())
            .map_err(|_| Error::Invalid("test WAV too long".into()))?
            * 8;
        let mut file = File::create(path)?;
        file.write_all(b"RIFF")?;
        file.write_all(&(36 + bytes).to_le_bytes())?;
        file.write_all(b"WAVEfmt ")?;
        file.write_all(&16_u32.to_le_bytes())?;
        file.write_all(&3_u16.to_le_bytes())?;
        file.write_all(&2_u16.to_le_bytes())?;
        file.write_all(&rate.to_le_bytes())?;
        file.write_all(&(rate * 8).to_le_bytes())?;
        file.write_all(&8_u16.to_le_bytes())?;
        file.write_all(&32_u16.to_le_bytes())?;
        file.write_all(b"data")?;
        file.write_all(&bytes.to_le_bytes())?;
        for value in samples.iter().flatten() {
            file.write_all(&value.to_le_bytes())?;
        }
        Ok(())
    }

    async fn verified_source(path: PathBuf, canonical_samples: u64) -> Result<AudioSource> {
        let tools = MediaTools::default();
        let digest = content_hash(&path).await?;
        let args = vec![
            "-hide_banner".into(),
            "-v".into(),
            "error".into(),
            "-nostdin".into(),
            "-protocol_whitelist".into(),
            "file".into(),
            "-i".into(),
            path.as_os_str().to_owned(),
            "-map".into(),
            "0:a:0".into(),
            "-vn".into(),
            "-af".into(),
            CANONICAL_DECODE_FILTER.into(),
            "-f".into(),
            "f32le".into(),
            "pipe:1".into(),
        ];
        // Test fixtures are one second, so existing bounded-output execution suffices.
        // Production source counting must stream bytes rather than materialize long PCM.
        let bytes = tools.run(&tools.ffmpeg, &args).await?;
        assert!(bytes.len().is_multiple_of(8));
        assert_eq!(bytes.len() as u64 / 8, canonical_samples);
        assert_eq!(content_hash(&path).await?, digest);
        Ok(AudioSource {
            digest,
            path,
            canonical_samples: Some(canonical_samples),
        })
    }

    async fn decode(tools: &MediaTools, plan: &AudioPlan, start: u64) -> Result<Vec<[f32; 2]>> {
        plan.verify_sources().await?;
        let bytes = tools.run(&tools.ffmpeg, &plan.pcm_args(start)?).await?;
        plan.verify_sources().await?;
        assert!(bytes.len().is_multiple_of(8));
        Ok(bytes
            .chunks_exact(8)
            .map(|bytes| {
                [
                    f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
                    f32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
                ]
            })
            .collect())
    }

    #[tokio::test]
    #[ignore = "requires configured FFmpeg; actual canonical PCM mix acceptance"]
    async fn decoded_mix_offset_gain_mute_clamp_seek_and_immutable_sources() -> Result<()> {
        let folder = tempfile::tempdir()?;
        let a = folder.path().join("constant%a.wav");
        let b = folder.path().join("ramp'b.wav");
        let constants = vec![[0.75, -0.75]; 48_000];
        let ramp = (0..48_000)
            .map(|index| [index as f32 / 100_000.0, -(index as f32 / 100_000.0)])
            .collect::<Vec<_>>();
        write_wav(&a, MIX_SAMPLE_RATE, &constants)?;
        write_wav(&b, MIX_SAMPLE_RATE, &ramp)?;
        let sources = vec![
            verified_source(a.clone(), 48_000).await?,
            verified_source(b.clone(), 48_000).await?,
        ];
        let rate = FrameRate::new(30, 1)?;
        let plan = AudioPlan::new(
            rate,
            60,
            sources.clone(),
            vec![
                segment(0, 0, 30),
                AudioSegment {
                    source: 1,
                    at_frame: 15,
                    source_in_sample: 8_000,
                    duration_frames: 15,
                    gain_millidecibels: -6_000,
                    muted: false,
                },
                AudioSegment {
                    source: 0,
                    at_frame: 30,
                    source_in_sample: 0,
                    duration_frames: 15,
                    gain_millidecibels: 12_000,
                    muted: true,
                },
            ],
        )?;
        let tools = MediaTools::default();
        let pcm = decode(&tools, &plan, 0).await?;
        assert_eq!(pcm.len(), 96_000);
        assert_eq!(pcm[0], constants[0]);
        assert_eq!(pcm[23_999], constants[0]);
        let expected = 0.75 + 0.08 * 10_f32.powf(-6.0 / 20.0);
        assert!((pcm[24_000][0] - expected).abs() < 0.000_002);
        assert!((pcm[24_000][1] + expected).abs() < 0.000_002);
        assert!(pcm[48_000..].iter().all(|samples| *samples == [0.0, 0.0]));
        assert_eq!(decode(&tools, &plan, 15).await?, pcm[24_000..]);
        let overlap = AudioPlan::new(
            rate,
            30,
            sources.clone(),
            vec![segment(0, 0, 30), segment(0, 0, 15)],
        )?;
        let overlap_pcm = decode(&tools, &overlap, 0).await?;
        assert!(
            overlap_pcm[..24_000]
                .iter()
                .all(|samples| *samples == [1.0, -1.0])
        );
        assert!(
            overlap_pcm[24_000..]
                .iter()
                .all(|samples| *samples == [0.75, -0.75])
        );
        let silence = AudioPlan::new(rate, 30, Vec::new(), Vec::new())?;
        assert!(
            decode(&tools, &silence, 0)
                .await?
                .iter()
                .all(|samples| *samples == [0.0, 0.0])
        );
        assert_eq!(content_hash(&a).await?, sources[0].digest);
        assert_eq!(content_hash(&b).await?, sources[1].digest);
        write_wav(&a, MIX_SAMPLE_RATE, &vec![[0.25, 0.25]; 48_000])?;
        assert!(plan.verify_sources().await.is_err());
        Ok(())
    }

    #[tokio::test]
    #[ignore = "requires configured FFmpeg; rational scheduling and actual resampling"]
    async fn decoded_fractional_schedule_and_resampling_seek_match() -> Result<()> {
        let folder = tempfile::tempdir()?;
        let path = folder.path().join("44100.wav");
        let samples = (0..44_100)
            .map(|index| {
                let tone = (index as f32 * 440.0 * std::f32::consts::TAU / 44_100.0).sin() * 0.25;
                [tone, -tone]
            })
            .collect::<Vec<_>>();
        write_wav(&path, 44_100, &samples)?;
        let rate = FrameRate::new(30_000, 1001)?;
        let plan = AudioPlan::new(
            rate,
            29,
            vec![verified_source(path, 48_000).await?],
            vec![AudioSegment {
                source: 0,
                at_frame: 1,
                source_in_sample: 1_000,
                duration_frames: 20,
                gain_millidecibels: 0,
                muted: false,
            }],
        )?;
        let tools = MediaTools::default();
        let pcm = decode(&tools, &plan, 0).await?;
        assert_eq!(pcm.len() as u64, sample_boundary(rate, 29)?);
        let start = sample_boundary(rate, 1)? as usize;
        let end = sample_boundary(rate, 21)? as usize;
        assert!(pcm[..start].iter().all(|samples| *samples == [0.0, 0.0]));
        assert!(pcm[end..].iter().all(|samples| *samples == [0.0, 0.0]));
        assert!(pcm[start..end].iter().any(|samples| samples[0].abs() > 0.2));
        assert!(
            pcm.iter()
                .all(|samples| samples.iter().all(|sample| sample.is_finite()))
        );
        assert_eq!(
            decode(&tools, &plan, 10).await?,
            pcm[sample_boundary(rate, 10)? as usize..]
        );
        Ok(())
    }
}
