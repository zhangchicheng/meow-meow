//! Loads the Tiled arena and turns its layers / objects into gameplay data.
//!
//! Map conventions (see `tools/gen_map.py`):
//! * tile layers `Walls` and `Outside` block movement and bullets, `Water` only blocks movement;
//! * the `Props` object layer holds tile objects from `props.tsx`, whose tiles carry
//!   `kind`, `collider_w` and `collider_h` custom properties;
//! * the `Spawns` object layer holds point objects named `player` / `enemy`.

use bevy::prelude::*;
use bevy_ecs_tiled::prelude::*;
use bevy_ecs_tiled::prelude::tiled::{self, PropertyValue};

use crate::{GameState, actor::{Health, ysort_z}, assets::GameAssets, player};

pub const TILE: f32 = 16.0;
pub const BLOCK_MOVE: u8 = 1;
pub const BLOCK_SHOT: u8 = 2;
const ANCHOR: TilemapAnchor = TilemapAnchor::BottomLeft;

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(GameState::Playing), spawn_map)
        // The title screen shows the same arena as its background.
        .add_systems(OnEnter(GameState::Menu), spawn_map)
        .add_systems(OnEnter(GameState::Restart), |mut commands: Commands| {
            commands.remove_resource::<Level>();
        });
}

/// Collision grid and spawn points of the loaded map.
#[derive(Resource)]
pub struct Level {
    pub width: i32,
    pub height: i32,
    cells: Vec<u8>,
    /// World position of the map's top-left corner (Tiled's origin).
    origin: Vec2,
    pub player_spawn: Vec2,
    pub enemy_spawns: Vec<Vec2>,
}

impl Level {
    #[cfg(test)]
    pub(crate) fn open_for_test() -> Self {
        Self {
            width: 100, height: 100, cells: vec![0; 10000],
            origin: Vec2::new(0.0, 1600.0), player_spawn: Vec2::ZERO, enemy_spawns: Vec::new(),
        }
    }

    pub fn cell(&self, x: i32, y: i32) -> u8 {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            BLOCK_MOVE | BLOCK_SHOT
        } else {
            self.cells[(y * self.width + x) as usize]
        }
    }

    pub fn cell_coords(&self, p: Vec2) -> IVec2 {
        IVec2::new(
            ((p.x - self.origin.x) / TILE).floor() as i32,
            ((self.origin.y - p.y) / TILE).floor() as i32,
        )
    }

    pub fn cell_center(&self, c: IVec2) -> Vec2 {
        Vec2::new(
            self.origin.x + (c.x as f32 + 0.5) * TILE,
            self.origin.y - (c.y as f32 + 0.5) * TILE,
        )
    }

    pub fn flags_at(&self, p: Vec2) -> u8 {
        let c = self.cell_coords(p);
        self.cell(c.x, c.y)
    }

    /// Whether a square of half-size `r` centred on `p` overlaps a movement-blocking tile.
    pub fn box_blocked(&self, p: Vec2, r: Vec2) -> bool {
        let min = self.cell_coords(p + Vec2::new(-r.x, r.y));
        let max = self.cell_coords(p + Vec2::new(r.x, -r.y));
        (min.y..=max.y).any(|y| (min.x..=max.x).any(|x| self.cell(x, y) & BLOCK_MOVE != 0))
    }

    /// Whether a bullet could fly from `a` to `b` without hitting a wall or a prop.
    pub fn line_of_sight(&self, obstacles: &[&Obstacle], a: Vec2, b: Vec2) -> bool {
        let steps = (a.distance(b) / 4.0).ceil() as i32;
        (0..=steps).all(|i| {
            let p = a.lerp(b, i as f32 / steps.max(1) as f32);
            self.flags_at(p) & BLOCK_SHOT == 0 && !obstacles.iter().any(|o| o.contains(p, BULLET_MARGIN))
        })
    }

    pub fn bounds(&self) -> Rect {
        Rect::new(
            self.origin.x,
            self.origin.y - self.height as f32 * TILE,
            self.origin.x + self.width as f32 * TILE,
            self.origin.y,
        )
    }
}

