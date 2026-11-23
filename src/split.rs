//! Split view: two tabs shown side by side. Pure bookkeeping (pane maths, which tabs are paired,
//! where new or dragged tabs go, saving/restoring pairs) so it can be tested without a window.
//!
//! Invariant kept by the app: a pair's two tabs are adjacent in the tab list (left right before
//! right), neither is pinned, and a tab is in at most one pair.

use eframe::egui::{self, Rect};

/// Narrowest a pane may get, in points.
pub const MIN_PANE: f32 = 240.0;
/// Space between the two panes (holds the divider).
pub const GAP: f32 = 8.0;
pub const DEFAULT_RATIO: f32 = 0.5;
const RATIO_MIN: f32 = 0.2;
const RATIO_MAX: f32 = 0.8;

/// Two tabs (by id) shown side by side; `ratio` is the left pane's share of the width.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Split {
    pub left: u64,
    pub right: u64,
    pub ratio: f32,
}

/// Keep the divider in 20–80 % and both panes at least `MIN_PANE` wide; a window too narrow for
/// that just shares the width equally.
pub fn clamp_ratio(ratio: f32, width: f32) -> f32 {
    if width < 2.0 * MIN_PANE + GAP {
        return DEFAULT_RATIO;
    }
    let min = MIN_PANE / (width - GAP);
    ratio.clamp(RATIO_MIN, RATIO_MAX).clamp(min, 1.0 - min)
}

/// The left and right pane areas inside `page`, with `GAP` between them.
pub fn pane_rects(page: Rect, ratio: f32) -> (Rect, Rect) {
    let usable = (page.width() - GAP).max(0.0);
    let left_w = usable * clamp_ratio(ratio, page.width());
    let left = Rect::from_min_size(page.min, egui::vec2(left_w, page.height()));
    let right = Rect::from_min_max(egui::pos2(left.right() + GAP, page.top()), page.max);
    (left, right)
}

/// Which pair (index into `splits`) contains `tab`.
pub fn find(splits: &[Split], tab: u64) -> Option<usize> {
    splits.iter().position(|s| s.left == tab || s.right == tab)
}

/// The other tab in `tab`'s pair.
pub fn partner(splits: &[Split], tab: u64) -> Option<u64> {
    let s = splits[find(splits, tab)?];
    Some(if s.left == tab { s.right } else { s.left })
}

fn paired(splits: &[Split], tab: u64) -> bool {
    find(splits, tab).is_some()
}

/// Where a new tab goes: right after the active tab's pair, or at the end.
pub fn insert_index(ids: &[u64], splits: &[Split], active: usize) -> usize {
    ids.get(active)
        .and_then(|&id| find(splits, id))
        .and_then(|k| ids.iter().position(|&id| id == splits[k].right))
        .map_or(ids.len(), |r| r + 1)
}

/// Drag-reorder target for moving `from` towards `to`: paired tabs don't drag, and a tab dragged
/// onto a pair jumps over the whole pair (so it never lands between the halves).
pub fn reorder_target(ids: &[u64], splits: &[Split], from: usize, to: usize) -> Option<usize> {
    if paired(splits, *ids.get(from)?) {
        return None;
    }
    let Some(k) = ids.get(to).and_then(|&id| find(splits, id)) else { return Some(to) };
    let pos = |id: u64| ids.iter().position(|&x| x == id);
    let (l, r) = (pos(splits[k].left)?, pos(splits[k].right)?);
    // Indices are where the tab is inserted after being removed from `from`.
    Some(if to > from { r } else { l })
}

/// The active index after closing tab `closed` (indices into `ids` before the close): the
/// partner if the active half of a split is closed, else the tab that slides into its place.
pub fn active_after_close(ids: &[u64], splits: &[Split], active: usize, closed: usize) -> usize {
    let new_len = ids.len().saturating_sub(1);
    if closed == active {
        if let Some(p) = ids.get(closed).and_then(|&id| partner(splits, id)) {
            if let Some(pi) = ids.iter().position(|&id| id == p) {
                return if pi > closed { pi - 1 } else { pi };
            }
        }
    }
    if closed < active || active >= new_len {
        active.saturating_sub(1).min(new_len.saturating_sub(1))
    } else {
        active
    }
}

