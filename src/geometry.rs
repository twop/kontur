use std::marker::PhantomData;

use ratatui::layout::{Rect, Size};

/// Padding (in cells) added on each side between a node's label and its border
/// when the node rect is computed automatically from the label text.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Padding {
    pub left: u8,
    pub top: u8,
    pub right: u8,
    pub bottom: u8,
}

impl Padding {
    pub fn to_ratatui(&self) -> ratatui::widgets::Padding {
        ratatui::widgets::Padding {
            left: self.left as u16,
            right: self.right as u16,
            top: self.top as u16,
            bottom: self.bottom as u16,
        }
    }
}

impl Default for Padding {
    fn default() -> Self {
        Self {
            left: 1,
            top: 0,
            right: 1,
            bottom: 0,
        }
    }
}

// ── Coordinate-space markers ───────────────────────────────────────────────────

/// Marks coordinates that live in infinite canvas space.
///
/// This is the default space for `SPoint` and `SRect`.  All graph elements
/// (nodes, edges, connection points) are stored in canvas space.
pub struct Canvas;

// ── Dir ───────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    Left,
    Right,
    Up,
    Down,
}

// ── SPoint ────────────────────────────────────────────────────────────────────

/// A signed 2-D point parameterised by coordinate space `S`.
///
/// The default space is [`Canvas`].  Use `SPoint<Screen>` (from
/// `screen_space`) for terminal screen coordinates produced by the viewport
/// projection.
///
/// Uses `i32` to support a large canvas (±2 billion cells).
pub struct SPoint<S = Canvas> {
    pub x: i32,
    pub y: i32,
    _space: PhantomData<S>,
}

pub type CanvasPoint = SPoint;

// Manual trait impls so the bounds don't leak to callers via `S: Clone` etc.
impl<S> Clone for SPoint<S> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<S> Copy for SPoint<S> {}
impl<S> PartialEq for SPoint<S> {
    fn eq(&self, other: &Self) -> bool {
        self.x == other.x && self.y == other.y
    }
}
impl<S> Eq for SPoint<S> {}
impl<S> std::fmt::Debug for SPoint<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SPoint")
            .field("x", &self.x)
            .field("y", &self.y)
            .finish()
    }
}

impl<S> SPoint<S> {
    pub const fn new(x: i32, y: i32) -> Self {
        Self {
            x,
            y,
            _space: PhantomData,
        }
    }

    /// Translate by `(dx, dy)`.
    pub const fn translate(self, dx: i32, dy: i32) -> Self {
        Self::new(self.x + dx, self.y + dy)
    }

    /// Component-wise subtraction.
    pub const fn sub(self, other: SPoint<S>) -> Self {
        Self::new(self.x - other.x, self.y - other.y)
    }

    /// Component-wise addition.
    pub const fn add(self, other: SPoint<S>) -> Self {
        Self::new(self.x + other.x, self.y + other.y)
    }
}

impl<S> From<(i32, i32)> for SPoint<S> {
    fn from((x, y): (i32, i32)) -> Self {
        Self::new(x, y)
    }
}

impl<S> std::ops::Add<(i32, i32)> for SPoint<S> {
    type Output = SPoint<S>;

    fn add(self, (dx, dy): (i32, i32)) -> Self::Output {
        Self::new(self.x + dx, self.y + dy)
    }
}

impl<S> std::ops::Add<Dir> for SPoint<S> {
    type Output = SPoint<S>;

    fn add(self, dir: Dir) -> Self::Output {
        match dir {
            Dir::Right => SPoint::new(self.x + 1, self.y),
            Dir::Left => SPoint::new(self.x - 1, self.y),
            Dir::Down => SPoint::new(self.x, self.y + 1),
            Dir::Up => SPoint::new(self.x, self.y - 1),
        }
    }
}

impl<S> std::ops::Sub<(i32, i32)> for SPoint<S> {
    type Output = SPoint<S>;

    fn sub(self, (dx, dy): (i32, i32)) -> Self::Output {
        Self::new(self.x - dx, self.y - dy)
    }
}

// ── SRect ─────────────────────────────────────────────────────────────────────

/// A signed rectangle parameterised by coordinate space `S`.
///
/// The default space is [`Canvas`].  The origin is `i32` to handle an
/// arbitrarily large canvas; width/height remain `u16` because terminal
/// dimensions are always small.
pub struct SRect<S = Canvas> {
    pub origin: SPoint<S>,
    pub size: Size,
    _space: PhantomData<S>,
}

pub type CanvasRect = SRect<Canvas>;

