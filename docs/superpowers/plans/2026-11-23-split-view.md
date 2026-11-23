# Split View Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Two tabs can be joined into a split pair shown side by side, kept across tab switches and restarts.

**Architecture:** All pair bookkeeping that can be pure lives in a new `src/split.rs` (pane maths, pair lookup, insertion/reorder/cycle indices, session conversion), unit-tested. `src/main.rs` holds `splits: Vec<split::Split>` and wires it into layout, tab switching, toolbar targeting, menus and persistence. `src/chrome.rs` draws a pair as one joined pill. Active side comes from a new WebView2 focus event in `src/wv2.rs`.

**Tech Stack:** Rust 2024, eframe/egui 0.37, wry 0.57 + webview2-com 0.39 (Windows only). Build/test via `.\build.ps1 test` / `.\build.ps1 build` (PowerShell, from the repo root).

**Spec:** `docs/superpowers/specs/2026-11-23-split-view-design.md`

## Global Constraints

- Windows only; no new dependencies.
- No new IPC message types (active side comes from WebView2's focus event, not page messages).
- Must not slow down normal single-tab browsing (layout still only touched when its key changes).
- Pair invariant: the two tabs are adjacent, left immediately before right, neither pinned; a tab is in at most one pair.
- `ratio` = left share of width, default `0.5`, clamped `0.2`–`0.8`; minimum pane width `240` pt; gap between panes = `INSET` (8 pt).
- Old `session.json` files (no `splits` field) must still load.

## Review Focus

1. Closing a paired tab by *any* route (×, Ctrl+W, middle-click, menu Close, Close others/right) leaves no `Split` pointing at a missing tab — expect the survivor at full width.
2. A page-opened tab while a pair is shown must not hide the pair or steal toolbar focus — expect the pair to stay visible and the active side unchanged.
3. Narrow window (< 2×240 + gap) — expect both panes visible at 50/50, no negative widths.
4. Restoring a session whose split indices no longer fit (tab failed to open, extension tab filtered) — expect the bad pair skipped, others kept, no panic.
5. Switching active side while the find bar or address bar is in use — expect find to close and the address bar to show the new side's URL.

Tests pinning these are added in Tasks 1, 2 and 5 (marked "Review Focus").

---

### Task 1: Pure split model (`src/split.rs`)

**Files:**
- Create: `src/split.rs` (with `#[cfg(test)] mod tests`)
- Modify: `src/main.rs` (add `mod split;`)

**Interfaces:**
- Produces (all `pub` in `split`):
  - `pub const MIN_PANE: f32 = 240.0;` `pub const GAP: f32 = 8.0;` `pub const DEFAULT_RATIO: f32 = 0.5;`
  - `#[derive(Clone, Copy, Debug, PartialEq)] pub struct Split { pub left: u64, pub right: u64, pub ratio: f32 }`
  - `pub fn clamp_ratio(ratio: f32, width: f32) -> f32` — clamp to 0.2–0.8, then so both panes ≥ `MIN_PANE`; if `width < 2*MIN_PANE + GAP`, return `0.5`.
  - `pub fn pane_rects(page: egui::Rect, ratio: f32) -> (egui::Rect, egui::Rect)` — uses `clamp_ratio`; left width = `(page.width() - GAP) * ratio`; gap between.
  - `pub fn find(splits: &[Split], tab: u64) -> Option<usize>` (index into `splits` of the pair containing `tab`)
  - `pub fn partner(splits: &[Split], tab: u64) -> Option<u64>`
  - `pub fn insert_index(ids: &[u64], splits: &[Split], active: usize) -> usize` — if `ids[active]` is paired, the index right after the pair's right tab; else `ids.len()`.
  - `pub fn reorder_target(ids: &[u64], splits: &[Split], from: usize, to: usize) -> Option<usize>` — `None` if `ids[from]` is paired; if `ids[to]` is paired, return the index that puts the dragged tab on the far side of the whole pair; else `Some(to)`.
  - `pub fn cycle(ids: &[u64], splits: &[Split], active: usize, forward: bool) -> usize` — inside a pair, first go to the other side (forward from left → right; backward from right → left), otherwise next/prev index with wrap.
  - `pub fn to_saved(ids: &[u64], splits: &[Split]) -> Vec<(usize, usize, f32)>` — indices into `ids`; pairs with a tab not in `ids` are dropped.
  - `pub fn from_saved(ids: &[u64], pinned: &[bool], saved: &[(usize, usize, f32)]) -> Vec<Split>` — keep only entries with `r == l + 1`, both in range, neither pinned, no tab reused; ratio clamped to 0.2–0.8.

- [ ] **Step 1: Write the failing tests** in `src/split.rs`:

```rust
#[test] fn panes_split_width_with_gap() {
    let page = egui::Rect::from_min_size(egui::pos2(8.0, 100.0), egui::vec2(1008.0, 600.0));
    let (l, r) = pane_rects(page, 0.5);
    assert_eq!((l.left(), l.width()), (8.0, 500.0));
    assert_eq!((r.left(), r.width()), (516.0, 500.0));
    assert_eq!((l.top(), l.height(), r.height()), (100.0, 600.0, 600.0));
}
#[test] fn ratio_clamps() {
    assert_eq!(clamp_ratio(0.05, 2000.0), 0.2);
    assert_eq!(clamp_ratio(0.95, 2000.0), 0.8);
    // 600 wide: each pane must be ≥ 240 of the 592 usable → ratio within [0.405.., 0.594..]
    let r = clamp_ratio(0.2, 600.0);
    assert!((r * 592.0 - 240.0).abs() < 0.01);
}
#[test] fn narrow_window_goes_half_and_half() { // Review Focus 3
    assert_eq!(clamp_ratio(0.3, 400.0), 0.5);
    let (l, r) = pane_rects(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 300.0)), 0.3);
    assert!(l.width() > 0.0 && r.width() > 0.0);
}
#[test] fn lookup_and_insert() {
    let ids = [1, 2, 3, 4];
    let s = [Split { left: 2, right: 3, ratio: 0.5 }];
    assert_eq!(find(&s, 3), Some(0)); assert_eq!(find(&s, 4), None);
    assert_eq!(partner(&s, 2), Some(3)); assert_eq!(partner(&s, 1), None);
    assert_eq!(insert_index(&ids, &s, 1), 3); // active = tab 2 (paired) → after tab 3
    assert_eq!(insert_index(&ids, &s, 3), 4); // active unpaired → end
}
#[test] fn reorder_jumps_over_pairs() {
    let ids = [1, 2, 3, 4];
    let s = [Split { left: 2, right: 3, ratio: 0.5 }];
    assert_eq!(reorder_target(&ids, &s, 0, 1), Some(2)); // tab 1 dragged right past pair → after it
    assert_eq!(reorder_target(&ids, &s, 3, 2), Some(1)); // tab 4 dragged left past pair → before it
    assert_eq!(reorder_target(&ids, &s, 1, 0), None);    // paired tabs don't drag
    assert_eq!(reorder_target(&ids, &s, 3, 3), Some(3));
}
#[test] fn cycle_visits_other_side_first() {
    let ids = [1, 2, 3, 4];
    let s = [Split { left: 2, right: 3, ratio: 0.5 }];
    assert_eq!(cycle(&ids, &s, 1, true), 2);
    assert_eq!(cycle(&ids, &s, 2, true), 3);
    assert_eq!(cycle(&ids, &s, 2, false), 1);
    assert_eq!(cycle(&ids, &s, 3, true), 0);
}
#[test] fn saved_round_trip_and_validation() { // Review Focus 4
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
```

- [ ] **Step 2: Run** `.\build.ps1 test` — Expected: compile errors (functions not defined).
- [ ] **Step 3: Implement** the Interfaces above in `src/split.rs`; add `mod split;` in `main.rs`.
- [ ] **Step 4: Run** `.\build.ps1 test` — Expected: all tests pass (existing 28 + 7 new).
- [ ] **Step 5: Commit** `git add src/split.rs src/main.rs && git commit -m "Add pure split view model"`

---

### Task 2: Persist pairs in the session (`src/store.rs`)

**Files:** Modify `src/store.rs` (`Session`, tests)

**Interfaces:**
- Produces: `Session.splits: Vec<(usize, usize, f32)>` (serde default via existing `#[serde(default)]`).

- [ ] **Step 1: Extend test** `session_pins_round_trip`: old JSON without `splits` loads with `s.splits.is_empty()`; a `Session { …, splits: vec![(0, 1, 0.4)] }` round-trips equal.
- [ ] **Step 2: Run** `.\build.ps1 test` — Expected: compile error (no field `splits`).
- [ ] **Step 3: Add** `pub splits: Vec<(usize, usize, f32)>` to `Session` with a doc comment ("split pairs: indices into `tabs`, left share of width"). Fix the one struct literal in `main.rs::save_session_if_changed` with `splits: vec![]` for now.
- [ ] **Step 4: Run** `.\build.ps1 test` — Expected: pass.
- [ ] **Step 5: Commit** `git commit -am "Session: store split pairs"`

---

### Task 3: WebView2 focus event (`src/wv2.rs`)

**Files:** Modify `src/wv2.rs`

**Interfaces:**
- Produces: `pub fn on_focus(wv: &WebView, notify: impl Fn() + 'static)` — registers `controller.add_GotFocus` (webview2-com `FocusChangedEventHandler`) and calls `notify` when the tab's webview gains focus.

- [ ] **Step 1: Implement** following the pattern of `on_zoom` (controller event, handler created with `FocusChangedEventHandler::create`).
- [ ] **Step 2: Run** `.\build.ps1 build` — Expected: `Finished` with no warnings other than `on_focus` unused.
- [ ] **Step 3: Commit** `git commit -am "wv2: report when a tab's page gains focus"`

---

### Task 4: Pairs in the app — create, show, layout, active side, end

**Files:** Modify `src/main.rs`

**Interfaces:**
- Consumes: Task 1 (`split::*`), Task 3 (`wv2::on_focus`).
- Produces (methods on `BrowserApp`, used by Tasks 5–7):
  - field `splits: Vec<split::Split>`
  - `fn shown_split(&self) -> Option<split::Split>` — the pair containing `tabs[active]`, if any
  - `fn start_split(&mut self, with: Option<usize>)` — `Some(i)`: move tab `i` (via `reposition`) to right after `active`, pair (active = left); `None`: open start page as right side and focus it. No-op if active is pinned or paired, or `i` is pinned/paired/active.
  - `fn end_split(&mut self, tab: u64)` — remove the pair containing `tab`; active stays; relayout.
  - `fn swap_split(&mut self, tab: u64)` — swap left/right tabs in the list and the `Split`, `ratio = 1.0 - ratio`.
  - `fn focus_side(&mut self, tab: u64)` — if `tab` is in the shown pair and not active: set `active` to its index, close find bar (as `after_switch` does, without hiding), `shown_url.clear()`.
  - `Ev::Focus(u64)` event from `wv2::on_focus` (registered in `open_tab`) → `focus_side`.
  - `Action::Split` ("split" in `Action::from_ipc`, `key.js` Ctrl+Shift+S, `shortcut()` Ctrl+Shift+S): if active paired → `end_split`, else if not pinned → `start_split(None)`.

- [ ] **Step 1: Layout.** Replace the single-bounds loop in `ui()` (`for t in &self.tabs { set_bounds(b) … }`) with: unpaired tabs get `page_rect`; tabs of every pair get their `split::pane_rects(page, ratio)` side; `round_corners` per rect. Extend the `last_layout` key to `(w, h, top, shown_split_key)` where `shown_split_key = shown_split().map(|s| (s.left, s.right, (s.ratio * 1000.0) as i32))` so divider moves and pair switches re-layout.
- [ ] **Step 2: Showing.** In `select(i)`: hide every visible tab not in the target's pair, then `set_visible(true)` on the target and its partner. Same in `close` after choosing the new active.
- [ ] **Step 3: Menu items** in `tab_menu`: add per spec — "Split view with current tab" (cmd `tab:split:<id>`, disabled per rules), "Split view with new tab" (`tab:splitnew:<id>`, on the active unpaired tab), and on paired tabs "Swap sides" (`tab:swap:<id>`) and "Close split view" (`tab:unsplit:<id>`). Handle them in `tab_cmd`.
- [ ] **Step 4: Ending on close/pin.** At the top of `close(i)`: `end_split(tabs[i].id)` if paired. In `set_pinned(i, true)`: `end_split` first.
- [ ] **Step 5: Run** `.\build.ps1 test` then `.\build.ps1 build` — Expected: pass / `Finished`.
- [ ] **Step 6: Manual check** (debug build with `target\lbtest\drive.ps1`): right-click tab → "Split view with new tab" shows two panes; clicking each page moves the address bar to it; "Close split view" returns to full width; closing one side leaves the other full width (Review Focus 1, 5).
- [ ] **Step 7: Commit** `git commit -am "Split view: create, show and end pairs"`

---

### Task 5: Divider, active outline, new-tab placement, cycling

**Files:** Modify `src/main.rs`, `src/scripts/key.js` (only if Task 4 didn't add Ctrl+Shift+S)

**Interfaces:**
- Consumes: Task 1 (`insert_index`, `cycle`, `clamp_ratio`), Task 4 methods.
- Produces: `struct NewTab { url: String, at: Option<usize>, switch: bool }`; `pending: Vec<NewTab>`; `fn open_tab(&mut self, frame, ctx, req: NewTab)`.

- [ ] **Step 1: Divider.** When a pair is shown and not fullscreen: egui `interact` over the gap between panes (gap + 3 pt each side, pane height), `CursorIcon::ResizeHorizontal`; drag sets that pair's `ratio = clamp_ratio((pointer_x - page.left()) / (page.width() - GAP), page.width())`; `double_clicked()` → `DEFAULT_RATIO`. Draw a 4×36 pt rounded grip at the gap centre (`pal.border`, `pal.accent` while hovered/dragged).
- [ ] **Step 2: Active outline.** Stroke the active pane rect expanded by 2 pt with `Stroke::new(2.0, pal.accent)`, radius `PAGE_RADIUS + 2`.
- [ ] **Step 3: New-tab placement.** Change `pending` to `Vec<NewTab>`. User-opened tabs (`Action::New`, `+`, `tab:dup`, `Action::Reopen`, bookmark `newtab`/middle-click, `show_page`, extension `open`): `at = shown_split().map(|_| split::insert_index(..))`, `switch: true`. Page-opened (`Ev::NewTab`): if a pair is shown → `at` = insert index, `switch: false`; else `at: None, switch: true`. Session restore entries: `at: None, switch: true`. In `open_tab`: insert at `at` (default end); when `switch` is false don't hide other tabs, keep `active` pointing at the same tab id, create the webview hidden.
- [ ] **Step 4: Cycling.** `Action::NextTab`/`PrevTab` use `split::cycle(..)`; within the shown pair call `focus_side` (and `webview.focus()`) instead of `select`.
- [ ] **Step 5: Write test** in `main.rs` tests or `split.rs` already covers indices; add a `split.rs` test `insert_index_without_pairs_is_end` (`insert_index(&[1,2], &[], 0) == 2`). Run `.\build.ps1 test` — Expected: pass.
- [ ] **Step 6: Manual check:** with a pair shown, open a `target=_blank` link in one pane (e.g. a Wikipedia language link with middle-click) → pair stays, new tab appears after the pair (Review Focus 2); Ctrl+T switches to a new tab after the pair; divider drag resizes, double-click resets; narrow the window → panes clamp (Review Focus 3).
- [ ] **Step 7: Commit** `git commit -am "Split view: divider, active outline, new tab placement"`

---

### Task 6: Joined pill in the tab strip (`src/chrome.rs`)

**Files:** Modify `src/chrome.rs`, `src/main.rs` (TabView construction, event handling)

**Interfaces:**
- Consumes: `split::Split`, `split::reorder_target`.
- Produces: `TabView.pair: PairPos` where `pub enum PairPos { None, Left, Right }`; `tab_strip` signature unchanged. `TabEvent::Move` is still emitted with raw indices; `main.rs::move_tab` maps it through `split::reorder_target` (replacing the pinned-boundary check, which it keeps).

- [ ] **Step 1: Draw pairs.** In `tab_strip`, a `Left` tab and the following `Right` tab are laid out with no gap between them and drawn as one card (active look if either is `active`), with a 1 pt `pal.border` vertical line between halves; the active half's title in `pal.text`, the other `pal.muted`. Each half keeps its own icon/spinner, speaker and close button. Clicking a half emits `Select(i)` (main.rs: if the pair is already shown, `focus_side` instead of `select`).
- [ ] **Step 2: No drag on paired halves** (skip `Move` events when `t.pair != PairPos::None`).
- [ ] **Step 3: Reorder past pairs** in `move_tab`: `if let Some(to) = split::reorder_target(&ids, &self.splits, from, to) { if pinned boundary ok { self.reposition(from, to) } }`.
- [ ] **Step 4: Run** `.\build.ps1 test` / `.\build.ps1 build` — Expected: pass / `Finished`.
- [ ] **Step 5: Manual check:** pair shows as one pill; clicking halves switches side; dragging a normal tab across the pill jumps over it; paired halves don't drag.
- [ ] **Step 6: Commit** `git commit -am "Tab strip: draw split pairs as one tab"`

---

### Task 7: Persistence and fullscreen

**Files:** Modify `src/main.rs`

**Interfaces:**
- Consumes: Task 1 (`to_saved`, `from_saved`), Task 2 (`Session.splits`).

- [ ] **Step 1: Save.** In `save_session_if_changed`, compute `ids` of the kept (non-extension) tabs and set `splits: split::to_saved(&ids, &self.splits)` (pairs whose tabs aren't kept drop out).
- [ ] **Step 2: Restore.** Where `restore_active` is applied (after pinned flags): `self.splits = split::from_saved(&ids, &pinned, &self.saved_session.splits)`; then `select` as today so both panes show if the active tab is paired.
- [ ] **Step 3: Fullscreen.** While `self.fullscreen` and a pair is shown, give the tab whose page went fullscreen the full window bounds and hide its partner; on exit, restore (include `fullscreen` in the layout key so it re-lays out). `Ev::Act(Action::Fullscreen(on))` from `on_fullscreen` must carry the tab id → change the event to `Ev::Fullscreen(u64, bool)`.
- [ ] **Step 4: Run** `.\build.ps1 test` / `.\build.ps1 build` — Expected: pass / `Finished`.
- [ ] **Step 5: Manual check:** make a pair, restart the debug app → pair restored with the same divider position; YouTube video fullscreen in one pane fills the window, Esc restores the split.
- [ ] **Step 6: Commit** `git commit -am "Split view: save pairs and handle video fullscreen"`

---

### Task 8: Final verification and install

- [ ] **Step 1:** `.\build.ps1 test` — all pass. `.\build.ps1 build --release` — `Finished`.
- [ ] **Step 2:** Walk the Review Focus list once more in the debug build.
- [ ] **Step 3:** Ask the user before pushing; on approval `git push`. Offer to run `install.ps1`.
