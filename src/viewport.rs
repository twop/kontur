// ── Viewport ──────────────────────────────────────────────────────────────────
//
// Owns the camera position and optional spring-physics animation.
//
// Coordinate contract:
//   - `desired_center` is the *target* top-left canvas coordinate in i32.
//   - When animation is enabled, `spring_x / spring_y` carry the floating-point
//     animated position that approaches `desired_center` over time.
//   - `to_screen` converts a canvas-space SPoint to a screen-space SPoint using
//     the currently animated position (or the raw desired position when disabled).

use crate::geometry::SPoint;
use damped_springs::{Spring, SpringConfig, SpringParams, SpringTimeStep};

// ── Animation config ──────────────────────────────────────────────────────────

/// Controls how the viewport animates toward a new target center.
pub enum AnimationConfig {
    /// Viewport snaps instantly; no spring math is performed.
    Disabled,
    /// Viewport animates using damped-spring physics.
    Spring {
        /// Higher values make the spring faster / stiffer.
        angular_freq: f32,
        /// 0 = no damping (bouncy forever), 1 = critically damped, >1 = overdamped.
        damping_ratio: f32,
    },
}

// ── Viewport ──────────────────────────────────────────────────────────────────

pub struct Viewport {
    /// Target position set by user actions (integer canvas coords).
    pub desired_center: SPoint,

    /// Per-axis springs that animate toward `desired_center`.
    /// Both springs start at rest at the initial position.
    spring_x: Spring<f32>,
    spring_y: Spring<f32>,

    /// Pre-computed spring parameters (derived from `AnimationConfig::Spring`).
    /// `None` when animation is `Disabled`.
    params: Option<SpringParams<f32>>,

    /// The animation mode in use.
    pub animation: AnimationConfig,
}

impl Viewport {
    /// Create a new viewport centred on `center` with the given animation config.
    /// The spring is initialised at rest *at* the initial center (no initial motion).
    pub fn new(center: SPoint, animation: AnimationConfig) -> Self {
        let params = match &animation {
            AnimationConfig::Disabled => None,
            AnimationConfig::Spring {
                angular_freq,
                damping_ratio,
            } => {
                let config = SpringConfig::new(*angular_freq, *damping_ratio);
                Some(SpringParams::from(config))
            }
        };

        let cx = center.x as f32;
        let cy = center.y as f32;

        // Start the springs at rest, position == equilibrium == initial center.
        let spring_x = Spring::new().with_position(cx).with_equilibrium(cx);
        let spring_y = Spring::new().with_position(cy).with_equilibrium(cy);

        Self {
            desired_center: center,
            spring_x,
            spring_y,
            params,
            animation,
        }
    }

    /// Set a new target center.
    ///
    /// Updates `desired_center` immediately and, when animation is enabled,
    /// also updates the spring equilibriums so they start pulling toward the
    /// new target on the next `tick`.
    pub fn set_center(&mut self, target: SPoint) {
        self.desired_center = target;
        if self.params.is_some() {
            self.spring_x.equilibrium = target.x as f32;
            self.spring_y.equilibrium = target.y as f32;
        }
    }

    /// Advance the spring simulation by `dt` seconds.
    ///
    /// Call this once per frame with the real wall-clock delta.  When animation
    /// is `Disabled` this is a no-op.
    pub fn tick(&mut self, dt: f32) {
        if let Some(params) = self.params {
            let time_step = SpringTimeStep::new(params, dt);
            self.spring_x.update(time_step);
            self.spring_y.update(time_step);
        }
    }

    /// Convert a canvas-space point to a screen-space point.
    ///
    /// Uses the animated (spring) position when animation is enabled, or the
    /// raw `desired_center` when disabled.  The result is in signed integer
    /// screen coordinates and must be range-checked by the caller before use.
    pub fn to_screen(&self, p: SPoint) -> SPoint {
        let (cx, cy) = match self.params {
            Some(_) => (
                self.spring_x.position.round() as i32,
                self.spring_y.position.round() as i32,
            ),
            None => (self.desired_center.x, self.desired_center.y),
        };
        SPoint::new(p.x - cx, p.y - cy)
    }
}