/// Which tabs (ids) should be on screen: the fullscreen page alone, else the active tab and its
/// split partner. A fullscreen tab that no longer exists is ignored.
pub fn visible(ids: &[u64], splits: &[Split], active: usize, fullscreen: Option<u64>) -> Vec<u64> {
    if let Some(f) = fullscreen.filter(|f| ids.contains(f)) {
        return vec![f];
    }
    let Some(&cur) = ids.get(active) else { return vec![] };
    let mut v = vec![cur];
    if let Some(p) = partner(splits, cur) {
        v.push(p);
    }
    let pos = |id: &u64| ids.iter().position(|x| x == id);
    v.sort_by_key(pos);
    v
}

/// Ctrl+Tab order: inside a pair visit the other side first, then move on (wrapping).
pub fn cycle(ids: &[u64], splits: &[Split], active: usize, forward: bool) -> usize {
    let n = ids.len();
    if n == 0 {
        return 0;
    }
    if let Some(s) = ids.get(active).and_then(|&id| find(splits, id)).map(|k| splits[k]) {
        if forward && ids[active] == s.left {
            return active + 1;
        }
        if !forward && ids[active] == s.right {
            return active - 1;
        }
    }
    if forward { (active + 1) % n } else { (active + n - 1) % n }
}

/// Pairs as (left index, right index, ratio) into `ids`, for the session file. Pairs with a tab
/// that isn't in `ids` are left out.
pub fn to_saved(ids: &[u64], splits: &[Split]) -> Vec<(usize, usize, f32)> {
    let pos = |id: u64| ids.iter().position(|&x| x == id);
    splits.iter().filter_map(|s| Some((pos(s.left)?, pos(s.right)?, s.ratio))).collect()
}

