use ratatui::layout::{Rect, Size};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    Left,
    Right,
    Up,
    Down,
}

// ── SPoint ────────────────────────────────────────────────────────────────────

/// A signed 2-D point in canvas or screen space.
///
/// Uses `i32` to support a large canvas (±2 billion cells).
/// Dimensions (width/height) stay `u16` because terminal sizes are small.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SPoint {
    pub x: i32,
    pub y: i32,
}

impl SPoint {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    /// Translate by `(dx, dy)`.
    pub const fn translate(self, dx: i32, dy: i32) -> Self {
        Self {
            x: self.x + dx,
            y: self.y + dy,
        }
    }

    /// Component-wise subtraction — useful for converting canvas→screen coords.
    pub const fn sub(self, other: SPoint) -> Self {
        Self {
            x: self.x - other.x,
            y: self.y - other.y,
        }
    }

    /// Component-wise addition.
    pub const fn add(self, other: SPoint) -> Self {
        Self {
            x: self.x + other.x,
            y: self.y + other.y,
        }
    }

    /// Returns `true` when this point lies within `[0, w) × [0, h)`.
    pub fn in_bounds(self, w: i32, h: i32) -> bool {
        self.x >= 0 && self.y >= 0 && self.x < w && self.y < h
    }
}

impl From<(i32, i32)> for SPoint {
    fn from((x, y): (i32, i32)) -> Self {
        Self { x, y }
    }
}

impl std::ops::Add<(i32, i32)> for SPoint {
    type Output = SPoint;

    fn add(self, (dx, dy): (i32, i32)) -> Self::Output {
        Self {
            x: self.x + dx,
            y: self.y + dy,
        }
    }
}

impl std::ops::Add<Dir> for SPoint {
    type Output = SPoint;

    fn add(self, dir: Dir) -> Self::Output {
        match dir {
            Dir::Right => SPoint::new(self.x + 1, self.y),
            Dir::Left => SPoint::new(self.x - 1, self.y),
            Dir::Down => SPoint::new(self.x, self.y + 1),
            Dir::Up => SPoint::new(self.x, self.y - 1),
        }
    }
}

impl std::ops::Sub<(i32, i32)> for SPoint {
    type Output = SPoint;

    fn sub(self, (dx, dy): (i32, i32)) -> Self::Output {
        Self {
            x: self.x - dx,
            y: self.y - dy,
        }
    }
}

// ── SRect ─────────────────────────────────────────────────────────────────────

/// A signed rectangle: top-left origin (canvas / screen coords) + ratatui `Size`.
///
/// The origin is `i32` to handle an arbitrarily large canvas; width/height
/// remain `u16` because terminal dimensions are always small.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SRect {
    pub origin: SPoint,
    pub size: Size,
}

impl SRect {
    pub const fn new(x: i32, y: i32, width: u16, height: u16) -> Self {
        Self {
            origin: SPoint::new(x, y),
            size: Size { width, height },
        }
    }

    // ── Corner accessors ──────────────────────────────────────────────────────

    pub const fn top_left(&self) -> SPoint {
        self.origin
    }

    pub fn top_right(&self) -> SPoint {
        SPoint::new(self.right(), self.origin.y)
    }

    pub fn bottom_left(&self) -> SPoint {
        SPoint::new(self.origin.x, self.bottom())
    }

