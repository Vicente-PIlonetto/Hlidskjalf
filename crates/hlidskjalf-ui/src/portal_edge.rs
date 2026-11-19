//! Decides when the cursor crosses a portal edge. Pure geometry, so it is tested on every OS.

/// A monitor rectangle in physical desktop pixels, as `GetCursorPos` reports them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScreenRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl ScreenRect {
    pub fn right(&self) -> i32 {
        self.x.saturating_add(self.width as i32)
    }

    pub fn bottom(&self) -> i32 {
        self.y.saturating_add(self.height as i32)
    }

    pub fn contains(&self, (x, y): (i32, i32)) -> bool {
        x >= self.x && x < self.right() && y >= self.y && y < self.bottom()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
    Top,
    Bottom,
}

impl Side {
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "Left" => Self::Left,
            "Right" => Self::Right,
            "Top" => Self::Top,
            "Bottom" => Self::Bottom,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Left => "Left",
            Self::Right => "Right",
            Self::Top => "Top",
            Self::Bottom => "Bottom",
        }
    }

    pub fn vertical(self) -> bool {
        matches!(self, Self::Left | Self::Right)
    }
}

/// The part of a monitor edge that opens the portal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Portal {
    pub screen: ScreenRect,
    pub side: Side,
    /// Offset along the edge, from the monitor's top (left/right edges) or left (top/bottom).
    pub start: i64,
    pub length: i64,
}

impl Portal {
    fn edge_length(&self) -> i64 {
        i64::from(if self.side.vertical() {
            self.screen.height
        } else {
            self.screen.width
        })
    }

    /// The visible part of the portal on its monitor, in desktop pixels: (x, y, width, height)
    /// of a zero-thickness line along the edge.
    pub fn segment(&self) -> (i32, i32, i32, i32) {
        let from = self.start.clamp(0, self.edge_length());
        let to = (self.start + self.length).clamp(from, self.edge_length());
        let (from, span) = (from as i32, (to - from) as i32);
        let s = self.screen;
        match self.side {
            Side::Left => (s.x, s.y + from, 0, span),
            Side::Right => (s.right(), s.y + from, 0, span),
            Side::Top => (s.x + from, s.y, span, 0),
            Side::Bottom => (s.x + from, s.bottom(), span, 0),
        }
    }
}

