use bevy::prelude::*;

use crate::{GameState, actor::{Dead, FEET, ysort_z}, map::{Level, Obstacle}, player::Player};

pub fn plugin(app: &mut App) {
    app.add_message::<PortalTravel>()
        .add_systems(Update, teleport_players.after(crate::actor::move_actors).run_if(in_state(GameState::Playing)));
}

#[derive(Component)]
pub struct Portal {
    pub center: Vec2,
    pub half: Vec2,
}

impl Portal {
    pub fn contains(&self, point: Vec2) -> bool {
        let distance = (point - self.center).abs();
        distance.x <= self.half.x && distance.y <= self.half.y
    }

    fn entered(&self, from: Vec2, to: Vec2) -> bool {
        if self.contains(from) { return false; }
        // Sweep the movement segment through the doorway so fast rolls cannot skip it.
        let delta = to - from;
        let min = self.center - self.half;
        let max = self.center + self.half;
        let (mut enter, mut exit) = (0.0_f32, 1.0_f32);
        for axis in 0..2 {
            if delta[axis].abs() < 0.00001 {
                if from[axis] < min[axis] || from[axis] > max[axis] { return false; }
            } else {
                let a = (min[axis] - from[axis]) / delta[axis];
                let b = (max[axis] - from[axis]) / delta[axis];
                enter = enter.max(a.min(b));
                exit = exit.min(a.max(b));
                if enter > exit { return false; }
            }
        }
        true
    }

    fn landing(&self, blocked: impl Fn(Vec2) -> bool) -> Option<Vec2> {
        let distance = self.half.y + FEET.y + 4.0;
        // Prefer the front of the destination, then its back, always checking collisions.
        for offset in [distance, distance + 8.0, distance + 16.0] {
            for direction in [-1.0, 1.0] {
                let point = self.center + Vec2::Y * offset * direction;
                if !blocked(point) { return Some(point); }
            }
        }
        (!blocked(self.center)).then_some(self.center)
    }
}

#[derive(Component, Default)]
pub struct PortalTraveler {
    previous: Option<Vec2>,
    cooldown: f32,
}

#[derive(Message)]
pub(crate) struct PortalTravel;

fn teleport_players(
    time: Res<Time>,
    level: Option<Res<Level>>,
    portals: Query<(Entity, &Portal)>,
    obstacles: Query<&Obstacle>,
    mut players: Query<(&mut Transform, &mut PortalTraveler), (With<Player>, Without<Dead>)>,
    mut travelled: MessageWriter<PortalTravel>,
) {
    let Some(level) = level else { return };
    let mut portals: Vec<_> = portals.iter().collect();
    // Pair the two current portals; additional portals form a stable spatial cycle.
    portals.sort_by(|(_, a), (_, b)| b.center.y.total_cmp(&a.center.y).then(a.center.x.total_cmp(&b.center.x)));
    for (mut transform, mut traveler) in &mut players {
        let position = transform.translation.truncate();
        let previous = traveler.previous.replace(position).unwrap_or(position);
        traveler.cooldown = (traveler.cooldown - time.delta_secs()).max(0.0);
        if portals.len() < 2 || traveler.cooldown > 0.0 || time.delta_secs() == 0.0 { continue; }
        let Some(source) = portals.iter().position(|(_, portal)| portal.entered(previous, position)) else { continue };
        let destination = portals[(source + 1) % portals.len()].1;
        let landing = destination.landing(|point| {
            level.box_blocked(point, FEET) || obstacles.iter().any(|obstacle| obstacle.contains(point, FEET))
        });
        if let Some(landing) = landing {
            transform.translation = landing.extend(ysort_z(landing.y));
            traveler.previous = Some(landing);
            traveler.cooldown = 0.5;
            travelled.write(PortalTravel);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portals_transfer_both_ways_without_bouncing_during_cooldown() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(std::time::Duration::from_millis(100)))
            .insert_resource(Level::open_for_test())
            .add_message::<PortalTravel>()
            .add_systems(Update, teleport_players);
        let first = Vec2::new(100.0, 1000.0);
        let second = Vec2::new(100.0, 200.0);
        for center in [first, second] {
            app.world_mut().spawn(Portal { center, half: Vec2::new(5.0, 4.5) });
        }
        let player = app.world_mut().spawn((Player::new(), Transform::from_translation((first - Vec2::Y * 10.0).extend(0.0)))).id();
        app.update();
        app.world_mut().get_mut::<Transform>(player).unwrap().translation = first.extend(0.0);
        app.update();
        let landed = app.world().get::<Transform>(player).unwrap().translation;
        assert_eq!(landed.truncate(), second - Vec2::Y * 11.5);
        assert_eq!(landed.z, ysort_z(landed.y));
        assert!(app.world().get::<PortalTraveler>(player).unwrap().cooldown > 0.0);
        // Entering the destination during cooldown and then standing there must not bounce.
        app.world_mut().get_mut::<Transform>(player).unwrap().translation = second.extend(0.0);
        for _ in 0..8 { app.update(); }
        assert_eq!(app.world().get::<Transform>(player).unwrap().translation.truncate(), second);
        // Leave, then enter again: travel back through the paired portal.
        app.world_mut().get_mut::<Transform>(player).unwrap().translation = (second - Vec2::Y * 10.0).extend(0.0);
        app.update();
        app.world_mut().get_mut::<Transform>(player).unwrap().translation = second.extend(0.0);
        app.update();
        assert_eq!(app.world().get::<Transform>(player).unwrap().translation.truncate(), first - Vec2::Y * 11.5);
    }

    #[test]
    fn doorway_detects_fast_crossings_without_retriggering_inside() {
        let portal = Portal { center: Vec2::ZERO, half: Vec2::new(5.0, 4.5) };
        assert!(portal.entered(Vec2::new(0.0, -20.0), Vec2::new(0.0, 20.0)));
        assert!(portal.entered(Vec2::new(0.0, -10.0), Vec2::ZERO));
        assert!(!portal.entered(Vec2::ZERO, Vec2::new(0.0, 1.0)));
        assert!(!portal.entered(Vec2::new(10.0, -20.0), Vec2::new(10.0, 20.0)));
    }

    #[test]
    fn destination_avoids_blocked_ground_and_can_reject_a_blocked_portal() {
        let portal = Portal { center: Vec2::new(457.0, 50.5), half: Vec2::new(5.0, 4.5) };
        let front = portal.landing(|_| false).unwrap();
        assert_eq!(front, Vec2::new(457.0, 39.0));
        assert!(!portal.contains(front));
        let back = portal.landing(|point| point.y < portal.center.y).unwrap();
        assert_eq!(back, Vec2::new(457.0, 62.0));
        assert!(portal.landing(|_| true).is_none());
    }
}
