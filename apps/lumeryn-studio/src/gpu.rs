use egui_wgpu::RenderState;
use lumeryn_preview::PreviewFrame;
use std::time::Instant;

/// Uses eframe's device/queue; decoded CPU frames are uploaded once, never read back.
pub struct PreviewTexture {
    state: RenderState,
    texture: Option<wgpu::Texture>,
    id: Option<egui::TextureId>,
    size: [u32; 2],
    last_key: Option<(u64, u64)>,
    pub uploads: u64,
    pub upload_cpu_ms: f64,
    pub adapter: String,
}

impl PreviewTexture {
    pub fn invalidate(&mut self) {
        self.last_key = None;
    }
    pub fn new(state: RenderState) -> Self {
        let info = state.adapter.get_info();
        Self {
            adapter: format!("{} · {:?}", info.name, info.backend),
            state,
            texture: None,
            id: None,
            size: [0, 0],
            last_key: None,
            uploads: 0,
            upload_cpu_ms: 0.0,
        }
    }

    pub fn upload(&mut self, frame: &PreviewFrame) -> Result<(), String> {
        let key = (frame.generation, frame.frame_index);
        if self.last_key == Some(key) {
            return Ok(());
        }
        let width = frame.width;
        let height = frame.height;
        let expected = u64::from(width) * u64::from(height) * 4;
        if width == 0 || height == 0 || expected != frame.rgba.len() as u64 {
            return Err("Decoder returned an invalid RGBA frame".into());
        }
        let started = Instant::now();
        if self.size != [width, height] {
            let texture = self.state.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Lumeryn decoded preview"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = texture.create_view(&Default::default());
            let mut renderer = self.state.renderer.write();
            if let Some(id) = self.id {
                renderer.update_egui_texture_from_wgpu_texture(
                    &self.state.device,
                    &view,
                    wgpu::FilterMode::Linear,
                    id,
                );
            } else {
                self.id = Some(renderer.register_native_texture(
                    &self.state.device,
                    &view,
                    wgpu::FilterMode::Linear,
                ));
            }
            self.texture = Some(texture);
            self.size = [width, height];
        }
        if let Some(texture) = &self.texture {
            self.state.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &frame.rgba,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(width * 4),
                    rows_per_image: Some(height),
                },
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );
        }
        self.last_key = Some(key);
        self.uploads += 1;
        self.upload_cpu_ms = started.elapsed().as_secs_f64() * 1000.0;
        Ok(())
    }

    pub fn image(&self) -> Option<(egui::TextureId, egui::Vec2)> {
        self.id
            .map(|id| (id, egui::vec2(self.size[0] as f32, self.size[1] as f32)))
    }
}

impl Drop for PreviewTexture {
    fn drop(&mut self) {
        if let Some(id) = self.id {
            self.state.renderer.write().free_texture(&id);
        }
    }
}
