//! Displays a single [`Sprite`], created from an image, but flipped on one axis.

use bevy::prelude::*;

const LOGO_PATH: &str = "branding/bevy_bird_dark.png";

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.spawn(Camera2dBundle::default());
    commands.spawn(SpriteBundle {
        texture: asset_server.load(LOGO_PATH),
        sprite: Sprite {
            // Flip the logo to the left
            flip_x: true,
            // And don't flip it upside-down ( the default )
            flip_y: false,
            ..default()
        },
        ..default()
    });
}
