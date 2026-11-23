//! Network-level ad and tracker blocking: host-suffix matching plus a few YouTube ad paths.

/// Blocked if the request host equals one of these or is a subdomain of one.
const BLOCKED_HOSTS: &[&str] = &[
    // Google ads
    "doubleclick.net", "googlesyndication.com", "googleadservices.com", "googletagservices.com",
    "adservice.google.com", "pagead2.googlesyndication.com", "2mdn.net", "admob.com",
    "google-analytics.com", "analytics.google.com", "googleoptimize.com",
    // Major ad exchanges and networks
    "adnxs.com", "adsrvr.org", "rubiconproject.com", "pubmatic.com", "openx.net", "openx.com",
    "casalemedia.com", "indexww.com", "contextweb.com", "advertising.com", "adform.net",
    "smartadserver.com", "criteo.com", "criteo.net", "taboola.com", "outbrain.com",
    "revcontent.com", "mgid.com", "zedo.com", "yieldmo.com", "sharethrough.com", "triplelift.com",
    "teads.tv", "media.net", "bidswitch.net", "adsymptotic.com", "lijit.com", "sovrn.com",
    "33across.com", "gumgum.com", "districtm.io", "amazon-adsystem.com", "adcolony.com",
    "applovin.com", "unityads.unity3d.com", "ads.yahoo.com", "advertising.yahoo.com",
    "ads.twitter.com", "static.ads-twitter.com", "ads.linkedin.com", "ads.tiktok.com",
    "analytics.tiktok.com", "an.facebook.com", "pixel.facebook.com", "connect.facebook.net",
    "adsafeprotected.com", "moatads.com", "doubleverify.com", "serving-sys.com", "flashtalking.com",
    "bidr.io", "eyeota.net", "tapad.com", "liveramp.com", "rlcdn.com", "agkn.com",
    "adtechus.com", "adtech.de", "yieldlab.net", "inmobi.com", "mopub.com", "popads.net",
    "propellerads.com", "popcash.net", "exoclick.com", "juicyads.com", "trafficjunky.net",
    "adsterra.com", "hilltopads.net", "clickadu.com",
    // Trackers and analytics beacons
    "scorecardresearch.com", "quantserve.com", "quantcount.com", "chartbeat.net", "hotjar.com",
    "mouseflow.com", "crazyegg.com", "fullstory.com", "segment.io", "mixpanel.com", "amplitude.com",
    "branch.io", "adjust.com", "appsflyer.com", "kochava.com", "omtrdc.net", "demdex.net",
    "everesttech.net", "bluekai.com", "krxd.net", "exelator.com", "mathtag.com", "turn.com",
    "owneriq.net", "addthis.com", "sharethis.com", "newrelic.com", "nr-data.net",
];

/// Request paths that are ad endpoints even on hosts we can't block entirely (e.g. youtube.com).
const BLOCKED_PATHS: &[(&str, &str)] = &[
    ("youtube.com", "/api/stats/ads"),
    ("youtube.com", "/pagead/"),
    ("youtube.com", "/ptracking"),
    ("youtube.com", "/get_midroll_info"),
    ("youtube.com", "/youtubei/v1/log_event"),
    ("google.com", "/pagead/"),
    ("google.com", "/adsense/"),
];

fn host_matches(host: &str, suffix: &str) -> bool {
    host == suffix || (host.len() > suffix.len() && host.ends_with(suffix) && host.as_bytes()[host.len() - suffix.len() - 1] == b'.')
}

/// True if a request to `uri` should be dropped.
pub fn is_blocked(uri: &str) -> bool {
    let Ok(u) = url::Url::parse(uri) else { return false };
    if !matches!(u.scheme(), "http" | "https") {
        return false;
    }
    let Some(host) = u.host_str() else { return false };
    let host = host.to_ascii_lowercase();
    if BLOCKED_HOSTS.iter().any(|d| host_matches(&host, d)) {
        return true;
    }
    let path = u.path();
    BLOCKED_PATHS.iter().any(|(h, p)| host_matches(&host, h) && path.starts_with(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_hosts_and_subdomains() {
        assert!(is_blocked("https://doubleclick.net/x"));
        assert!(is_blocked("https://ad.doubleclick.net/x.js"));
        assert!(is_blocked("https://pagead2.googlesyndication.com/pagead/js/adsbygoogle.js"));
    }
    #[test]
    fn does_not_match_lookalikes() {
        assert!(!is_blocked("https://notdoubleclick.net/"));
        assert!(!is_blocked("https://example.com/doubleclick.net"));
    }
    #[test]
    fn youtube_ad_paths_only() {
        assert!(is_blocked("https://www.youtube.com/api/stats/ads?x=1"));
        assert!(!is_blocked("https://www.youtube.com/watch?v=abc"));
        assert!(!is_blocked("https://www.youtube.com/youtubei/v1/player"));
    }
    #[test]
    fn ignores_non_http() {
        assert!(!is_blocked("http://lb.localhost/"));
        assert!(!is_blocked("about:blank"));
    }
}
