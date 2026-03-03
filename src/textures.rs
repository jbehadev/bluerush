use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

pub struct TexturesPlugin;

impl Plugin for TexturesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_textures);
    }
}

#[derive(Resource)]
pub struct TextureAssets {
    pub froth_frame1: Handle<Image>,
    pub froth_frame2: Handle<Image>,
}

fn load_textures(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    // Frame 1: sin(x)*sin(y) interference pattern
    // Frame 2: cos(x)*cos(y) — phase-shifted 90° so bubble positions are complementary.
    // Alternating between them creates the illusion of churning foam.
    let froth_frame1 = images.add(make_froth_frame(0.0, 0.0));
    let froth_frame2 = images.add(make_froth_frame(
        std::f32::consts::FRAC_PI_2,
        std::f32::consts::FRAC_PI_2,
    ));
    commands.insert_resource(TextureAssets {
        froth_frame1,
        froth_frame2,
    });
}

/// Generates a 16×16 foam texture using a sine-wave interference pattern.
/// `x_phase` and `y_phase` shift the pattern so two frames have complementary
/// bubble positions.
fn make_froth_frame(x_phase: f32, y_phase: f32) -> Image {
    let size = 16u32;
    // A 5-pixel period gives ~3 bubble cycles across the tile without
    // perfectly aligning to the grid, which looks more organic.
    let freq = std::f32::consts::TAU / 5.0;
    let mut pixels: Vec<u8> = Vec::with_capacity((size * size * 4) as usize);

    for y in 0..size {
        for x in 0..size {
            let val = (x as f32 * freq + x_phase).sin() * (y as f32 * freq + y_phase).sin();

            let color: [u8; 4] = if val > 0.5 {
                [255, 255, 255, 255] // bright white foam blob
            } else if val > 0.0 {
                [210, 230, 255, 255] // pale blue-white fringe
            } else {
                [255, 255, 255, 255] // white between blobs
            };
            pixels.extend_from_slice(&color);
        }
    }

    Image::new(
        Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        pixels,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}
