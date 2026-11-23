use url::Url;

/// Turn whatever the user typed into a URL to load: a full URL, a bare host, or a search with
/// `engine` (a URL prefix the encoded query is appended to).
pub fn resolve(input: &str, engine: &str) -> String {
    let s = input.trim();
    if s.is_empty() {
        return "about:blank".into();
    }
    if let Ok(u) = Url::parse(s) {
        if matches!(u.scheme(), "http" | "https" | "file" | "about" | "chrome-extension") {
            return u.into();
        }
    }
    if !s.contains(char::is_whitespace) && looks_like_host(s) {
        return format!("https://{s}");
    }
    format!("{engine}{}", encode(s))
}

fn looks_like_host(s: &str) -> bool {
    let host = s.split(['/', '?', '#']).next().unwrap_or(s);
    let host = host.rsplit_once(':').map_or(host, |(h, p)| {
        if p.chars().all(|c| c.is_ascii_digit()) { h } else { host }
    });
    host == "localhost" || (host.contains('.') && !host.starts_with('.') && !host.ends_with('.'))
}

fn encode(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

#[cfg(test)]
mod tests {
    const DDG: &str = "https://duckduckgo.com/?q=";
    fn resolve(s: &str) -> String {
        super::resolve(s, DDG)
    }

    #[test]
    fn full_url() {
        assert_eq!(resolve("https://example.com/a"), "https://example.com/a");
    }
    #[test]
    fn bare_host() {
        assert_eq!(resolve("example.com"), "https://example.com");
        assert_eq!(resolve("localhost:3000/x"), "https://localhost:3000/x");
    }
    #[test]
    fn search() {
        assert_eq!(resolve("rust lang"), "https://duckduckgo.com/?q=rust+lang");
        assert_eq!(resolve("rust"), "https://duckduckgo.com/?q=rust");
    }
    #[test]
    fn extension_pages() {
        assert_eq!(resolve("chrome-extension://abc/popup.html"), "chrome-extension://abc/popup.html");
    }
    #[test]
    fn custom_engine() {
        assert_eq!(super::resolve("a b", "https://www.google.com/search?q="), "https://www.google.com/search?q=a+b");
    }
    #[test]
    fn empty() {
        assert_eq!(resolve("  "), "about:blank");
    }
}
