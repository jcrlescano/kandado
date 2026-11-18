//! Which websites a login belongs to.
//!
//! This decides where autofill may offer and fill an item, so it errs on the
//! side of not matching. The registrable domain comes from the Public Suffix
//! List, so `evil.github.io` and `good.github.io` are different sites while
//! `accounts.google.com` and `mail.google.com` are the same one.

use serde::{Deserialize, Serialize};
use url::{Host, Url};

/// How strictly a saved website must match the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MatchMode {
    /// The same registrable domain: any subdomain of the same site.
    #[default]
    Domain,
    /// Exactly the same host and port.
    Host,
    /// Never offer this item on this website.
    Never,
}

/// The result of comparing a saved website with a page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UrlMatch {
    /// Do not offer the item.
    No,
    /// Offer the item.
    Yes,
    /// The site matches, but the item was saved for HTTPS and the page is
    /// plain HTTP. Offer it only behind a warning.
    Insecure,
}

/// Compares a website saved on an item with the URL of a page or frame.
pub fn match_url(saved: &str, mode: MatchMode, page: &str) -> UrlMatch {
    if mode == MatchMode::Never {
        return UrlMatch::No;
    }
    let (Some(saved), Some(page)) = (parse(saved), parse(page)) else {
        return UrlMatch::No;
    };
    let same_site = match mode {
        MatchMode::Host => {
            saved.host() == page.host()
                && saved.port_or_known_default() == page.port_or_known_default()
        }
        MatchMode::Domain => site(&saved).is_some_and(|saved| Some(saved) == site(&page)),
        MatchMode::Never => false,
    };
    if !same_site {
        UrlMatch::No
    } else if saved.scheme() == "https" && page.scheme() == "http" {
        UrlMatch::Insecure
    } else {
        UrlMatch::Yes
    }
}

/// The registrable domain of a URL, such as `google.com` for
/// `https://accounts.google.com/signin`. For an IP address or a name with no
/// public suffix (`localhost`, `nas.lan`) it is the whole host.
pub fn registrable_domain(url: &str) -> Option<String> {
    site(&parse(url)?)
}

/// People save "github.com", not "https://github.com/". A website with no
/// scheme is read as HTTPS. Only web pages can match.
fn parse(text: &str) -> Option<Url> {
    let text = text.trim();
    let url = if text.contains("://") {
        Url::parse(text).ok()?
    } else {
        // Without "://", a colon before the path is either a port
        // ("localhost:3000") or another kind of URL ("mailto:…", "javascript:…").
        let authority = text.split(['/', '?', '#']).next().unwrap_or_default();
        if let Some((_, after_colon)) = authority.rsplit_once(':') {
            let is_port =
                !after_colon.is_empty() && after_colon.bytes().all(|b| b.is_ascii_digit());
            let is_ipv6 = authority.starts_with('[') && after_colon.ends_with(']');
            if !is_port && !is_ipv6 {
                return None;
            }
        }
        Url::parse(&format!("https://{text}")).ok()?
    };
    matches!(url.scheme(), "http" | "https").then_some(url)
}