/// Static axis-aligned collider (world space) of a prop.
#[derive(Component, Clone)]
pub struct Obstacle {
    pub center: Vec2,
    pub half: Vec2,
}

/// Bullets stop a bit above a prop's ground collider, where the rest of the sprite is.
pub const BULLET_MARGIN: Vec2 = Vec2::new(1.0, 6.0);

impl Obstacle {
    pub fn contains(&self, p: Vec2, margin: Vec2) -> bool {
        let d = (p - self.center).abs();
        d.x < self.half.x + margin.x && d.y < self.half.y + margin.y
    }
}

fn portal_pillars(pos: Vec2, width: f32, pillar_width: f32, pillar_height: f32, inset: f32) -> [Obstacle; 2] {
    let half = Vec2::new(pillar_width, pillar_height) / 2.0;
    [
        Obstacle { center: pos + Vec2::new(inset + half.x, half.y), half },
        Obstacle { center: pos + Vec2::new(width - inset - half.x, half.y), half },
    ]
}

#[derive(Component)]
pub struct Explosive;

/// Keep the authored collider after detonation so each wave can restore the same barrel.
#[derive(Component)]
pub(crate) struct BarrelHome(pub Obstacle);

pub(crate) fn reset_barrels(commands: &mut Commands, barrels: &Query<(Entity, &BarrelHome)>) {
    for (entity, home) in barrels {
        commands.entity(entity).insert((home.0.clone(), Health::new(1), Visibility::Inherited));
    }
}

