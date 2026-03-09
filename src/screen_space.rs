// ── Screen coordinate space ───────────────────────────────────────────────────
//
// `Screen` is a zero-size marker type that tags `SPoint` and `SRect` values as
// living in terminal screen space (column/row coordinates after the viewport
// projection has been applied).
//
// The inner `PhantomData<()>` field is intentionally private: `Screen` can be
// *named* anywhere in the crate, but it can only be *constructed* inside this
// module.  The only way to obtain an `SPoint<Screen>` or `SRect<Screen>` is
// through the associated conversion functions on `Screen` itself, which require
// a `&Viewport`.  This ensures that all screen-space coordinates provably come
// from a viewport projection.

use std::marker::PhantomData;

use ratatui::layout::{Position, Rect, Size};

use crate::geometry::{Canvas, SPoint, SRect};
use crate::viewport::Viewport;

/// Marker type for terminal screen-space coordinates.
///
/// Cannot be constructed outside of this module; use [`Screen::point`] and
/// [`Screen::rect`] to project canvas-space values through a [`Viewport`].
pub struct Screen(PhantomData<()>);

pub type ViewportPoint = SPoint<Screen>;
pub type ViewportRect = SRect<Screen>;

impl Screen {
    /// Project a canvas-space point through `vp` into screen space.
    ///
    /// Uses the viewport's current animated position, falling back to
    /// `desired_center` when no animation is active.
    pub fn point(vp: &Viewport, p: SPoint<Canvas>) -> ViewportPoint {
        let SPoint { x, y, .. } = vp.animated_center();
        SPoint::new(p.x - x, p.y - y)
    }

    /// Project a canvas-space rect through `vp` into screen space.
    ///
    /// Only the origin is translated; the size is unchanged.
    pub fn rect(vp: &Viewport, r: SRect<Canvas>) -> ViewportRect {
        let origin = Self::point(vp, r.origin);
        SRect::new(origin.x, origin.y, r.size.width, r.size.height)
    }

    /// Convert a [`ViewportPoint`] to a `(u16, u16)` terminal coordinate, given
    /// the canvas render size.
    ///
    /// The viewport is centered on the canvas, so the screen origin (0, 0) maps
    /// to the top-left of the canvas.  We shift by half the canvas size to
    /// convert from viewport-centered coordinates to canvas-top-left coordinates.
    /// Coordinates that fall outside `[0, canvas_size)` are saturated to 0.
    pub fn to_ratatui_point(p: ViewportPoint, canvas_size: Size) -> Position {
        let x = p.x + canvas_size.width as i32 / 2;
        let y = p.y + canvas_size.height as i32 / 2;
        Position::new(x.max(0) as u16, y.max(0) as u16)
    }

    /// Convert a [`ViewportRect`] to a ratatui [`Rect`], given the canvas render
    /// size.
    ///
    /// Applies the same half-canvas shift as [`Self::to_ratatui_point`], then
    /// clamps the result so it stays within the canvas bounds.
    pub fn to_ratatui_rect(r: ViewportRect, canvas_size: Size) -> Rect {
        let Position { x, y } = Self::to_ratatui_point(r.origin, canvas_size);
        let right = (x as u32 + r.size.width as u32).min(canvas_size.width as u32) as u16;
        let bottom = (y as u32 + r.size.height as u32).min(canvas_size.height as u32) as u16;
        let width = right.saturating_sub(x);
        let height = bottom.saturating_sub(y);
        Rect {
            x,
            y,
            width,
            height,
        }
    }
}
