use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use rand::Rng;

pub struct TexturesPlugin;

impl Plugin for TexturesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_textures);
    }
}

#[derive(Resource)]
pub struct TextureAssets {
    pub froth_frame1: Handle<Image>,
}

fn load_textures(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let froth_frame1 = images.add(make_froth_frame());
    commands.insert_resource(TextureAssets { froth_frame1 });
}

fn make_froth_frame() -> Image {
    let mut rng = rand::thread_rng();
    let mut pixels: Vec<u8> = Vec::new();

    for _ in 0..16 {
        let is_speckle = rng.r#gen::<f32>() < 0.1;
        if is_speckle {
            pixels.extend_from_slice(&[180, 210, 255, 255]); // blue speckle
        } else {
            pixels.extend_from_slice(&[255, 255, 255, 255]); // white base
        }
    }

    Image::new(
        Extent3d {
            width: 4,
            height: 4,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        pixels,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}
