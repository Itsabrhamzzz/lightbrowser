# Split view — design

Date: 2026-11-23 · Status: ready for approval (builder check done)

## Intent

Let the user multitask by showing two tabs side by side in one window — e.g. a video or chat
beside the page they're working on — and keep that arrangement while they jump between other
tabs. A split is a lasting working setup, not a one-off view.

## Goals

- Join two tabs into a **split pair** that shows both pages side by side.
- The pair behaves like **one item** in the tab strip: clicking another tab hides the pair;
  clicking the pair brings both pages back exactly as they were.
- **Several pairs** can exist at once.
- Pairs **survive a restart** (with the existing "Reopen tabs on start" setting).
- The toolbar (address bar, back/forward/reload, star, shield, zoom badge, find) acts on the
  **active side** — the side last clicked — which is clearly marked.
- A **draggable divider** between the two pages sets how the width is shared.

## Non-goals (v1)

- More than two pages in one view; vertical (top/bottom) splits; free tiling.
- Splitting pinned tabs.
- Dragging a tab onto another tab to make a pair (conflicts with drag-to-reorder; see Open
  decisions). v1 starts splits from the tab menu and a shortcut.
- Drag-reordering a pair in the tab strip (pairs stay where they were made; dissolve to move).

## Anti-goals (would make it a failure even if it "works")