    pub fn bottom_right(&self) -> SPoint {
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

    pub fn mid_top(&self) -> SPoint {
        SPoint::new(self.origin.x + self.size.width as i32 / 2, self.top())
    }

    pub fn mid_bottom(&self) -> SPoint {
        SPoint::new(self.origin.x + self.size.width as i32 / 2, self.bottom())
    }

    pub fn mid_left(&self) -> SPoint {
        SPoint::new(self.left(), self.origin.y + self.size.height as i32 / 2)
    }

    pub fn mid_right(&self) -> SPoint {
        SPoint::new(self.right(), self.origin.y + self.size.height as i32 / 2)
    }

    // ── Centre ────────────────────────────────────────────────────────────────

    pub fn center(&self) -> SPoint {
        SPoint::new(
            self.origin.x + self.size.width as i32 / 2,
            self.origin.y + self.size.height as i32 / 2,
        )
    }

    // ── Bounds extension ─────────────────────────────────────────────────────

    /// Extend this rect to be the smallest rect that contains both `self` and
    /// the given point `p`.  Returns a new `SRect`; `self` is unchanged.
    pub fn extend_to(self, p: SPoint) -> Self {
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

    pub fn contains(&self, p: SPoint) -> bool {
        p.x >= self.left() && p.x <= self.right() && p.y >= self.top() && p.y <= self.bottom()
    }

    /// Clip `self` against a frame rectangle `[0, fw) × [0, fh)`.
    ///
    /// Returns `None` if the rect is entirely outside the frame.
    pub fn clip_to_frame(&self, fw: i32, fh: i32) -> Option<SRect> {
        let x1 = self.left();
        let y1 = self.top();
        let x2 = self.left() + self.size.width as i32;
        let y2 = self.top() + self.size.height as i32;

        if x1 >= fw || x2 <= 0 || y1 >= fh || y2 <= 0 {
            return None;
        }

        let cx1 = x1.max(0);
        let cy1 = y1.max(0);
        let cx2 = x2.min(fw);
        let cy2 = y2.min(fh);

        Some(SRect::new(cx1, cy1, (cx2 - cx1) as u16, (cy2 - cy1) as u16))
    }

    /// Translate origin by a viewport offset (canvas → screen).
    pub fn to_screen(&self, vp_origin: SPoint) -> SRect {
        SRect {
            origin: self.origin.sub(vp_origin),
            size: self.size,
        }
    }

    // ── Conversions ───────────────────────────────────────────────────────────

    /// Convert to a ratatui `Rect`. Panics in debug if origin is negative.
    pub fn to_rect(&self) -> Rect {
        debug_assert!(self.origin.x >= 0 && self.origin.y >= 0);
        Rect::new(
            self.origin.x as u16,
            self.origin.y as u16,
            self.size.width,
            self.size.height,
        )
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

    fn rect() -> SRect {
        // origin (4, 6), width 10, height 6  →  right=13, bottom=11
        SRect::new(4, 6, 10, 6)
    }

    // ── SPoint ────────────────────────────────────────────────────────────────

    #[test]
    fn spoint_offset() {
        let p = SPoint::new(2, 3);
        assert_eq!(p.translate(1, -1), SPoint::new(3, 2));
    }

    #[test]
    fn spoint_add_sub() {
        let a = SPoint::new(5, 7);
        let b = SPoint::new(2, 3);
        assert_eq!(a.add(b), SPoint::new(7, 10));
        assert_eq!(a.sub(b), SPoint::new(3, 4));
    }

    #[test]
    fn spoint_in_bounds() {
        let p = SPoint::new(3, 4);
        assert!(p.in_bounds(10, 10));
        assert!(!p.in_bounds(3, 10)); // x == w  →  out
        assert!(!SPoint::new(-1, 0).in_bounds(10, 10));
    }

    #[test]
    fn spoint_large_canvas() {
        // Verify i32 handles coordinates well beyond i16::MAX (32767)
        let p = SPoint::new(100_000, -50_000);
        assert_eq!(p.translate(-100_000, 50_000), SPoint::new(0, 0));
    }

    // ── SRect corners ─────────────────────────────────────────────────────────

    #[test]
    fn top_left() {
        assert_eq!(rect().top_left(), SPoint::new(4, 6));
    }

    #[test]
    fn top_right() {
        assert_eq!(rect().top_right(), SPoint::new(13, 6));
    }

    #[test]
    fn bottom_left() {
        assert_eq!(rect().bottom_left(), SPoint::new(4, 11));
    }

    #[test]
    fn bottom_right() {
        assert_eq!(rect().bottom_right(), SPoint::new(13, 11));
    }

    // ── SRect edges ───────────────────────────────────────────────────────────

    #[test]
    fn left_right_top_bottom() {
        let r = rect();
        assert_eq!(r.left(), 4);
        assert_eq!(r.right(), 13);
        assert_eq!(r.top(), 6);
        assert_eq!(r.bottom(), 11);
    }

    // ── Mid-edge points ───────────────────────────────────────────────────────

    #[test]
    fn mid_edges() {
        let r = rect(); // width=10 → midx=4+5=9; height=6 → midy=6+3=9
        assert_eq!(r.mid_top(), SPoint::new(9, 6));
        assert_eq!(r.mid_bottom(), SPoint::new(9, 11));
        assert_eq!(r.mid_left(), SPoint::new(4, 9));
        assert_eq!(r.mid_right(), SPoint::new(13, 9));
    }

    // ── Center ────────────────────────────────────────────────────────────────

    #[test]
    fn center() {
        // width=10 → x+5=9; height=6 → y+3=9
        assert_eq!(rect().center(), SPoint::new(9, 9));
    }

    // ── Contains ──────────────────────────────────────────────────────────────

    #[test]
    fn contains_inside() {
        assert!(rect().contains(SPoint::new(8, 8)));
    }

    #[test]
    fn contains_on_border() {
        let r = rect();
        assert!(r.contains(r.top_left()));
        assert!(r.contains(r.bottom_right()));
    }

    #[test]
    fn contains_outside() {
        assert!(!rect().contains(SPoint::new(0, 0)));
        assert!(!rect().contains(SPoint::new(14, 9)));
    }

    // ── Clip ──────────────────────────────────────────────────────────────────

    #[test]
    fn clip_fully_visible() {
        let r = SRect::new(2, 2, 4, 3);
        assert_eq!(r.clip_to_frame(20, 20), Some(r));
    }

    #[test]
    fn clip_partial_left() {
        // origin at x=-2, width=6 → visible columns 0..4
        let r = SRect::new(-2, 0, 6, 3);
        let clipped = r.clip_to_frame(20, 20).unwrap();
        assert_eq!(clipped.left(), 0);
        assert_eq!(clipped.size.width, 4);
    }

    #[test]
    fn clip_partial_right() {
        let r = SRect::new(18, 0, 6, 3);
        let clipped = r.clip_to_frame(20, 20).unwrap();
        assert_eq!(clipped.right(), 19);
        assert_eq!(clipped.size.width, 2);
    }

    #[test]
    fn clip_entirely_off_left() {
        let r = SRect::new(-10, 0, 4, 3);
        assert_eq!(r.clip_to_frame(20, 20), None);
    }

    #[test]
    fn clip_entirely_off_right() {
        let r = SRect::new(22, 0, 4, 3);
        assert_eq!(r.clip_to_frame(20, 20), None);
    }

    // ── to_screen / to_rect ───────────────────────────────────────────────────

    #[test]
    fn to_screen_translates_origin() {
        let r = SRect::new(10, 8, 4, 3);
        let vp = SPoint::new(3, 2);
        let s = r.to_screen(vp);
        assert_eq!(s.origin, SPoint::new(7, 6));
        assert_eq!(s.size, r.size);
    }

    #[test]
    fn to_rect_round_trips() {
        let r = SRect::new(2, 3, 8, 5);
        let rect = r.to_rect();
        assert_eq!(rect.x, 2);
        assert_eq!(rect.y, 3);
        assert_eq!(rect.width, 8);
        assert_eq!(rect.height, 5);
    }

    #[test]
    fn from_rect() {
        let rect = Rect::new(1, 2, 10, 4);
        let sr = SRect::from(rect);
        assert_eq!(sr.origin, SPoint::new(1, 2));
        assert_eq!(sr.size.width, 10);
        assert_eq!(sr.size.height, 4);
    }
}