/// Returns the entry position (0..=65535 along the portal) when moving from `old` to `point`
/// pushes the cursor into the portal edge, and `None` otherwise.
///
/// The portal never opens where another local monitor continues past the edge: there the
/// cursor must be free to move onto that monitor.
pub fn crossing(
    screens: &[ScreenRect],
    portal: &Portal,
    old: (i32, i32),
    point: (i32, i32),
) -> Option<u16> {
    let screen = portal.screen;
    if !screen.contains(point) || !screen.contains(old) {
        return None;
    }
    let reached = match portal.side {
        Side::Right => point.0 >= screen.right() - 2 && point.0 > old.0,
        Side::Left => point.0 <= screen.x + 1 && point.0 < old.0,
        Side::Bottom => point.1 >= screen.bottom() - 2 && point.1 > old.1,
        Side::Top => point.1 <= screen.y + 1 && point.1 < old.1,
    };
    if !reached {
        return None;
    }
    let beyond = match portal.side {
        Side::Right => (screen.right(), point.1),
        Side::Left => (screen.x - 1, point.1),
        Side::Bottom => (point.0, screen.bottom()),
        Side::Top => (point.0, screen.y - 1),
    };
    if screens
        .iter()
        .any(|other| *other != screen && other.contains(beyond))
    {
        return None;
    }
    let along = i64::from(if portal.side.vertical() {
        point.1 - screen.y
    } else {
        point.0 - screen.x
    });
    let length = portal.length.max(1);
    if along < portal.start || along >= portal.start + length {
        return None;
    }
    let fraction = (along - portal.start) as f64 / (length - 1).max(1) as f64;
    Some((fraction * 65535.0).round().clamp(0.0, 65535.0) as u16)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEFT: ScreenRect = ScreenRect {
        x: 0,
        y: 0,
        width: 1920,
        height: 1080,
    };
    const RIGHT: ScreenRect = ScreenRect {
        x: 1920,
        y: 0,
        width: 1920,
        height: 1080,
    };

    fn portal(screen: ScreenRect, side: Side) -> Portal {
        let length = i64::from(if side.vertical() {
            screen.height
        } else {
            screen.width
        });
        Portal {
            screen,
            side,
            start: 0,
            length,
        }
    }

    #[test]
    fn opens_at_the_outer_edge_of_two_side_by_side_monitors() {
        let screens = [LEFT, RIGHT];
        let p = portal(RIGHT, Side::Right);
        assert_eq!(
            crossing(&screens, &p, (3830, 540), (3839, 540)),
            Some(32798)
        );
        assert_eq!(crossing(&screens, &p, (3830, 0), (3839, 0)), Some(0));
        assert_eq!(
            crossing(&screens, &p, (3830, 1079), (3839, 1079)),
            Some(65535)
        );
    }

    #[test]
    fn never_traps_the_cursor_between_local_monitors() {
        let screens = [LEFT, RIGHT];
        let p = portal(LEFT, Side::Right);
        assert_eq!(crossing(&screens, &p, (1910, 540), (1919, 540)), None);
    }

    #[test]
    fn opens_where_a_shorter_neighbour_leaves_the_edge_free() {
        let short = ScreenRect {
            x: 1920,
            y: 0,
            width: 1280,
            height: 720,
        };
        let screens = [LEFT, short];
        let p = portal(LEFT, Side::Right);
        assert_eq!(crossing(&screens, &p, (1910, 300), (1919, 300)), None);
        assert!(crossing(&screens, &p, (1910, 900), (1919, 900)).is_some());
    }

    #[test]
    fn ignores_motion_away_from_the_edge_and_outside_the_segment() {
        let screens = [LEFT];
        let p = Portal {
            start: 200,
            length: 400,
            ..portal(LEFT, Side::Right)
        };
        assert_eq!(crossing(&screens, &p, (1919, 300), (1910, 300)), None);
        assert_eq!(crossing(&screens, &p, (1910, 100), (1919, 100)), None);
        assert_eq!(crossing(&screens, &p, (1910, 700), (1919, 700)), None);
        assert_eq!(
            crossing(&screens, &p, (1910, 400), (1919, 400)),
            Some(32850)
        );
    }

    #[test]
    fn handles_negative_coordinates_and_stacked_monitors() {
        let above = ScreenRect {
            x: 0,
            y: -1080,
            width: 1920,
            height: 1080,
        };
        let screens = [LEFT, above];
        assert_eq!(
            crossing(&screens, &portal(LEFT, Side::Top), (500, 10), (500, 0)),
            None
        );
        assert!(crossing(
            &screens,
            &portal(above, Side::Top),
            (500, -1070),
            (500, -1080)
        )
        .is_some());
        assert!(crossing(&screens, &portal(above, Side::Left), (10, -500), (0, -500)).is_some());
    }

    #[test]
    fn ignores_jumps_from_another_monitor() {
        let screens = [LEFT, RIGHT];
        let p = portal(RIGHT, Side::Right);
        assert_eq!(crossing(&screens, &p, (1000, 540), (3839, 540)), None);
    }

    #[test]
    fn sides_round_trip_through_the_saved_names() {
        for side in [Side::Left, Side::Right, Side::Top, Side::Bottom] {
            assert_eq!(Side::parse(side.as_str()), Some(side));
        }
        assert_eq!(Side::parse("left"), None);
    }

    #[test]
    fn segment_is_clipped_to_the_monitor() {
        let p = Portal {
            start: -100,
            length: 500,
            ..portal(RIGHT, Side::Right)
        };
        assert_eq!(p.segment(), (3840, 0, 0, 400));
        let p = Portal {
            start: 1000,
            length: 500,
            ..portal(LEFT, Side::Bottom)
        };
        assert_eq!(p.segment(), (1000, 1080, 500, 0));
    }
}