/// Rebuild pairs from the session file, skipping any that no longer fit the invariant.
pub fn from_saved(ids: &[u64], pinned: &[bool], saved: &[(usize, usize, f32)]) -> Vec<Split> {
    let mut out: Vec<Split> = Vec::new();
    for &(l, r, ratio) in saved {
        // `checked_sub` so a corrupt index (e.g. usize::MAX in a hand-edited file) can't overflow.
        let ok = r.checked_sub(1) == Some(l)
            && r < ids.len()
            && !pinned.get(l).copied().unwrap_or(false)
            && !pinned.get(r).copied().unwrap_or(false)
            && !paired(&out, ids[l])
            && !paired(&out, ids[r]);
        if ok {
            out.push(Split { left: ids[l], right: ids[r], ratio: ratio.clamp(RATIO_MIN, RATIO_MAX) });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui;

    #[test]
    fn panes_split_width_with_gap() {
        let page = egui::Rect::from_min_size(egui::pos2(8.0, 100.0), egui::vec2(1008.0, 600.0));
        let (l, r) = pane_rects(page, 0.5);
        assert_eq!((l.left(), l.width()), (8.0, 500.0));
        assert_eq!((r.left(), r.width()), (516.0, 500.0));
        assert_eq!((l.top(), l.height(), r.height()), (100.0, 600.0, 600.0));
    }

    #[test]
    fn ratio_clamps() {
        assert_eq!(clamp_ratio(0.05, 2000.0), 0.2);
        assert_eq!(clamp_ratio(0.95, 2000.0), 0.8);
        // 600 wide: each pane must be ≥ 240 of the 592 usable → ratio within [0.405.., 0.594..]
        let r = clamp_ratio(0.2, 600.0);
        assert!((r * 592.0 - 240.0).abs() < 0.01);
    }

    #[test]
    fn narrow_window_goes_half_and_half() {
        // Review Focus 3
        assert_eq!(clamp_ratio(0.3, 400.0), 0.5);
        let (l, r) = pane_rects(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 300.0)), 0.3);
        assert!(l.width() > 0.0 && r.width() > 0.0);
    }

    #[test]
    fn lookup_and_insert() {
        let ids = [1, 2, 3, 4];
        let s = [Split { left: 2, right: 3, ratio: 0.5 }];
        assert_eq!(find(&s, 3), Some(0));
        assert_eq!(find(&s, 4), None);
        assert_eq!(partner(&s, 2), Some(3));
        assert_eq!(partner(&s, 1), None);
        assert_eq!(insert_index(&ids, &s, 1), 3); // active = tab 2 (paired) → after tab 3
        assert_eq!(insert_index(&ids, &s, 3), 4); // active unpaired → end
    }

    #[test]
    fn closing_a_split_side_shows_its_partner() {
        let ids = [1, 2, 3, 4];
        let s = [Split { left: 2, right: 3, ratio: 0.5 }];
        // Active right half (index 2) closed: the left half (index 1) is shown, not tab 4.
        assert_eq!(active_after_close(&ids, &s, 2, 2), 1);
        // Active left half (index 1) closed: the right half moves to index 1.
        assert_eq!(active_after_close(&ids, &s, 1, 1), 1);
        // Unpaired: same as before — the tab after it, or the new last tab.
        assert_eq!(active_after_close(&ids, &[], 1, 1), 1);
        assert_eq!(active_after_close(&ids, &[], 3, 3), 2);
        assert_eq!(active_after_close(&ids, &[], 3, 0), 2); // a tab left of the active one
        assert_eq!(active_after_close(&ids, &[], 1, 3), 1); // a tab right of the active one
    }

    #[test]
    fn visible_tabs_fall_back_when_fullscreen_tab_is_gone() {
        let ids = [1, 2, 3];
        let s = [Split { left: 1, right: 2, ratio: 0.5 }];
        assert_eq!(visible(&ids, &s, 0, None), vec![1, 2]);
        assert_eq!(visible(&ids, &s, 0, Some(2)), vec![2]);
        assert_eq!(visible(&ids, &s, 2, None), vec![3]);
        // The fullscreen tab was closed: show the active tab (and partner) instead of nothing.
        assert_eq!(visible(&ids, &s, 0, Some(99)), vec![1, 2]);
    }

    #[test]
    fn corrupt_saved_indices_dont_panic() {
        let ids = [10, 11];
        assert!(from_saved(&ids, &[false, false], &[(usize::MAX, 0, 0.5), (5, 6, 0.5)]).is_empty());
    }

    #[test]
    fn insert_index_without_pairs_is_end() {
        assert_eq!(insert_index(&[1, 2], &[], 0), 2);
    }

    #[test]
    fn reorder_jumps_over_pairs() {
        let ids = [1, 2, 3, 4];
        let s = [Split { left: 2, right: 3, ratio: 0.5 }];
        assert_eq!(reorder_target(&ids, &s, 0, 1), Some(2)); // tab 1 dragged right past pair → after it
        assert_eq!(reorder_target(&ids, &s, 3, 2), Some(1)); // tab 4 dragged left past pair → before it
        assert_eq!(reorder_target(&ids, &s, 1, 0), None); // paired tabs don't drag
        assert_eq!(reorder_target(&ids, &s, 3, 3), Some(3));
    }

    #[test]
    fn cycle_visits_other_side_first() {
        let ids = [1, 2, 3, 4];
        let s = [Split { left: 2, right: 3, ratio: 0.5 }];
        assert_eq!(cycle(&ids, &s, 1, true), 2);
        assert_eq!(cycle(&ids, &s, 2, true), 3);
        assert_eq!(cycle(&ids, &s, 2, false), 1);
        assert_eq!(cycle(&ids, &s, 3, true), 0);
    }

    #[test]
    fn saved_round_trip_and_validation() {
        // Review Focus 4
        let ids = [10, 11, 12, 13];
        let s = vec![Split { left: 11, right: 12, ratio: 0.3 }];
        let saved = to_saved(&ids, &s);
        assert_eq!(saved, vec![(1, 2, 0.3)]);
        assert_eq!(from_saved(&ids, &[false; 4], &saved), s);
        // out of range, not adjacent, pinned, reused tab → skipped
        let bad = [(3, 4, 0.5), (0, 2, 0.5), (0, 1, 0.5), (1, 2, 0.5), (2, 3, 0.5)];
        let got = from_saved(&ids, &[true, false, false, false], &bad);
        assert_eq!(got, vec![Split { left: 11, right: 12, ratio: 0.5 }]);
        assert!(to_saved(&[10, 12], &s).is_empty()); // tab 11 missing → pair dropped
    }
}