- The toolbar acting on the wrong side (typing a URL loads it in the side you didn't mean).
- A pair silently falling apart when switching tabs, or coming back with the wrong sizes.
- Closing one side leaving a blank half or a broken layout.

## Approaches considered

| | Approach | Trade-off |
|---|---|---|
| A | One temporary split; switching to any other tab ends it | Simplest, but the user rebuilds it constantly — defeats multitasking |
| **B** | **Split pairs that act like one tab, several allowed, saved across restarts** | **Chosen.** Keeps a working setup intact while the user moves around; fits how tabs already work |
| C | Free tiling (up to 4 panes, drag-to-dock) | Much more code and screen juggling than multitasking needs |

The user picked **B**.

## How it works today (relevant facts)

- Each tab is its own WebView2 child window. Switching tabs only toggles visibility (`select`,
  `src/main.rs`). All tab webviews get the same bounds from `page_rect()` and are re-laid out
  only when the window size or `content_top()` changes (`last_layout` check in `ui()`).
- The page sits inset from the window as a rounded "card" (`INSET`, `PAGE_RADIUS`,
  `wv2::round_corners`). The gap around it is drawn by egui and is *not* covered by a webview —
  so egui can draw there (divider, active-side outline).
- Everything that acts on "the current tab" uses `self.active` / `active_wv()`.
- Page clicks reach the app as the `pagedown` IPC message from `key.js` (token-protected); `key.js`
  runs in the main frame only, so clicks inside iframes don't send it.
- New tabs all go through `pending` → `open_tab`, which appends at the end, hides every other
  webview and switches to the new tab.
- Tabs are a `Vec<Tab>`; pinned tabs are always first. Session = list of URLs + active index +
  pinned count (`store::Session`).

## Design

### Model

- `BrowserApp.splits: Vec<Split>` where `Split { left: u64, right: u64, ratio: f32 }` (tab ids;
  `ratio` = left side's share of the width, default 0.5, clamped 0.2–0.8).
- **Invariant:** the two tabs of a pair are adjacent in the tab list, left immediately before
  right, and neither is pinned.
- A tab belongs to at most one pair. Helper: `split_of(tab_id) -> Option<&Split>`.
- `self.active` keeps meaning "the tab the toolbar acts on". When a pair is shown, `active` is
  the index of its **active side**.

### Starting a split

- Tab right-click menu (existing `tab_menu`), new items:
  - On a non-active, unpinned, unpaired tab: **"Split view with current tab"** — moves that tab
    to sit right after the active tab and pairs them (active tab = left, chosen tab = right).
    Disabled if the active tab is pinned or already paired.
  - On the active unpaired tab: **"Split view with new tab"** — opens the start page as the right
    side.
  - On a paired tab: **"Swap sides"**, **"Close split view"** (keeps both tabs as normal tabs).
- Shortcut **Ctrl+Shift+S**: if the active tab is in a pair → close that split; otherwise split
  the active tab with a new tab. (Added to `shortcut()` and `key.js`.)
- New pair focuses the right side for "with new tab" (so the user can type an address), and keeps
  focus on the current side for "with current tab".

### Layout

- `page_rect()` stays the full content area. New `pane_rects(page, ratio) -> (left, right)`:
  splits the width with an `INSET`-wide gap (8 pt) for the divider; each pane keeps rounded
  corners (`round_corners` per pane, same radius).
- Minimum pane width 240 pt; if the window is narrower than that allows, ratio is clamped so both
  panes stay ≥ 240 pt (if even that's impossible, panes just share 50/50).
- Re-layout trigger: extend the `last_layout` key with the shown pair's ratio (and which tabs are
  shown), so bounds update on resize, divider drag, and switching.
- **Showing**: selecting either tab of a pair shows both webviews with pane bounds; selecting any
  other tab hides both. Other (unpaired) tabs keep full-width bounds.

### Divider

- egui interact area in the gap between panes (gap width + a few pt of slop), full pane height,
  resize cursor (`ResizeHorizontal`).
- Drag updates `ratio` live; **double-click** resets to 0.5.
- Drawn as a short rounded grip in the gap's centre (subtle, accent on hover/drag).

### Active side

- Marked with a 2 pt accent outline drawn by egui just outside that pane's rect (in the gap, so
  it's visible around the webview). The inactive side has no outline.
- Becomes active when that side's webview **gains keyboard focus** — detected with WebView2's
  native controller `GotFocus` event (new `wv2::on_focus`, tagged with the tab id). This also
  catches clicks inside iframes (video players, chat widgets), which never send `pagedown`
  because `key.js` runs in the main frame only. Also: clicking that half of the pair in the tab
  strip, or Ctrl+Tab. `pagedown` keeps its current job (closing the dropdown panel).
  *(Builder's call, from the builder check: clicks-only would leave iframe clicks on the wrong
  side — the first anti-goal.)*
- Switching active side updates `self.active`, which makes the address bar, back/forward, reload,
  star, shield, zoom badge and find bar follow it. An open find bar closes on side switch (same as
  a tab switch today).

### Tab strip

- A pair is drawn as **one joined pill** containing both tabs' favicon + title, separated by a
  thin vertical line. Width = two normal tab widths (it shares space like two tabs).
- Clicking a half shows the pair and makes that side active. Each half keeps its own close
  button (visible on hover), its loading spinner and its speaker/mute icon.
- The active half is drawn in the normal text colour, the other muted; the whole pill gets the
  "active tab" card look when the pair is shown.
- Paired tabs are not drag-reorderable (drag on a pair does nothing in v1).
- Dragging a **normal** tab past a pair makes it **jump over the pair as one block** (never lands
  between the halves), so the adjacency invariant always holds. *(Builder's call.)*

### Ending a split

- **Close split view** (menu / Ctrl+Shift+S): remove the `Split`; both tabs stay, in order, as
  normal tabs; the active side stays active and goes full width.
- **Closing either tab** of a pair (×, Ctrl+W, middle-click, "Close"): dissolve the pair first,
  then close the tab as today; the other tab becomes active at full width.
- Closing the last tab in the window keeps today's behaviour (start page).
- "Close other tabs" / "Close tabs to the right": a pair counts as two tabs; if either side is
  closed, the pair dissolves.

### Interactions with existing features

- **Video fullscreen** (WebView2 fullscreen event): the pane whose page went fullscreen fills
  the window; the other is hidden until fullscreen ends; then the split is restored.
- **Pinned tabs**: can't be split; pinning a paired tab first closes its split.
- **Duplicate / reload / mute** on a paired tab act on that tab only.
- **Dropdown panel** (menus, suggestions, extension popups): unchanged — it floats over both panes.
- **Bookmarks bar / find strip / download toast**: change `content_top()` as today; panes follow.
- **New tabs while a pair is shown** (user decision): every new tab is inserted **right after the
  pair** in the tab list (today `open_tab` always appends at the end).
  - Opened *by a page* (link with `target=_blank` / `window.open` → `Ev::NewTab`): opens **in the
    background** — the pair stays on screen and active; the new tab just appears after the pair.
  - Opened *by the user* (Ctrl+T, the + button, Duplicate, Reopen closed tab, bookmark
    middle-click / "Open in new tab"): inserted after the pair and **switched to** (the pair hides
    until clicked again).
  - With no pair shown, placement stays as today (append at the end) and new tabs are switched to.
  - This needs `pending` entries to carry "where" and "switch to it or not", and `open_tab` to
    insert at an index without hiding the shown pages when opening in the background.

### Persistence

- `store::Session` gains `splits: Vec<(usize, usize, f32)>` = (left index, right index, ratio),
  indices into the saved `tabs` list (serde default empty, so old session files still load).
- Saved whenever it changes (existing `save_session_if_changed`). Indices are computed **after**
  extension (`chrome-extension://`) tabs are filtered out of the saved list; a pair containing an
  extension tab isn't saved.
- Restore: after all session tabs open (where `restore_active` is applied today), rebuild pairs
  from the indices, skipping any that break the invariant (out of range, not adjacent, pinned).

## Constraints

- Windows only, WebView2 + egui as today; no new dependencies.
- No new IPC message types (active side comes from WebView2's focus event, not page messages).
- Must not slow down normal single-tab browsing (layout still only touched when its key changes).

## Open decisions (made as builder's calls; user can override)

1. **Drag tab onto tab to pair** — left out of v1 (non-goal) because the strip's drag currently
   means "reorder" and swaps when passing a neighbour's centre; a reliable "drop to pair" needs a
   separate gesture. Could be added later, e.g. dragging a tab down onto the page area.
2. **Ctrl+Tab / Ctrl+1–9** keep working by tab index; inside a pair, Ctrl+Tab moves to the other
   side first, then on to the next tab.
3. **Ratio is per pair** (each pair remembers its own divider position).
4. **Ctrl+Shift+S on a pinned tab** does nothing (pinned tabs can't be split).
5. **Reopen closed tab** on a tab that was in a pair brings it back as a normal tab (the pair
   isn't rebuilt).

## Left to the builder

- Exact visuals (grip shape, outline colour/alpha, joined-pill drawing) within the existing
  `Palette` and `chrome.rs` style.
- Internal structure (helpers, where `Split` lives), test layout.
- Unit tests for: pane rect maths (ratio + clamping), pair invariant checks on restore, session
  round-trip with splits, dissolve-on-close bookkeeping, insertion index for new tabs after a
  pair, drag-past-pair reorder.
