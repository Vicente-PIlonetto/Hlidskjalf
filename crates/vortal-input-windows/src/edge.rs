//! Platform-neutral portal geometry for a Windows receiver.

use vortal_proto::{Edge, EnterScreen};

/// A monitor rectangle in virtual-desktop pixels; `right` and `bottom` are exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    fn width(&self) -> i32 {
        (self.right - self.left).max(1)
    }

    fn height(&self) -> i32 {
        (self.bottom - self.top).max(1)
    }
}

/// `edge` is the sender's exit edge; the cursor therefore enters on the opposite side here.
/// Picks the monitor that faces the sender, preferring the primary monitor on ties.
pub fn entry_monitor(edge: Edge, monitors: &[(Rect, bool)]) -> Option<Rect> {
    let score = |rect: &Rect| match edge {
        Edge::Right => -rect.left,
        Edge::Left => rect.right,
        Edge::Bottom => -rect.top,
        Edge::Top => rect.bottom,
    };
    monitors
        .iter()
        .max_by_key(|(rect, primary)| (score(rect), *primary))
        .map(|(rect, _)| *rect)
}

/// The pixel where the cursor appears after crossing the sender's portal.
pub fn entry_point(entry: EnterScreen, monitor: Rect) -> (i32, i32) {
    let fraction = f64::from(entry.position) / 65535.0;
    let x = monitor.left + (fraction * f64::from(monitor.width() - 1)) as i32;
    let y = monitor.top + (fraction * f64::from(monitor.height() - 1)) as i32;
    match entry.edge {
        Edge::Right => (monitor.left + 2, y),
        Edge::Left => (monitor.right - 3, y),
        Edge::Top => (x, monitor.bottom - 3),
        Edge::Bottom => (x, monitor.top + 2),
    }
}

/// Whether a motion points back toward the sender.
pub fn toward_sender(edge: Edge, dx: i32, dy: i32) -> bool {
    match edge {
        Edge::Right => dx < 0,
        Edge::Left => dx > 0,
        Edge::Top => dy > 0,
        Edge::Bottom => dy < 0,
    }
}

/// The point one pixel beyond the cursor on the sender's side.
pub fn probe_point(edge: Edge, cursor: (i32, i32)) -> (i32, i32) {
    match edge {
        Edge::Right => (cursor.0 - 1, cursor.1),
        Edge::Left => (cursor.0 + 1, cursor.1),
        Edge::Top => (cursor.0, cursor.1 + 1),
        Edge::Bottom => (cursor.0, cursor.1 - 1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRIMARY: Rect = Rect {
        left: 0,
        top: 0,
        right: 1920,
        bottom: 1080,
    };
    const LEFT_SCREEN: Rect = Rect {
        left: -1280,
        top: 0,
        right: 0,
        bottom: 1024,
    };

    #[test]
    fn enters_on_the_side_facing_the_sender() {
        let monitors = [(PRIMARY, true), (LEFT_SCREEN, false)];
        assert_eq!(entry_monitor(Edge::Right, &monitors), Some(LEFT_SCREEN));
        assert_eq!(entry_monitor(Edge::Left, &monitors), Some(PRIMARY));
        assert_eq!(entry_monitor(Edge::Top, &monitors), Some(PRIMARY));
        let entry = EnterScreen {
            edge: Edge::Right,
            position: 0,
        };
        assert_eq!(entry_point(entry, PRIMARY), (2, 0));
        let entry = EnterScreen {
            edge: Edge::Left,
            position: 65535,
        };
        assert_eq!(entry_point(entry, PRIMARY), (1917, 1079));
        let entry = EnterScreen {
            edge: Edge::Bottom,
            position: 65535,
        };
        assert_eq!(entry_point(entry, PRIMARY), (1919, 2));
    }

    #[test]
    fn returns_only_when_moving_back_toward_the_sender() {
        assert!(toward_sender(Edge::Right, -4, 0));
        assert!(!toward_sender(Edge::Right, 4, 0));
        assert!(toward_sender(Edge::Top, 0, 3));
        assert_eq!(probe_point(Edge::Right, (0, 10)), (-1, 10));
        assert_eq!(probe_point(Edge::Bottom, (5, 0)), (5, -1));
    }
}