// Manual trait impls — same reason as SPoint.
impl<S> Clone for SRect<S> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<S> Copy for SRect<S> {}
impl<S> PartialEq for SRect<S> {
    fn eq(&self, other: &Self) -> bool {
        self.origin == other.origin && self.size == other.size
    }
}
impl<S> Eq for SRect<S> {}
impl<S> std::fmt::Debug for SRect<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SRect")
            .field("origin", &self.origin)
            .field("size", &self.size)
            .finish()
    }
}

impl<S> SRect<S> {
    pub const fn new(x: i32, y: i32, width: u16, height: u16) -> Self {
        Self {
            origin: SPoint::new(x, y),
            size: Size { width, height },
            _space: PhantomData,
        }
    }

    /// Create the smallest rect that contains both `a` and `b`.
    ///
    /// The two points are treated as opposite corners; the constructor handles
    /// any ordering, so `from_two_points(p, q) == from_two_points(q, p)`.
    pub fn from_two_points(a: SPoint<S>, b: SPoint<S>) -> Self {
        let left = a.x.min(b.x);
        let top = a.y.min(b.y);
        let right = a.x.max(b.x);
        let bottom = a.y.max(b.y);
        Self::new(
            left,
            top,
            (right - left + 1) as u16,
            (bottom - top + 1) as u16,
        )
    }

    /// Create a rect centered on `center` with the given `size`.
    ///
    /// The origin is shifted so that `center` falls on the rect's centre cell
    /// (integer division, biased toward top-left for even dimensions).
    pub fn from_center(center: SPoint<S>, size: Size) -> Self {
        Self::new(
            center.x - size.width as i32 / 2,
            center.y - size.height as i32 / 2,
            size.width,
            size.height,
        )
    }

    /// Create a rect where top left is `origin` with specified `size`
    pub fn from_origin(origin: SPoint<S>, size: Size) -> Self {
        Self::new(origin.x, origin.y, size.width, size.height)
    }

    // ── Corner accessors ──────────────────────────────────────────────────────

    pub const fn top_left(&self) -> SPoint<S> {
        self.origin
    }

    pub fn top_right(&self) -> SPoint<S> {
        SPoint::new(self.right(), self.origin.y)
    }

    pub fn bottom_left(&self) -> SPoint<S> {
        SPoint::new(self.origin.x, self.bottom())
    }

    pub fn bottom_right(&self) -> SPoint<S> {
        SPoint::new(self.right(), self.bottom())
    }

    // ── Edge coordinates (inclusive) ──────────────────────────────────────────

    pub const fn left(&self) -> i32 {
        self.origin.x
    }

    /// Inclusive right edge (last occupied column).
    pub fn right(&self) -> i32 {
        self.origin.x + self.size.width as i32 - 1
    }

    pub const fn top(&self) -> i32 {
        self.origin.y
    }

    /// Inclusive bottom edge (last occupied row).
    pub fn bottom(&self) -> i32 {
        self.origin.y + self.size.height as i32 - 1
    }

    // ── Mid-edge points (used for edge connection points) ─────────────────────

    pub fn mid_top(&self) -> SPoint<S> {
        SPoint::new(self.origin.x + self.size.width as i32 / 2, self.top())
    }

    pub fn mid_bottom(&self) -> SPoint<S> {
        SPoint::new(self.origin.x + self.size.width as i32 / 2, self.bottom())
    }

    pub fn mid_left(&self) -> SPoint<S> {
        SPoint::new(self.left(), self.origin.y + self.size.height as i32 / 2)
    }

    pub fn mid_right(&self) -> SPoint<S> {
        SPoint::new(self.right(), self.origin.y + self.size.height as i32 / 2)
    }

    // ── Centre ────────────────────────────────────────────────────────────────

    pub fn center(&self) -> SPoint<S> {
        SPoint::new(
            self.origin.x + self.size.width as i32 / 2,
            self.origin.y + self.size.height as i32 / 2,
        )
    }

    // ── Bounds extension ─────────────────────────────────────────────────────

    /// Extend this rect to be the smallest rect that contains both `self` and
    /// the given point `p`.  Returns a new `SRect`; `self` is unchanged.
    pub fn extend_to(self, p: SPoint<S>) -> Self {
        let new_left = self.left().min(p.x);
        let new_top = self.top().min(p.y);
        let new_right = self.right().max(p.x);
        let new_bottom = self.bottom().max(p.y);
        SRect::new(
            new_left,
            new_top,
            (new_right - new_left + 1) as u16,
            (new_bottom - new_top + 1) as u16,
        )
    }