#[derive(Component)]
pub struct Pickup {
    pub kind: PickupKind,
    pub center_offset: Vec2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickupKind {
    Health,
    Time,
    Speed,
}

fn spawn_map(mut commands: Commands, server: Res<AssetServer>) {
    commands
        .spawn((
            Name::new("Arena"),
            TiledMap(server.load("maps/arena.tmx")),
            ANCHOR,
            // Keep layers close together so the y-sorted actors (z ~ 10) are above all of them.
            TiledMapLayerZOffset(1.0),
            DespawnOnEnter(GameState::Restart),
        ))
        .observe(on_map_created);
}

fn on_map_created(
    created: On<TiledEvent<MapCreated>>,
    map_assets: Res<Assets<TiledMapAsset>>,
    storages: Query<&TiledMapStorage>,
    mut transforms: Query<&mut Transform>,
    parents: Query<&ChildOf>,
    game_assets: Res<GameAssets>,
    character: Res<player::PlayerCharacter>,
    state: Res<State<GameState>>,
    mut commands: Commands,
) {
    let event = created.event();
    let (Some(asset), Ok(storage)) = (event.get_map_asset(&map_assets), storages.get(event.origin)) else {
        return;
    };
    let map = &asset.map;
    let to_world = |x: f32, y: f32| asset.world_space_from_tiled_position(&ANCHOR, Vec2::new(x, y));

    // Collision grid from the tile layers.
    let (width, height) = (map.width as i32, map.height as i32);
    let mut cells = vec![0u8; (width * height) as usize];
    for layer in map.layers() {
        let flags = match layer.name.as_str() {
            "Walls" | "Outside" => BLOCK_MOVE | BLOCK_SHOT,
            "Water" => BLOCK_MOVE,
            _ => continue,
        };
        let Some(tiles) = layer.as_tile_layer() else { continue };
        for y in 0..height {
            for x in 0..width {
                if tiles.get_tile(x, y).is_some() {
                    cells[(y * width + x) as usize] |= flags;
                }
            }
        }
    }

    let mut player_spawn = to_world(map.width as f32 * TILE / 2.0, map.height as f32 * TILE / 2.0);
    let mut enemy_spawns = Vec::new();

    for (_, &entity) in storage.objects() {
        let Some(object) = storage.get_object(map, entity) else { continue };
        let pos = to_world(object.x, object.y);
        match object.name.as_str() {
            "player" => player_spawn = pos,
            "enemy" => enemy_spawns.push(pos),
            _ => {}
        }
        let (Some(tile), tiled::ObjectShape::Rect { width: w, .. }) =
            (object.get_tile().and_then(|t| t.get_tile()), &object.shape)
        else {
            continue;
        };

        // Y-sort the prop with the actors: its bottom edge is its depth.
        let parent_z = parents
            .get(entity)
            .ok()
            .and_then(|c| transforms.get(c.parent()).ok())
            .map_or(0.0, |t| t.translation.z);
        if let Ok(mut transform) = transforms.get_mut(entity) {
            transform.translation.z = ysort_z(pos.y) - parent_z;
        }

        let int = |name: &str| match tile.properties.get(name) {
            Some(PropertyValue::IntValue(v)) => *v as f32,
            _ => 0.0,
        };
        let kind = match tile.properties.get("kind") {
            Some(PropertyValue::StringValue(s)) => s.as_str(),
            _ => "",
        };
        let (cw, ch) = (int("collider_w"), int("collider_h"));
        let mut e = commands.entity(entity);
        if kind == "portal" && cw > 0.0 && ch > 0.0 {
            // Separate pillar bases: no collider spans the portal's opening.
            let pillars = portal_pillars(pos, *w, cw, ch, int("collider_inset"));
            e.with_children(|children| {
                for (name, pillar) in ["Portal left pillar", "Portal right pillar"].into_iter().zip(pillars) {
                    children.spawn((Name::new(name), pillar));
                }
            });
        } else if cw > 0.0 && ch > 0.0 {
            e.insert(Obstacle {
                center: pos + Vec2::new(w / 2.0, ch / 2.0),
                half: Vec2::new(cw, ch) / 2.0,
            });
        }
        match kind {
            "portal" => {
                let opening_half = (w / 2.0 - int("collider_inset") - cw - crate::actor::FEET.x).max(1.0);
                e.insert(crate::portal::Portal {
                    center: pos + Vec2::new(w / 2.0, ch / 2.0),
                    half: Vec2::new(opening_half, (ch / 2.0).max(4.5)),
                });
            }
            "explosive" => {
                e.insert((Explosive, Health::new(1), BarrelHome(Obstacle {
                    center: pos + Vec2::new(w / 2.0, ch / 2.0),
                    half: Vec2::new(cw, ch) / 2.0,
                })));
            }
            "health" | "time" | "speed" => {
                // Pickups are spawned randomly each wave, including on older authored maps.
                e.despawn();
            }
            _ => {}
        }
    }

    let level = Level {
        width,
        height,
        cells,
        origin: to_world(0.0, 0.0),
        player_spawn,
        enemy_spawns,
    };
    if *state.get() == GameState::Playing {
        player::spawn_player(&mut commands, &game_assets, level.player_spawn, *character);
    }
    info!(
        "map loaded: {}x{} tiles, {} enemy spawn points",
        width,
        height,
        level.enemy_spawns.len()
    );
    commands.insert_resource(level);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actor::FEET;

    #[test]
    fn portal_pillars_block_the_bases_and_leave_the_middle_open() {
        let origin = Vec2::new(432.0, 46.0);
        let pillars = portal_pillars(origin, 50.0, 13.0, 9.0, 2.0);
        assert_eq!(pillars[0].center, origin + Vec2::new(8.5, 4.5));
        assert_eq!(pillars[1].center, origin + Vec2::new(41.5, 4.5));
        for pillar in &pillars {
            assert!(pillar.contains(pillar.center, FEET));
            assert!(pillar.contains(pillar.center, BULLET_MARGIN));
        }
        for y in [-5.0, 0.0, 4.5, 9.0, 15.0] {
            let middle = origin + Vec2::new(25.0, y);
            assert!(pillars.iter().all(|p| !p.contains(middle, FEET)));
            assert!(pillars.iter().all(|p| !p.contains(middle, BULLET_MARGIN)));
        }
    }
}
