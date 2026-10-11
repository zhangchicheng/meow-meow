//! Flow field towards the player, so enemies can walk around walls and props
//! when they can't see the player.

use std::collections::VecDeque;

use bevy::prelude::*;

use crate::{
    actor::{Dead, FEET},
    map::{BLOCK_MOVE, Level, Obstacle},
    player::Player,
};

pub fn plugin(app: &mut App) {
    app.init_resource::<FlowField>().add_systems(Update, update_flow_field);
}

/// Walking distance (in tiles) from every cell to the player's cell.
#[derive(Resource, Default)]
pub struct FlowField {
    width: i32,
    height: i32,
    distance: Vec<u16>,
    refresh: f32,
}

const UNREACHABLE: u16 = u16::MAX;
const DIRS: [IVec2; 8] = [
    IVec2::new(1, 0),
    IVec2::new(-1, 0),
    IVec2::new(0, 1),
    IVec2::new(0, -1),
    IVec2::new(1, 1),
    IVec2::new(1, -1),
    IVec2::new(-1, 1),
    IVec2::new(-1, -1),
];

impl FlowField {
    fn get(&self, c: IVec2) -> u16 {
        if c.x < 0 || c.y < 0 || c.x >= self.width || c.y >= self.height {
            UNREACHABLE
        } else {
            self.distance[(c.y * self.width + c.x) as usize]
        }
    }

    /// Direction to walk from `pos` to get closer to the player.
    pub fn direction(&self, level: &Level, pos: Vec2) -> Option<Vec2> {
        let here = level.cell_coords(pos);
        let mut best = (self.get(here), here);
        for d in DIRS {
            let c = here + d;
            // No diagonal moves that would clip a blocked corner.
            if d.x != 0 && d.y != 0
                && (self.get(here + IVec2::new(d.x, 0)) == UNREACHABLE || self.get(here + IVec2::new(0, d.y)) == UNREACHABLE)
            {
                continue;
            }
            let dist = self.get(c);
            if dist < best.0 {
                best = (dist, c);
            }
        }
        if best.1 == here {
            return None;
        }
        (level.cell_center(best.1) - pos).try_normalize()
    }
}

fn update_flow_field(
    time: Res<Time>,
    level: Option<Res<Level>>,
    mut field: ResMut<FlowField>,
    players: Query<&Transform, (With<Player>, Without<Dead>)>,
    obstacles: Query<&Obstacle>,
) {
    field.refresh -= time.delta_secs();
    let (Some(level), Ok(player)) = (level, players.single()) else { return };
    if field.refresh > 0.0 && !level.is_changed() {
        return;
    }
    field.refresh = 0.25;

    let (w, h) = (level.width, level.height);
    let mut walkable = vec![true; (w * h) as usize];
    for y in 0..h {
        for x in 0..w {
            walkable[(y * w + x) as usize] = level.cell(x, y) & BLOCK_MOVE == 0;
        }
    }
    for obstacle in &obstacles {
        let min = level.cell_coords(obstacle.center + Vec2::new(-obstacle.half.x - FEET.x, obstacle.half.y + FEET.y));
        let max = level.cell_coords(obstacle.center + Vec2::new(obstacle.half.x + FEET.x, -obstacle.half.y - FEET.y));
        for y in min.y.max(0)..=max.y.min(h - 1) {
            for x in min.x.max(0)..=max.x.min(w - 1) {
                // Only block cells mostly covered by the prop.
                if obstacle.contains(level.cell_center(IVec2::new(x, y)), FEET) {
                    walkable[(y * w + x) as usize] = false;
                }
            }
        }
    }

    field.width = w;
    field.height = h;
    field.distance = vec![UNREACHABLE; (w * h) as usize];
    let start = level.cell_coords(player.translation.truncate());
    if start.x < 0 || start.y < 0 || start.x >= w || start.y >= h {
        return;
    }
    let mut queue = VecDeque::from([start]);
    field.distance[(start.y * w + start.x) as usize] = 0;
    while let Some(c) = queue.pop_front() {
        let next = field.distance[(c.y * w + c.x) as usize] + 1;
        for d in &DIRS[..4] {
            let n = c + *d;
            if n.x < 0 || n.y < 0 || n.x >= w || n.y >= h {
                continue;
            }
            let i = (n.y * w + n.x) as usize;
            if walkable[i] && field.distance[i] == UNREACHABLE {
                field.distance[i] = next;
                queue.push_back(n);
            }
        }
    }
}
