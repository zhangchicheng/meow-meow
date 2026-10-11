//! Refresh the three pickups at reachable random positions when a wave starts.
use std::collections::VecDeque;

use bevy::prelude::*;
use rand::{Rng, seq::SliceRandom};

use crate::{GameState, actor::{FEET, ysort_z}, map::{Level, Obstacle, Pickup, PickupKind}, portal::Portal};

fn positions(level: &Level, player: Vec2, obstacles: &[&Obstacle], portals: &[&Portal], rng: &mut impl Rng) -> Vec<Vec2> {
    let mut visited = vec![false; (level.width * level.height) as usize];
    let clear = |pos| !level.box_blocked(pos, FEET) && !obstacles.iter().any(|o| o.contains(pos, FEET));
    let connected = |from: Vec2, to: Vec2| {
        let steps = (from.distance(to) / 2.0).ceil().max(1.0) as u32;
        (0..=steps).all(|step| clear(from.lerp(to, step as f32 / steps as f32)))
    };
    let mut queue = VecDeque::new();
    // A player beside a prop can stand in a cell whose centre is blocked.
    let start = level.cell_coords(player);
    for y in -1..=1 {
        for x in -1..=1 {
            let cell = start + IVec2::new(x, y);
            if connected(player, level.cell_center(cell)) {
                queue.push_back(cell);
            }
        }
    }
    let mut candidates = Vec::new();
    while let Some(cell) = queue.pop_front() {
        if cell.x < 0 || cell.y < 0 || cell.x >= level.width || cell.y >= level.height {
            continue;
        }
        let index = (cell.y * level.width + cell.x) as usize;
        if std::mem::replace(&mut visited[index], true) {
            continue;
        }
        let pos = level.cell_center(cell);
        if !clear(pos) {
            continue;
        }
        if pos.distance(player) >= 48.0 && portals.iter().all(|portal| pos.distance(portal.center) >= 32.0) {
            candidates.push(pos);
        }
        for delta in [IVec2::X, IVec2::NEG_X, IVec2::Y, IVec2::NEG_Y] {
            let next = cell + delta;
            if connected(pos, level.cell_center(next)) {
                queue.push_back(next);
            }
        }
    }
    candidates.shuffle(rng);
    let mut chosen: Vec<Vec2> = Vec::new();
    for candidate in candidates {
        if chosen.iter().all(|pos| pos.distance(candidate) >= 48.0) {
            chosen.push(candidate);
            if chosen.len() == 3 {
                break;
            }
        }
    }
    chosen
}

pub(crate) fn refresh(
    commands: &mut Commands, server: &AssetServer, level: &Level, player: Vec2,
    pickups: &Query<Entity, With<Pickup>>, obstacles: &Query<&Obstacle>, portals: &Query<&Portal>,
    barrels: &Query<(Entity, &crate::map::BarrelHome)>,
) {
    for entity in pickups {
        commands.entity(entity).despawn();
    }
    // Restored barrel colliders are still queued in Commands at this point.
    let obstacles: Vec<_> = obstacles.iter().chain(barrels.iter().map(|(_, home)| &home.0)).collect();
    let positions = positions(level, player, &obstacles, &portals.iter().collect::<Vec<_>>(), &mut rand::rng());
    for ((name, kind), pos) in [
        ("health", PickupKind::Health), ("time", PickupKind::Time), ("speed", PickupKind::Speed),
    ].into_iter().zip(positions) {
        commands.spawn((
            Name::new(format!("Wave pickup: {name}")),
            Pickup { kind, center_offset: Vec2::ZERO },
            Sprite::from_image(server.load(format!("maps/props/{name}.png"))),
            Transform::from_translation(pos.extend(ysort_z(pos.y))),
            DespawnOnEnter(GameState::Restart),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn random_pickups_remain_reachable_and_clear_of_obstacles() {
        let level = Level::open_for_test();
        let barrier = Obstacle { center: Vec2::new(800.0, 800.0), half: Vec2::new(16.0, 800.0) };
        let player = Vec2::new(200.0, 800.0);
        for _ in 0..10 {
            let points = positions(&level, player, &[&barrier], &[], &mut rand::rng());
            assert_eq!(points.len(), 3);
            for point in &points {
                assert!(point.x < 800.0);
                assert!(!level.box_blocked(*point, FEET));
                assert!(!barrier.contains(*point, FEET));
                assert!(point.distance(player) >= 48.0);
            }
            assert!(points[0].distance(points[1]) >= 48.0);
        }
    }
}