    // ── Containment & clipping ────────────────────────────────────────────────

    pub fn contains(&self, p: SPoint<S>) -> bool {
        p.x >= self.left() && p.x <= self.right() && p.y >= self.top() && p.y <= self.bottom()
    }

    /// Clip `self` against another rect in the same coordinate space
    ///
    /// Returns `None` if the rect is entirely outside the frame.
    pub fn clip_by(&self, clip_rect: SRect<S>) -> Option<SRect<S>> {
        if self.left() >= clip_rect.right()
            || self.top() >= clip_rect.bottom()
            || self.right() <= clip_rect.left()
            || self.bottom() <= clip_rect.top()
        {
            return None;
        }

        let left = self.left().max(clip_rect.left());
        let right = self.right().min(clip_rect.right());

        let top = self.top().max(clip_rect.top());
        let bottom = self.bottom().min(clip_rect.bottom());

        Some(SRect::from_two_points(
            SPoint::new(left, top),
            SPoint::new(right, bottom),
        ))
    }
}

impl From<Rect> for SRect {
    fn from(r: Rect) -> Self {
        Self::new(r.x as i32, r.y as i32, r.width, r.height)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── SPoint ────────────────────────────────────────────────────────────────

    #[test]
    fn spoint_offset() {
        let p = SPoint::<Canvas>::new(2, 3);
        assert_eq!(p.translate(1, -1), SPoint::new(3, 2));
    }

    #[test]
    fn spoint_add_sub() {
        let a = SPoint::<Canvas>::new(5, 7);
        let b = SPoint::new(2, 3);
        assert_eq!(a.add(b), SPoint::new(7, 10));
        assert_eq!(a.sub(b), SPoint::new(3, 4));
    }

    #[test]
    fn spoint_large_canvas() {
        // Verify i32 handles coordinates well beyond i16::MAX (32767)
        let p = SPoint::<Canvas>::new(100_000, -50_000);
        assert_eq!(p.translate(-100_000, 50_000), SPoint::new(0, 0));
    }

    // ── SRect corners ─────────────────────────────────────────────────────────

    #[test]
    fn top_left() {
        assert_eq!(CanvasRect::new(4, 6, 10, 6).top_left(), SPoint::new(4, 6));
    }

    #[test]
    fn top_right() {
        assert_eq!(CanvasRect::new(4, 6, 10, 6).top_right(), SPoint::new(13, 6));
    }

    #[test]
    fn bottom_left() {
        assert_eq!(
            CanvasRect::new(4, 6, 10, 6).bottom_left(),
            SPoint::new(4, 11)
        );
    }

    #[test]
    fn bottom_right() {
        assert_eq!(
            CanvasRect::new(4, 6, 10, 6).bottom_right(),
            SPoint::new(13, 11)
        );
    }

    // ── SRect edges ───────────────────────────────────────────────────────────

    #[test]
    fn left_right_top_bottom() {
        let r = CanvasRect::new(4, 6, 10, 6);
        assert_eq!(r.left(), 4);
        assert_eq!(r.right(), 13);
        assert_eq!(r.top(), 6);
        assert_eq!(r.bottom(), 11);
    }

    // ── Mid-edge points ───────────────────────────────────────────────────────

    #[test]
    fn mid_edges() {
        let r = CanvasRect::new(4, 6, 10, 6); // width=10 → midx=4+5=9; height=6 → midy=6+3=9
        assert_eq!(r.mid_top(), SPoint::new(9, 6));
        assert_eq!(r.mid_bottom(), SPoint::new(9, 11));
        assert_eq!(r.mid_left(), SPoint::new(4, 9));
        assert_eq!(r.mid_right(), SPoint::new(13, 9));
    }

    // ── Center ────────────────────────────────────────────────────────────────

    #[test]
    fn center() {
        // width=10 → x+5=9; height=6 → y+3=9
        assert_eq!(CanvasRect::new(4, 6, 10, 6).center(), SPoint::new(9, 9));
    }

    #[test]
    fn from_center() {
        // Even dimensions: center=(10,8), size=(6,4) → origin=(7,6)
        let r = CanvasRect::from_center(
            SPoint::new(10, 8),
            Size {
                width: 6,
                height: 4,
            },
        );
        assert_eq!(r.origin, SPoint::new(7, 6));
        assert_eq!(
            r.size,
            Size {
                width: 6,
                height: 4
            }
        );
        assert_eq!(r.center(), SPoint::new(10, 8));

        // Odd dimensions: center=(5,5), size=(5,3) → origin=(3,4)
        let r2 = CanvasRect::from_center(
            SPoint::new(5, 5),
            Size {
                width: 5,
                height: 3,
            },
        );
        assert_eq!(r2.origin, SPoint::new(3, 4));
        assert_eq!(r2.center(), SPoint::new(5, 5));
    }

    // ── Contains ──────────────────────────────────────────────────────────────

    #[test]
    fn contains_inside() {
        assert!(CanvasRect::new(4, 6, 10, 6).contains(SPoint::new(8, 8)));
    }

    #[test]
    fn contains_on_border() {
        let r = CanvasRect::new(4, 6, 10, 6);
        assert!(r.contains(r.top_left()));
        assert!(r.contains(r.bottom_right()));
    }

    #[test]
    fn contains_outside() {
        assert!(!CanvasRect::new(4, 6, 10, 6).contains(SPoint::new(0, 0)));
        assert!(!CanvasRect::new(4, 6, 10, 6).contains(SPoint::new(14, 9)));
    }

    // ── Clip ──────────────────────────────────────────────────────────────────

    #[test]
    fn clip_fully_visible() {
        let r = CanvasRect::new(2, 2, 4, 3);
        assert_eq!(r.clip_by(SRect::new(0, 0, 20, 20)), Some(r));
    }

    #[test]
    fn clip_partial_left() {
        // origin at x=-2, width=6 → visible columns 0..3 (width 4)
        let r = CanvasRect::new(-2, 0, 6, 3);
        let clipped = r.clip_by(SRect::new(0, 0, 20, 20)).unwrap();
        assert_eq!(clipped.left(), 0);
        assert_eq!(clipped.size.width, 4);
    }

    #[test]
    fn clip_partial_right() {
        // origin at x=18, width=6 → visible columns 18..19 (width 2)
        let r = CanvasRect::new(18, 0, 6, 3);
        let clipped = r.clip_by(SRect::new(0, 0, 20, 20)).unwrap();
        assert_eq!(clipped.right(), 19);
        assert_eq!(clipped.size.width, 2);
    }

    #[test]
    fn clip_partial_top() {
        // origin at y=-3, height=5 → visible rows 0..1 (height 2)
        let r = CanvasRect::new(0, -3, 4, 5);
        let clipped = r.clip_by(SRect::new(0, 0, 20, 20)).unwrap();
        assert_eq!(clipped.top(), 0);
        assert_eq!(clipped.size.height, 2);
    }

    #[test]
    fn clip_partial_bottom() {
        // origin at y=18, height=5 → visible rows 18..19 (height 2)
        let r = CanvasRect::new(0, 18, 4, 5);
        let clipped = r.clip_by(SRect::new(0, 0, 20, 20)).unwrap();
        assert_eq!(clipped.bottom(), 19);
        assert_eq!(clipped.size.height, 2);
    }

    #[test]
    fn clip_entirely_off_left() {
        let r = CanvasRect::new(-10, 0, 4, 3);
        assert_eq!(r.clip_by(SRect::new(0, 0, 20, 20)), None);
    }

    #[test]
    fn clip_entirely_off_right() {
        let r = CanvasRect::new(22, 0, 4, 3);
        assert_eq!(r.clip_by(SRect::new(0, 0, 20, 20)), None);
    }

    #[test]
    fn clip_entirely_off_top() {
        let r = CanvasRect::new(0, -5, 4, 3);
        assert_eq!(r.clip_by(SRect::new(0, 0, 20, 20)), None);
    }

    #[test]
    fn clip_entirely_off_bottom() {
        let r = CanvasRect::new(0, 22, 4, 3);
        assert_eq!(r.clip_by(SRect::new(0, 0, 20, 20)), None);
    }

    #[test]
    fn clip_against_non_origin_rect() {
        // clip rect does not start at (0,0)
        let clip = CanvasRect::new(5, 5, 10, 10); // right=14, bottom=14
        let r = CanvasRect::new(3, 8, 6, 4); // left=3..8, top=8..11
        let clipped = r.clip_by(clip).unwrap();
        assert_eq!(clipped.left(), 5);
        assert_eq!(clipped.right(), 8);
        assert_eq!(clipped.top(), 8);
        assert_eq!(clipped.bottom(), 11);
    }

    // ── from_rect ─────────────────────────────────────────────────────────────

    #[test]
    fn from_rect() {
        let rect = Rect::new(1, 2, 10, 4);
        let sr = CanvasRect::from(rect);
        assert_eq!(sr.origin, SPoint::new(1, 2));
        assert_eq!(sr.size.width, 10);
        assert_eq!(sr.size.height, 4);
    }
}
