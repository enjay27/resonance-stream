//! Window geometry decisions, kept free of Tauri so they can be tested on any OS.

use resonance_types::WindowRect;

/// The INNER size to ask a window for so that its OUTER size becomes `outer`:
/// `set_size` sets the inner (client) size, while the rects this app keeps are
/// outer ones (`outer_position` / `outer_size`). Setting an outer size as the
/// inner one makes the window one frame bigger every time (K18's runbook run:
/// +22 x +13 px per restore). The frame is `outer - inner` of the live window.
pub fn inner_size_for(outer: WindowRect, frame_width: u32, frame_height: u32) -> (u32, u32) {
    (
        outer.width.saturating_sub(frame_width).max(1),
        outer.height.saturating_sub(frame_height).max(1),
    )
}

/// The rect a window should take to be at least `min_width` x `min_height`
/// (physical pixels) without leaving `work_area`, the monitor minus the taskbar.
/// It grows around its current centre and is pushed back inside the work area;
/// a side already larger than the minimum keeps its size. `None` when the
/// window is already big enough.
pub fn grow_to_fit(
    current: WindowRect,
    min_width: u32,
    min_height: u32,
    work_area: WindowRect,
) -> Option<WindowRect> {
    let (x, width) = grow_axis(
        current.x,
        current.width,
        min_width,
        work_area.x,
        work_area.width,
    );
    let (y, height) = grow_axis(
        current.y,
        current.height,
        min_height,
        work_area.y,
        work_area.height,
    );
    let grown = WindowRect {
        x,
        y,
        width,
        height,
    };
    (grown != current).then_some(grown)
}

/// One axis of [`grow_to_fit`]: the new start and length. An axis that does not
/// grow keeps both; one that grows is centred on the old centre, then clamped.
fn grow_axis(pos: i32, len: u32, min: u32, area_pos: i32, area_len: u32) -> (i32, u32) {
    let target = min.min(area_len);
    if len >= target {
        return (pos, len);
    }
    let centred = pos as i64 - (target as i64 - len as i64) / 2;
    let last = area_pos as i64 + area_len as i64 - target as i64;
    (centred.clamp(area_pos as i64, last) as i32, target)
}

/// The rect a window had before settings grew it, so the app can put it back
/// when it closes with settings still open (the window-state plugin would save
/// the grown size, and the next start would open enlarged -- K18). Only the
/// first rect counts: a second grow starts from the already grown one.
#[derive(Debug, Default)]
pub struct GrowMemory(Option<WindowRect>);

impl GrowMemory {
    pub const fn new() -> Self {
        Self(None)
    }

    pub fn remember(&mut self, rect: WindowRect) {
        self.0.get_or_insert(rect);
    }

    pub fn forget(&mut self) {
        self.0 = None;
    }

    /// The remembered rect, clearing the memory.
    pub fn take(&mut self) -> Option<WindowRect> {
        self.0.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_size_before_the_first_grow_is_the_one_remembered() {
        let mut memory = GrowMemory::new();
        assert_eq!(memory.take(), None);
        memory.remember(r(10, 20, 400, 300));
        memory.remember(r(0, 0, 900, 700)); // a second grow while the first is open
        assert_eq!(memory.take(), Some(r(10, 20, 400, 300)));
    }

    #[test]
    fn taking_or_forgetting_clears_the_memory() {
        let mut memory = GrowMemory::new();
        memory.remember(r(1, 2, 3, 4));
        assert!(memory.take().is_some());
        assert_eq!(memory.take(), None);
        memory.remember(r(1, 2, 3, 4));
        memory.forget();
        assert_eq!(memory.take(), None);
        memory.remember(r(5, 6, 7, 8)); // and it remembers again afterwards
        assert_eq!(memory.take(), Some(r(5, 6, 7, 8)));
    }

    #[test]
    fn the_inner_size_leaves_room_for_the_frame() {
        // Kade's window: 923x815 outer, a frame of 22x13 around the client area.
        assert_eq!(inner_size_for(r(-958, 141, 923, 815), 22, 13), (901, 802));
    }

    #[test]
    fn without_a_frame_the_size_is_unchanged() {
        assert_eq!(inner_size_for(r(0, 0, 900, 640), 0, 0), (900, 640));
    }

    #[test]
    fn a_frame_bigger_than_the_window_never_gives_zero() {
        assert_eq!(inner_size_for(r(0, 0, 10, 10), 30, 30), (1, 1));
    }

    fn r(x: i32, y: i32, width: u32, height: u32) -> WindowRect {
        WindowRect {
            x,
            y,
            width,
            height,
        }
    }

    const SCREEN: WindowRect = WindowRect {
        x: 0,
        y: 0,
        width: 1920,
        height: 1040,
    };

    #[test]
    fn a_big_enough_window_is_left_alone() {
        assert_eq!(grow_to_fit(r(100, 100, 1000, 700), 900, 640, SCREEN), None);
        assert_eq!(grow_to_fit(r(100, 100, 900, 640), 900, 640, SCREEN), None);
    }

    #[test]
    fn a_small_window_grows_around_its_centre() {
        // centre (500, 400) stays put
        assert_eq!(
            grow_to_fit(r(300, 250, 400, 300), 900, 640, SCREEN),
            Some(r(50, 80, 900, 640))
        );
    }

    #[test]
    fn only_the_short_side_grows() {
        assert_eq!(
            grow_to_fit(r(100, 100, 1200, 300), 900, 640, SCREEN),
            Some(r(100, 0, 1200, 640))
        );
    }

    #[test]
    fn a_window_near_an_edge_is_pushed_back_inside() {
        // bottom-right corner: would overflow right and bottom
        assert_eq!(
            grow_to_fit(r(1700, 900, 200, 100), 900, 640, SCREEN),
            Some(r(1020, 400, 900, 640))
        );
        // partly off the top-left of the screen
        assert_eq!(
            grow_to_fit(r(-150, -50, 400, 300), 900, 640, SCREEN),
            Some(r(0, 0, 900, 640))
        );
    }

    #[test]
    fn a_side_that_does_not_grow_keeps_its_position() {
        // wider than the screen: the width is not touched, only the height grows
        assert_eq!(
            grow_to_fit(r(-50, 500, 2000, 300), 900, 640, SCREEN),
            Some(r(-50, 330, 2000, 640))
        );
    }

    #[test]
    fn it_never_grows_past_the_work_area() {
        let small = r(0, 0, 800, 600);
        assert_eq!(
            grow_to_fit(r(100, 100, 400, 300), 900, 640, small),
            Some(r(0, 0, 800, 600))
        );
    }

    #[test]
    fn a_work_area_off_the_origin_is_respected() {
        // second monitor to the left, taskbar on top
        let left = r(-1920, 40, 1920, 1040);
        assert_eq!(
            grow_to_fit(r(-100, 100, 400, 300), 900, 640, left),
            Some(r(-900, 40, 900, 640))
        );
    }
}
