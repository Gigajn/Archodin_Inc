//! OpenRouter attribution (ADR 0027): the two headers that identify
//! Basileia to OpenRouter on every chat completion — `HTTP-Referer`
//! (the repo URL, so OpenRouter can show which app the traffic comes
//! from) and `X-OpenRouter-Title` (the app name as OpenRouter displays
//! it). The values are NOT secrets, so they may appear in `Debug` and
//! in error text; they are constants in v1 (no env override) and they
//! are sent ONLY for kind `openrouter` — never for `openai_compatible`
//! or any other kind, and never on a health check (ADR 0027 §D4).

use reqwest::header::{HeaderName, HeaderValue};
use reqwest::Url;

/// The referer every `openrouter` Provider sends: the repo URL, so
/// OpenRouter attributes the traffic to Basileia's public home.
const DEFAULT_REFERER: &str = "https://github.com/Gigajn/Archodin_Inc";

/// The title every `openrouter` Provider sends: the app's modern name.
const DEFAULT_TITLE: &str = "Basileia";

/// Why an [`Attribution`] could not be built. The message names the
/// FIELD (`referer` / `title`), never the value that was refused —
/// even though neither value is a secret, the shape matches the
/// codebase rule that validation errors name fields, not inputs.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AttributionError {
    /// The referer was empty.
    #[error("attribution referer must not be empty")]
    EmptyReferer,
    /// The referer did not parse as an `http(s)` URL.
    #[error("attribution referer must be an http(s) URL")]
    BadReferer,
    /// The title was empty or whitespace-only.
    #[error("attribution title must not be empty")]
    EmptyTitle,
    /// The title contains a character a header value cannot carry
    /// (for example a control character).
    #[error("attribution title contains a character a header value cannot carry")]
    BadTitle,
}

/// The OpenRouter attribution of one Provider: the `HTTP-Referer` and
/// `X-OpenRouter-Title` headers its chat completions carry.
///
/// This struct holds no key, so deriving `Debug` is safe — its values
/// are public constants in v1, not secrets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attribution {
    referer: String,
    title: String,
}

impl Default for Attribution {
    /// The v1 constants (ADR 0027 §D2): the repo URL as referer and
    /// `Basileia` as title.
    fn default() -> Self {
        Self::new(DEFAULT_REFERER, DEFAULT_TITLE)
            .expect("the built-in attribution values are valid http(s) URL + title")
    }
}

impl Attribution {
    /// Builds an attribution, refusing a referer that is empty or not
    /// an `http(s)` URL, and a title that is empty, whitespace-only,
    /// or contains a character a header value cannot carry.
    pub fn new(
        referer: impl Into<String>,
        title: impl Into<String>,
    ) -> Result<Self, AttributionError> {
        let referer = referer.into();
        let title = title.into();
        if referer.is_empty() {
            return Err(AttributionError::EmptyReferer);
        }
        let url = Url::parse(&referer).map_err(|_| AttributionError::BadReferer)?;
        if url.scheme() != "http" && url.scheme() != "https" {
            return Err(AttributionError::BadReferer);
        }
        if url.host_str().is_none_or(|host| host.is_empty()) {
            return Err(AttributionError::BadReferer);
        }
        // `HeaderValue::from_str` refuses control characters, so a
        // referer that parsed as a URL but cannot travel as a header
        // is still refused here (a URL with a control character does
        // not parse, so this is belt-and-braces, not the main gate).
        HeaderValue::from_str(&referer).map_err(|_| AttributionError::BadReferer)?;
        if title.trim().is_empty() {
            return Err(AttributionError::EmptyTitle);
        }
        HeaderValue::from_str(&title).map_err(|_| AttributionError::BadTitle)?;
        Ok(Self { referer, title })
    }

    /// The two headers, in the order OpenRouter documents them:
    /// `HTTP-Referer` then `X-OpenRouter-Title`. `X-Title` (the old
    /// alias) is deliberately NOT sent (ADR 0027 §D2).
    ///
    /// Infallible by construction: [`Attribution::new`] validated both
    /// values through `HeaderValue::from_str`, and `new` / `default`
    /// are the only constructors.
    pub fn headers(&self) -> [(HeaderName, HeaderValue); 2] {
        let referer = HeaderValue::from_str(&self.referer)
            .expect("Attribution::new accepts only header-safe referers");
        let title = HeaderValue::from_str(&self.title)
            .expect("Attribution::new accepts only header-safe titles");
        [
            (HeaderName::from_static("http-referer"), referer),
            (HeaderName::from_static("x-openrouter-title"), title),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_attribution_is_the_repo_url_and_basileia() {
        // Independent source of truth (ADR 0027 §D2): the literals
        // below come from the decision, not from the code under test.
        let headers = Attribution::default().headers();
        assert_eq!(headers.len(), 2);
        let referer = headers
            .iter()
            .find(|(name, _)| name.as_str() == "http-referer")
            .expect("an http-referer header");
        assert_eq!(
            referer.1.to_str().unwrap(),
            "https://github.com/Gigajn/Archodin_Inc"
        );
        let title = headers
            .iter()
            .find(|(name, _)| name.as_str() == "x-openrouter-title")
            .expect("an x-openrouter-title header");
        assert_eq!(title.1.to_str().unwrap(), "Basileia");
    }

    #[test]
    fn new_accepts_a_well_formed_pair() {
        let attribution = Attribution::new("https://example.com/app", "Example").unwrap();
        let headers = attribution.headers();
        let referer = headers
            .iter()
            .find(|(name, _)| name.as_str() == "http-referer")
            .expect("an http-referer header");
        assert_eq!(referer.1.to_str().unwrap(), "https://example.com/app");
        let title = headers
            .iter()
            .find(|(name, _)| name.as_str() == "x-openrouter-title")
            .expect("an x-openrouter-title header");
        assert_eq!(title.1.to_str().unwrap(), "Example");
    }

    #[test]
    fn new_rejects_a_referer_that_is_not_an_http_url() {
        // Not a URL at all, the wrong scheme, and empty: each error
        // names the FIELD (`referer`) and never echoes the bad value.
        for bad in [
            "not a url",
            "ftp://example.com/app",
            "HTTPS://",
            "http://",
            "",
        ] {
            let err = Attribution::new(bad, "Basileia").unwrap_err();
            assert!(err.to_string().contains("referer"), "{bad}: {err}");
            // An empty value has nothing to echo; a non-empty one must
            // never appear in the error.
            if !bad.is_empty() {
                assert!(!err.to_string().contains(bad), "{bad}: {err}");
            }
        }
    }

    #[test]
    fn new_rejects_an_empty_or_blank_title() {
        for bad in ["", "   "] {
            let err = Attribution::new("https://github.com/Gigajn/Archodin_Inc", bad).unwrap_err();
            assert!(err.to_string().contains("title"), "{bad:?}: {err}");
        }
    }

    #[test]
    fn new_rejects_a_title_with_a_control_character() {
        // A header value cannot carry a control character
        // (`HeaderValue::from_str` refuses it): the error names the
        // field, never the value.
        let err =
            Attribution::new("https://github.com/Gigajn/Archodin_Inc", "Ba\nsileia").unwrap_err();
        assert!(err.to_string().contains("title"), "{err}");
        assert!(!err.to_string().contains("Ba\nsileia"), "{err}");
    }
}
