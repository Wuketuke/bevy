//! Displays a single [`Sprite`], created from an image.

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
        ..default()
    });
}
