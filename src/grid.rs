use bevy::{color::palettes::css::BLUE, prelude::*};

pub struct GridPlugin;

impl Plugin for GridPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup)
            .add_systems(Update, move_system);
    }
}

#[derive(Clone)]
enum Cell {
    Air,
    Water,
    Object(f32),
}

#[derive(Component)]
struct MyBox;

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
    commands.spawn((
        Sprite::from_color(
            BLUE,
            Vec2 {
                x: (50.0),
                y: (20.0),
            },
        ),
        Transform::from_xyz(100.0, 50.0, 0.0),
        MyBox,
    ));
}

fn move_system(time: Res<Time>, mut query: Query<&mut Transform, With<MyBox>>) {
    for mut transform in &mut query {
        transform.translation.x += 10.0 * time.delta_secs();
        transform.translation.y += 10.0 * time.delta_secs();
    }
}
