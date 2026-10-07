//! Optional CPU rendering of headless egui frames for visual layout checks.
//! No native application or desktop capture is needed.
use eframe::egui::{self, epaint::Primitive};
use std::collections::HashMap;
pub(super) fn text_center(shapes: &[egui::epaint::ClippedShape], text: &str) -> Option<egui::Pos2> {
    fn find(shape: &egui::Shape, text: &str) -> Option<egui::Pos2> {
        match shape {
            egui::Shape::Text(t) if t.galley.job.text == text => {
                Some(t.pos + t.galley.rect.center().to_vec2())
            }
            egui::Shape::Vec(shapes) => shapes.iter().find_map(|s| find(s, text)),
            _ => None,
        }
    }
    shapes.iter().find_map(|s| find(&s.shape, text))
}

pub(super) fn save(
    ctx: &egui::Context,
    mut outputs: Vec<egui::FullOutput>,
    path: &std::path::Path,
    size: egui::Vec2,
) {
    let mut textures: HashMap<egui::TextureId, egui::ColorImage> = HashMap::new();
    for output in &outputs {
        for (id, delta) in &output.textures_delta.set {
            let egui::ImageData::Color(data) = &delta.image;
            if let Some([x, y]) = delta.pos {
                let full = textures.get_mut(id).unwrap();
                for row in 0..data.size[1] {
                    for col in 0..data.size[0] {
                        full.pixels[(y + row) * full.size[0] + x + col] =
                            data.pixels[row * data.size[0] + col];
                    }
                }
            } else {
                textures.insert(*id, data.as_ref().clone());
            }
        }
    }
    let output = outputs.pop().unwrap();
    let mut canvas = image::RgbaImage::from_pixel(
        size.x as u32,
        size.y as u32,
        image::Rgba([245, 245, 245, 255]),
    );
    for clipped in ctx.tessellate(output.shapes, output.pixels_per_point) {
        let Primitive::Mesh(mesh) = clipped.primitive else {
            continue;
        };
        for ids in mesh.indices.chunks_exact(3) {
            let v = [
                mesh.vertices[ids[0] as usize],
                mesh.vertices[ids[1] as usize],
                mesh.vertices[ids[2] as usize],
            ];
            let cross = |a: egui::Vec2, b: egui::Vec2| a.x * b.y - a.y * b.x;
            let area = cross(v[1].pos - v[0].pos, v[2].pos - v[0].pos);
            if area.abs() < 1e-6 {
                continue;
            }
            let min_x = v
                .iter()
                .map(|p| p.pos.x)
                .fold(f32::INFINITY, f32::min)
                .max(clipped.clip_rect.min.x)
                .max(0.0)
                .floor() as u32;
            let max_x = v
                .iter()
                .map(|p| p.pos.x)
                .fold(f32::NEG_INFINITY, f32::max)
                .min(clipped.clip_rect.max.x)
                .min(size.x)
                .ceil() as u32;
            let min_y = v
                .iter()
                .map(|p| p.pos.y)
                .fold(f32::INFINITY, f32::min)
                .max(clipped.clip_rect.min.y)
                .max(0.0)
                .floor() as u32;
            let max_y = v
                .iter()
                .map(|p| p.pos.y)
                .fold(f32::NEG_INFINITY, f32::max)
                .min(clipped.clip_rect.max.y)
                .min(size.y)
                .ceil() as u32;
            for y in min_y..max_y {
                for x in min_x..max_x {
                    let p = egui::pos2(x as f32 + 0.5, y as f32 + 0.5);
                    let a = cross(v[1].pos - p, v[2].pos - p) / area;
                    let b = cross(v[2].pos - p, v[0].pos - p) / area;
                    let weights = [a, b, 1.0 - a - b];
                    if weights.iter().any(|w| *w < -1e-5) {
                        continue;
                    }
                    let mut color = [0.0; 4];
                    let mut uv = egui::Vec2::ZERO;
                    for (vert, w) in v.iter().zip(weights) {
                        for (c, value) in color.iter_mut().zip(vert.color.to_array()) {
                            *c += w * value as f32;
                        }
                        uv += vert.uv.to_vec2() * w;
                    }
                    if let Some(tex) = textures.get(&mesh.texture_id) {
                        let tx = (uv.x * tex.size[0] as f32).clamp(0.0, (tex.size[0] - 1) as f32)
                            as usize;
                        let ty = (uv.y * tex.size[1] as f32).clamp(0.0, (tex.size[1] - 1) as f32)
                            as usize;
                        for (c, value) in color
                            .iter_mut()
                            .zip(tex.pixels[ty * tex.size[0] + tx].to_array())
                        {
                            *c *= value as f32 / 255.0;
                        }
                    }
                    let dest = canvas.get_pixel_mut(x, y);
                    let alpha = color[3] / 255.0;
                    for (i, c) in color.iter().enumerate().take(3) {
                        dest.0[i] = (c + dest.0[i] as f32 * (1.0 - alpha)).clamp(0.0, 255.0) as u8;
                    }
                }
            }
        }
    }
    canvas.save(path).unwrap();
}
