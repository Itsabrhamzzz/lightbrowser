//! Address-bar suggestions from history and bookmarks.

use serde::Serialize;

use crate::store::{Bookmark, HistoryEntry};

#[derive(Serialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// What Enter does with the typed text: a web search…
    Search,
    /// …or going straight to an address.
    Url,
    History,
    Bookmark,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Suggestion {
    pub title: String,
    pub url: String,
    pub kind: Kind,
}

/// At most this many rows, including the first (search / go-to) row.
pub const MAX: usize = 8;

/// `https://www.example.com/a` → `example.com/a`, for matching what people type.
fn bare(url: &str) -> &str {
    let u = url.split_once("://").map_or(url, |(_, rest)| rest);
    u.strip_prefix("www.").unwrap_or(u)
}

/// How well `q` (lower-case, trimmed) matches; `None` = not at all.
fn match_score(q: &str, title: &str, url: &str) -> Option<f64> {
    let (title, u) = (title.to_lowercase(), bare(url).to_lowercase());
    if u.starts_with(q) {
        return Some(100.0);
    }
    if title.split(|c: char| !c.is_alphanumeric()).any(|w| w.starts_with(q)) {
        return Some(45.0);
    }
    if u.contains(q) || title.contains(q) {
        return Some(30.0);
    }
    let hay = format!("{title} {u}");
    let terms: Vec<&str> = q.split_whitespace().collect();
    (terms.len() > 1 && terms.iter().all(|t| hay.contains(t))).then_some(20.0)
}

fn is_web(url: &str) -> bool {
    (url.starts_with("http://") || url.starts_with("https://")) && !url.starts_with("http://lb.localhost")
}

/// Suggestions for what's typed in the address bar. The first row is always what Enter would do.
pub fn suggest(input: &str, history: &[HistoryEntry], bookmarks: &[Bookmark], engine: &str, now: u64) -> Vec<Suggestion> {
    let typed = input.trim();
    if typed.is_empty() {
        return vec![];
    }
    let target = crate::url_input::resolve(typed, engine);
    let first = if target.starts_with(engine) {
        Suggestion { title: typed.into(), url: target, kind: Kind::Search }
    } else {
        Suggestion { title: typed.into(), url: target, kind: Kind::Url }
    };

    let q = typed.to_lowercase();
    let mut scored: Vec<(f64, Suggestion)> = Vec::new();
    for b in bookmarks.iter().filter(|b| is_web(&b.url)) {
        if let Some(s) = match_score(&q, &b.title, &b.url) {
            let visits = history.iter().find(|h| h.url == b.url).map_or(0, |h| h.visits);
            let score = s + 25.0 + (visits as f64 + 1.0).ln() * 10.0;
            scored.push((score, Suggestion { title: b.title.clone(), url: b.url.clone(), kind: Kind::Bookmark }));
        }
    }
    for h in history.iter().filter(|h| is_web(&h.url)) {
        if bookmarks.iter().any(|b| b.url == h.url) {
            continue; // already listed as a bookmark
        }
        if let Some(s) = match_score(&q, &h.title, &h.url) {
            let age = now.saturating_sub(h.last);
            let recency = match age {
                a if a < 86_400 => 15.0,
                a if a < 7 * 86_400 => 8.0,
                a if a < 30 * 86_400 => 3.0,
                _ => 0.0,
            };
            let score = s + (h.visits as f64 + 1.0).ln() * 10.0 + recency;
            scored.push((score, Suggestion { title: h.title.clone(), url: h.url.clone(), kind: Kind::History }));
        }
    }
    scored.sort_by(|a, b| b.0.total_cmp(&a.0));

    let mut out = vec![first];
    for (_, s) in scored {
        if out.len() >= MAX {
            break;
        }
        if !out.iter().any(|o| o.url == s.url) {
            out.push(s);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENGINE: &str = "https://duckduckgo.com/?q=";

    fn h(url: &str, title: &str, visits: u32, last: u64) -> HistoryEntry {
        HistoryEntry { url: url.into(), title: title.into(), visits, last }
    }

    #[test]
    fn first_row_is_what_enter_does() {
        let s = suggest("rust lang", &[], &[], ENGINE, 0);
        assert_eq!(s.len(), 1);
        assert_eq!((s[0].kind, s[0].url.as_str()), (Kind::Search, "https://duckduckgo.com/?q=rust+lang"));
        let s = suggest("example.com", &[], &[], ENGINE, 0);
        assert_eq!((s[0].kind, s[0].url.as_str()), (Kind::Url, "https://example.com"));
        assert!(suggest("   ", &[], &[], ENGINE, 0).is_empty());
    }

    #[test]
    fn address_prefix_beats_substring() {
        let hist = [h("https://www.github.com/", "GitHub", 1, 0), h("https://example.com/git-guide", "A guide", 1, 0)];
        let s = suggest("git", &hist, &[], ENGINE, 0);
        assert_eq!(s[1].url, "https://www.github.com/");
        assert_eq!(s[2].url, "https://example.com/git-guide");
    }

    #[test]
    fn title_words_match_and_bookmarks_rank_higher() {
        let hist = [h("https://a.com/", "Rust news", 3, 0)];
        let bms = [Bookmark { title: "The Rust Book".into(), url: "https://doc.rust-lang.org/book/".into() }];
        // Visited long ago, so no recency bonus.
        let s = suggest("rust", &hist, &bms, ENGINE, 100 * 86_400);
        assert_eq!(s[1].kind, Kind::Bookmark);
        assert_eq!(s[2].url, "https://a.com/");
    }

    #[test]
    fn dedupes_skips_internal_and_caps() {
        let mut hist: Vec<_> = (0..20).map(|i| h(&format!("https://site{i}.com/"), "Site", 1, 0)).collect();
        hist.push(h("http://lb.localhost/history", "Site history", 9, 0));
        let bms = [Bookmark { title: "Site".into(), url: "https://site0.com/".into() }];
        let s = suggest("site", &hist, &bms, ENGINE, 0);
        assert_eq!(s.len(), MAX);
        assert_eq!(s.iter().filter(|x| x.url == "https://site0.com/").count(), 1);
        assert!(!s.iter().any(|x| x.url.starts_with("http://lb.localhost")));
    }

    #[test]
    fn recent_and_frequent_rank_higher() {
        let now = 100 * 86_400;
        let hist = [h("https://news.old.com/", "News", 1, 0), h("https://news.new.com/", "News", 1, now - 60)];
        let s = suggest("news", &hist, &[], ENGINE, now);
        assert_eq!(s[1].url, "https://news.new.com/");
    }
}