fn site(url: &Url) -> Option<String> {
    match url.host()? {
        Host::Domain(domain) => {
            let domain = domain.trim_end_matches('.');
            match psl::domain_str(domain) {
                // A registrable domain under a known public suffix.
                Some(registrable) if psl::suffix(domain.as_bytes())?.is_known() => {
                    Some(registrable.to_owned())
                }
                // Not on the list (localhost, nas.lan, a bare suffix): the host is the site.
                _ => Some(domain.to_owned()),
            }
        }
        Host::Ipv4(address) => Some(address.to_string()),
        Host::Ipv6(address) => Some(format!("[{address}]")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use MatchMode::{Domain, Host as ExactHost, Never};

    #[test]
    fn domain_mode_matches_any_subdomain_of_the_same_site() {
        for (saved, page) in [
            ("https://github.com/login", "https://github.com/session"),
            ("https://google.com", "https://accounts.google.com/signin"),
            ("https://accounts.google.com", "https://mail.google.com"),
            ("github.com", "https://gist.github.com"),
            ("https://GitHub.com", "https://github.com."),
            ("https://bbc.co.uk", "https://www.bbc.co.uk/news"),
            ("https://example.com:8443", "https://example.com"),
        ] {
            assert_eq!(
                match_url(saved, Domain, page),
                UrlMatch::Yes,
                "{saved} vs {page}"
            );
        }
    }

    #[test]
    fn domain_mode_does_not_match_other_sites() {
        for (saved, page) in [
            ("https://github.com", "https://github.com.evil.example"),
            ("https://github.com", "https://notgithub.com"),
            ("https://github.com", "https://evil.example/github.com"),
            (
                "https://github.com",
                "https://evil.example/?next=https://github.com",
            ),
            ("https://github.com", "https://github.com@evil.example"),
            ("https://bbc.co.uk", "https://other.co.uk"),
            // Sites that share a public suffix belong to different people.
            ("https://alice.github.io", "https://mallory.github.io"),
            ("https://one.blogspot.com", "https://two.blogspot.com"),
            ("https://xn--pypal-4ve.com", "https://paypal.com"),
        ] {
            assert_eq!(
                match_url(saved, Domain, page),
                UrlMatch::No,
                "{saved} vs {page}"
            );
        }
    }

    #[test]
    fn host_mode_needs_the_same_host_and_port() {
        assert_eq!(
            match_url(
                "https://app.example.com",
                ExactHost,
                "https://app.example.com/x"
            ),
            UrlMatch::Yes
        );
        assert_eq!(
            match_url(
                "https://app.example.com:443",
                ExactHost,
                "https://app.example.com"
            ),
            UrlMatch::Yes
        );
        for page in [
            "https://example.com",
            "https://www.app.example.com",
            "https://app.example.com:8443",
        ] {
            assert_eq!(
                match_url("https://app.example.com", ExactHost, page),
                UrlMatch::No,
                "{page}"
            );
        }
    }

    #[test]
    fn never_mode_matches_nothing() {
        assert_eq!(
            match_url("https://github.com", Never, "https://github.com"),
            UrlMatch::No
        );
    }

    #[test]
    fn an_https_login_on_an_http_page_needs_a_warning() {
        assert_eq!(
            match_url("https://example.com", Domain, "http://example.com/login"),
            UrlMatch::Insecure
        );
        assert_eq!(
            match_url("example.com", Domain, "http://example.com"),
            UrlMatch::Insecure
        );
        // Saved for HTTP: the person already accepted it. Upgrades are always fine.
        assert_eq!(
            match_url(
                "http://router.example.com",
                Domain,
                "http://router.example.com"
            ),
            UrlMatch::Yes
        );
        assert_eq!(
            match_url("http://example.com", Domain, "https://example.com"),
            UrlMatch::Yes
        );
    }

    #[test]
    fn local_names_and_addresses_match_only_themselves() {
        assert_eq!(
            match_url("http://localhost:3000", Domain, "http://localhost:8080"),
            UrlMatch::Yes
        );
        assert_eq!(
            match_url("http://192.168.1.1", Domain, "http://192.168.1.1/admin"),
            UrlMatch::Yes
        );
        assert_eq!(
            match_url("http://nas.lan", Domain, "http://nas.lan"),
            UrlMatch::Yes
        );
        for (saved, page) in [
            ("http://192.168.1.1", "http://192.168.1.2"),
            ("http://nas.lan", "http://printer.lan"),
            ("http://localhost", "http://localhost.evil.example"),
            ("http://[::1]", "http://[::2]"),
        ] {
            assert_eq!(
                match_url(saved, Domain, page),
                UrlMatch::No,
                "{saved} vs {page}"
            );
        }
    }

    #[test]
    fn only_web_pages_can_match() {
        for page in [
            "",
            "not a url",
            "file:///C:/github.com",
            "chrome-extension://abcdef/popup.html",
            "javascript:alert(1)",
            "data:text/html,github.com",
            "about:blank",
        ] {
            assert_eq!(
                match_url("https://github.com", Domain, page),
                UrlMatch::No,
                "{page}"
            );
        }
        assert_eq!(match_url("", Domain, "https://github.com"), UrlMatch::No);
    }

    #[test]
    fn registrable_domains() {
        for (url, expected) in [
            ("https://accounts.google.com/signin", "google.com"),
            ("www.bbc.co.uk", "bbc.co.uk"),
            ("https://alice.github.io", "alice.github.io"),
            ("http://localhost:3000", "localhost"),
            ("http://192.168.1.1", "192.168.1.1"),
        ] {
            assert_eq!(registrable_domain(url).as_deref(), Some(expected), "{url}");
        }
        assert_eq!(registrable_domain("mailto:juan@example.com"), None);
    }
}
