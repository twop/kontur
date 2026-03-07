// ── Viewport ──────────────────────────────────────────────────────────────────
//
// Owns the camera position and animation state.
//
// Two animation modes are supported and chosen per-call via `MoveKind`:
//   - Spring  — damped spring physics; used for large jumps (FocusSelected).
//               Feels natural for unknown distances; settles without overshoot.
//   - Tween   — fixed-duration ExpoOut easing; used for incremental pan.
//               Predictable arrival time; snappy on repeated key presses since
//               each press restarts the tween from the current visual position.
//
// Coordinate contract:
//   `desired_center` is the *target* top-left canvas coordinate (i32).
//   `to_screen` converts a canvas-space SPoint to screen-space using whichever
//   animation is currently active, or the raw desired position when idle.

use crate::geometry::SPoint;
use damped_springs::{Spring, SpringConfig, SpringParams, SpringTimeStep};
use tween::Tweener;

// ── Animation config ──────────────────────────────────────────────────────────

/// Selects which animation strategy to use for a particular `set_center` call.
pub enum AnimationConfig {
    /// Viewport snaps instantly; no animation is performed.
    Disabled,
    /// Damped spring physics.  Good for large, distance-independent jumps.
    Spring {
        /// Higher values make the spring faster / stiffer.
        angular_freq: f32,
        /// 0 = undamped, 1 = critically damped, >1 = overdamped (no bounce).
        damping_ratio: f32,
    },
    /// Fixed-duration ExpoOut tween.  Good for incremental pan where the user
    /// expects a predictable, snappy arrival.
    Tween {
        /// Duration of the tween in seconds.
        duration: f32,
    },
}

// ── Active animation state ────────────────────────────────────────────────────

/// The animation that is currently running (if any).
enum Active {
    /// Spring simulation on each axis.
    Spring {
        spring_x: Spring<f32>,
        spring_y: Spring<f32>,
        params: SpringParams<f32>,
    },
    /// ExpoOut tweener on each axis, started from the animated position at the
    /// moment `set_center` was called so mid-flight interruptions are seamless.
    /// `pos_x` / `pos_y` cache the last value produced by `tick` so
    /// `current_position` can be called from a shared reference.
    Tween {
        tween_x: Tweener<f32, f32, tween::ExpoOut>,
        tween_y: Tweener<f32, f32, tween::ExpoOut>,
        pos_x: f32,
        pos_y: f32,
    },
    /// No animation running; `to_screen` uses `desired_center` directly.
    None,
}

// ── Viewport ──────────────────────────────────────────────────────────────────

pub struct Viewport {
    /// Target position set by user actions (integer canvas coords).
    pub desired_center: SPoint,

    /// Currently running animation (or `Active::None` when idle / disabled).
    active: Active,
}

impl Viewport {
    /// Create a new viewport centred on `center` with no active animation.
    pub fn new(center: SPoint) -> Self {
        Self {
            desired_center: center,
            active: Active::None,
        }
    }

    /// Set a new target center and start the appropriate animation.
    ///
    /// Always begins from the *current animated position* so interrupting a
    /// running animation mid-flight looks seamless.
    pub fn set_center(&mut self, target: SPoint, config: &AnimationConfig) {
        let (from_x, from_y) = self.current_position();
        self.desired_center = target;

        match config {
            AnimationConfig::Disabled => {
                self.active = Active::None;
            }

            AnimationConfig::Spring {
                angular_freq,
                damping_ratio,
            } => {
                let cfg = SpringConfig::new(*angular_freq, *damping_ratio);
                let params = SpringParams::from(cfg);

                let tx = target.x as f32;
                let ty = target.y as f32;

                // Carry over velocity when interrupting another spring so the
                // handoff is continuous; start from rest otherwise.
                let (vel_x, vel_y) = match &self.active {
                    Active::Spring {
                        spring_x, spring_y, ..
                    } => (spring_x.velocity, spring_y.velocity),
                    _ => (0.0, 0.0),
                };

                let mut spring_x = Spring::new().with_position(from_x).with_equilibrium(tx);
                spring_x.velocity = vel_x;

                let mut spring_y = Spring::new().with_position(from_y).with_equilibrium(ty);
                spring_y.velocity = vel_y;

                self.active = Active::Spring {
                    spring_x,
                    spring_y,
                    params,
                };
            }

            AnimationConfig::Tween { duration } => {
                let tx = target.x as f32;
                let ty = target.y as f32;

                self.active = Active::Tween {
                    tween_x: Tweener::expo_out(from_x, tx, *duration),
                    tween_y: Tweener::expo_out(from_y, ty, *duration),
                    pos_x: from_x,
                    pos_y: from_y,
                };
            }
        }
    }

    /// Advance the active animation by `dt` seconds.
    ///
    /// Call once per frame with the real wall-clock delta.  No-op when no
    /// animation is running.
    pub fn tick(&mut self, dt: f32) {
        match &mut self.active {
            Active::Spring {
                spring_x,
                spring_y,
                params,
            } => {
                let step = SpringTimeStep::new(*params, dt);
                spring_x.update(step);
                spring_y.update(step);

                // Settle to rest when close enough to avoid perpetual tiny updates.
                if (spring_x.position - spring_x.equilibrium).abs() < 0.01
                    && (spring_y.position - spring_y.equilibrium).abs() < 0.01
                    && spring_x.velocity.abs() < 0.01
                    && spring_y.velocity.abs() < 0.01
                {
                    self.active = Active::None;
                }
            }

            Active::Tween {
                tween_x,
                tween_y,
                pos_x,
                pos_y,
            } => {
                *pos_x = tween_x.move_by(dt);
                *pos_y = tween_y.move_by(dt);
                if tween_x.is_finished() && tween_y.is_finished() {
                    self.active = Active::None;
                }
            }

            Active::None => {}
        }
    }

    /// Convert a canvas-space point to a screen-space SPoint.
    ///
    /// Uses the animated position when an animation is active, or the raw
    /// `desired_center` otherwise.
    pub fn to_screen(&self, p: SPoint) -> SPoint {
        let (cx, cy) = self.current_position();
        SPoint::new(p.x - cx.round() as i32, p.y - cy.round() as i32)
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    /// Returns the current animated camera position as `(x, y)` floats.
    fn current_position(&self) -> (f32, f32) {
        match &self.active {
            Active::Spring {
                spring_x, spring_y, ..
            } => (spring_x.position, spring_y.position),
            Active::Tween { pos_x, pos_y, .. } => (*pos_x, *pos_y),
            Active::None => (self.desired_center.x as f32, self.desired_center.y as f32),
        }
    }
}
