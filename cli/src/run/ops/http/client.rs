use url::Url;
use v8;

use crate::errors::MycoError;
use crate::run::ops::macros::{async_op, get_state, get_string_arg, throw_js_error};
use crate::run::state::OpResult;
use crate::{impl_from_v8_struct, register_async_op, request_op, Capability};

struct TokenOptionalPathArg {
    token: String,
    path: Option<String>,
}

impl_from_v8_struct!(TokenOptionalPathArg {
    token: String,
    path: Option<String>,
});

pub fn register_http_client_ops(
    scope: &mut v8::PinScope<'_, '_>,
    myco_ops: &v8::Object,
) -> Result<(), MycoError> {
    register_async_op!(
        scope,
        myco_ops,
        "request_fetch_url",
        async_op_request_fetch_url
    );
    register_async_op!(
        scope,
        myco_ops,
        "request_fetch_prefix",
        async_op_request_fetch_prefix
    );
    register_async_op!(scope, myco_ops, "fetch_url", async_op_fetch_url);

    Ok(())
}

// Token request operations
request_op!(async_op_request_fetch_url, FetchUrl);

/// Parse the prefix at issuance so grants and requests use identical semantics.
fn async_op_request_fetch_prefix<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue,
) {
    let prefix = match get_string_arg(scope, &args, 0, "url") {
        Ok(prefix) => prefix,
        Err(_) => return,
    };

    let parsed = match Url::parse(&prefix) {
        Ok(parsed) => parsed,
        Err(_) => {
            throw_js_error(scope, &format!("{}", MycoError::InvalidUrl { url: prefix }));
            return;
        }
    };

    // Appending to a query or fragment would not extend the URL's path.
    if parsed.query().is_some() || parsed.fragment().is_some() {
        throw_js_error(
            scope,
            &format!("{}", MycoError::PrefixWithQueryOrFragment { url: prefix }),
        );
        return;
    }

    match get_state(scope) {
        Ok(state) => {
            let token = state.capabilities.register(Capability::FetchPrefix(parsed));
            let token_string = v8::String::new(scope, &token).unwrap();
            rv.set(token_string.into());
        }
        Err(e) => throw_js_error(scope, &format!("Failed to get state: {}", e)),
    }
}

/// Append using the public API's concatenation semantics, then check the parsed URL.
fn confine_to_prefix(prefix: &Url, path: &str) -> Result<Url, MycoError> {
    let candidate = format!("{}{}", prefix, path);
    let joined = Url::parse(&candidate).map_err(|_| MycoError::InvalidUrl {
        url: candidate.clone(),
    })?;
    check_prefix(prefix, &joined).map_err(|error| match error {
        MycoError::UrlOutsidePrefix { .. } => MycoError::UrlOutsidePrefix { url: candidate },
        error => error,
    })?;
    Ok(joined)
}

/// Used for both the initial request and every redirect destination.
fn check_prefix(prefix: &Url, url: &Url) -> Result<(), MycoError> {
    let outside = || MycoError::UrlOutsidePrefix {
        url: url.to_string(),
    };
    // Opaque origins do not compare equal even for identical non-special URLs.
    let same_authority = url.scheme() == prefix.scheme()
        && url.host() == prefix.host()
        && url.port_or_known_default() == prefix.port_or_known_default()
        && url.username() == prefix.username()
        && url.password() == prefix.password();
    if !same_authority {
        return Err(outside());
    }

    // Servers may decode separators before normalizing. Inspect the parsed path:
    // URL parsing strips TAB/LF/CR, and encoded URLs in queries are harmless here.
    let base = prefix.path();
    let target = url.path();
    let lower = target.to_ascii_lowercase();
    if lower.contains("%2f") || lower.contains("%5c") {
        return Err(MycoError::EncodedSeparatorInPath {
            path: target.to_string(),
        });
    }
    let within = target == base
        || (target.starts_with(base)
            && (base.ends_with('/') || target.as_bytes().get(base.len()) == Some(&b'/')));
    if !within {
        return Err(outside());
    }

    Ok(())
}

fn fetch_client(url: Url, prefix: Option<Url>) -> Result<reqwest::Client, reqwest::Error> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::custom(move |attempt| {
            let allowed = match &prefix {
                Some(prefix) => check_prefix(prefix, attempt.url()),
                None if attempt.url() == &url => Ok(()),
                None => Err(MycoError::UrlOutsideGrant {
                    url: attempt.url().to_string(),
                }),
            };
            if let Err(error) = allowed {
                return attempt.error(error);
            }
            if attempt.previous().len() >= 10 {
                return attempt.error("too many redirects");
            }
            attempt.follow()
        }))
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fetch(prefix: &str, path: &str) -> Result<String, MycoError> {
        let prefix = Url::parse(prefix).expect("a valid prefix");
        confine_to_prefix(&prefix, path).map(|url| url.to_string())
    }

    fn is_outside(prefix: &str, path: &str) -> bool {
        matches!(fetch(prefix, path), Err(MycoError::UrlOutsidePrefix { .. }))
    }

    #[test]
    fn admits_paths_under_the_prefix() {
        assert_eq!(
            fetch("https://api.example.com/public/", "file.json").unwrap(),
            "https://api.example.com/public/file.json"
        );
        assert_eq!(
            fetch("https://api.example.com/public/", "deep/file.json").unwrap(),
            "https://api.example.com/public/deep/file.json"
        );
        // A query is not part of the path, so one may contain anything.
        assert_eq!(
            fetch("https://api.example.com/public/", "s?to=https://x.test").unwrap(),
            "https://api.example.com/public/s?to=https://x.test"
        );
        // `..` that lands back inside the prefix is applied, not refused.
        assert_eq!(
            fetch("https://api.example.com/public/", "deep/../file.json").unwrap(),
            "https://api.example.com/public/file.json"
        );
    }

    #[test]
    fn refuses_every_way_out_of_the_prefix() {
        // Encoded dot segments: `url` pops a segment for each, and no substring
        // test for ".." sees them.
        assert!(is_outside(
            "https://api.example.com/public/",
            "%2e%2e/%2e%2e/admin"
        ));
        assert!(is_outside(
            "https://api.example.com/public/",
            "%2E%2E/secret"
        ));
        assert!(is_outside("https://api.example.com/public/", ".%2e/secret"));
        // Literal ones.
        assert!(is_outside("https://api.example.com/public/", "../secret"));
        // Reaching a sibling of the prefix is out, however it is spelled.
        assert!(is_outside(
            "https://api.example.com/public/",
            "../private/x"
        ));
    }

    // A prefix naming only an authority serializes with a trailing slash, so a
    // leading `@` has nowhere to land and cannot move the host into userinfo.
    #[test]
    fn a_prefix_without_a_path_still_pins_the_host() {
        assert_eq!(
            fetch("https://api.example.com", "@evil.test/x").unwrap(),
            "https://api.example.com/@evil.test/x"
        );
    }

    #[test]
    fn a_url_shaped_path_lands_under_the_prefix() {
        assert_eq!(
            fetch("https://api.example.com/public/", "https://evil.test/x").unwrap(),
            "https://api.example.com/public/https://evil.test/x"
        );
        assert_eq!(
            fetch("https://api.example.com/public/", "file:///etc/passwd").unwrap(),
            "https://api.example.com/public/file:///etc/passwd"
        );
        assert_eq!(
            fetch("https://api.example.com/", "//evil.test/x").unwrap(),
            "https://api.example.com///evil.test/x"
        );
    }

    // Non-special schemes do not insert a slash after the authority.
    #[test]
    fn the_authority_is_part_of_the_grant() {
        // Without the `/` a special scheme would have added, the `@` reaches
        // the authority and moves the host into userinfo.
        assert!(is_outside("myco://api.example.test", "@evil.test/x"));
        // What stays on the granted host is still admitted.
        assert_eq!(
            fetch("myco://api.example.test", "/x").unwrap(),
            "myco://api.example.test/x"
        );
    }

    #[test]
    fn encoded_separators_are_refused() {
        let encoded = [
            "..%2f..%2fadmin",
            "..%2F..%2Fadmin",
            "a%2fb",
            "..%5cadmin",
            "..%5Cadmin",
            // Control characters are stripped by URL parsing.
            "..%\t2f..%\t2fadmin",
            "..%\n2f..%\n2fadmin",
            "..%2\rf..%2\rfadmin",
            "..%\t5cadmin",
        ];
        for path in encoded {
            assert!(
                matches!(
                    fetch("https://api.example.com/public/", path),
                    Err(MycoError::EncodedSeparatorInPath { .. })
                ),
                "{path:?}"
            );
        }

        // A real slash is how a caller reaches a subdirectory.
        assert!(fetch("https://api.example.com/public/", "a/b").is_ok());
        // Encoded separators in queries do not change the path.
        assert_eq!(
            fetch(
                "https://api.example.com/public/",
                "cb?redirect=https%3A%2F%2Fx.test%2Fy"
            )
            .unwrap(),
            "https://api.example.com/public/cb?redirect=https%3A%2F%2Fx.test%2Fy"
        );
    }

    #[test]
    fn a_prefix_is_not_a_string_prefix() {
        assert!(is_outside("https://api.example.com/public", "-secrets/x"));
        assert_eq!(
            fetch("https://api.example.com/public", "/x").unwrap(),
            "https://api.example.com/public/x"
        );
    }

    async fn redirects(locations: &[&str], prefix_grant: bool, succeeds: bool) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;
        use tokio::time::{timeout, Duration};

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let outside = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let url = Url::parse(&format!("{origin}/public/start")).unwrap();
        let prefix = prefix_grant.then(|| Url::parse(&format!("{origin}/public/")).unwrap());
        let locations: Vec<String> = locations
            .iter()
            .map(|location| {
                if *location == "cross-origin" {
                    format!("http://{}/public/secret", outside.local_addr().unwrap())
                } else {
                    location.to_string()
                }
            })
            .collect();
        let server = tokio::spawn(async move {
            let mut replies: Vec<String> = locations.iter().map(|location| {
                format!("HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            }).collect();
            if succeeds {
                replies.push(
                    "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok".into(),
                );
            }
            for reply in replies {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                let mut byte = [0];
                while !request.ends_with(b"\r\n\r\n") {
                    assert_eq!(stream.read(&mut byte).await.unwrap(), 1);
                    request.push(byte[0]);
                }
                stream.write_all(reply.as_bytes()).await.unwrap();
            }
            listener
        });
        let result = timeout(
            Duration::from_secs(5),
            fetch_client(url.clone(), prefix).unwrap().get(url).send(),
        )
        .await
        .unwrap();
        if succeeds {
            assert_eq!(result.unwrap().text().await.unwrap(), "ok");
        } else {
            assert!(result.unwrap_err().is_redirect());
        }
        // No connection may be made to a refused redirect, even on the same host.
        for listener in [server.await.unwrap(), outside] {
            assert_eq!(
                listener.into_std().unwrap().accept().unwrap_err().kind(),
                std::io::ErrorKind::WouldBlock
            );
        }
    }

    #[tokio::test]
    async fn follows_redirects_within_the_prefix() {
        redirects(&["/public/next"], true, true).await;
    }

    #[tokio::test]
    async fn refuses_redirects_outside_the_grant_before_connecting() {
        for location in ["/admin", "cross-origin", "/public/a%2fb", "/public/a%5cb"] {
            redirects(&["/public/next", location], true, false).await;
        }
        redirects(&["/public/next"], false, false).await;
    }
}

// Fetch operation
fn async_op_fetch_url<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    rv: v8::ReturnValue,
) {
    async_op(
        scope,
        rv,
        &args,
        |scope, input: TokenOptionalPathArg| {
            let state = get_state(scope)?;
            let (url, prefix) = match state.capabilities.get(&input.token) {
                Some(Capability::FetchUrl(allowed_url)) => {
                    if input.path.is_some() {
                        return Err(MycoError::PathNotAllowedForSpecificUrlTokens);
                    }
                    (Url::parse(allowed_url), None)
                }
                Some(Capability::FetchPrefix(base_url)) => match input.path {
                    Some(path) => (
                        Ok(confine_to_prefix(base_url, &path)?),
                        Some(base_url.clone()),
                    ),
                    None => {
                        return Err(MycoError::PathRequiredForPrefix);
                    }
                },
                _ => {
                    return Err(MycoError::InvalidTokenForUrlAccess);
                }
            };
            Ok((url, prefix))
        },
        |(url, prefix)| async move {
            let result = async move {
                let url = url.map_err(|e| format!("HTTP request failed: builder error: {e}"))?;
                let client = fetch_client(url.clone(), prefix)
                    .map_err(|e| format!("Failed to create HTTP client: {e}"))?;
                let response = client
                    .get(url)
                    .send()
                    .await
                    .map_err(|e| format!("HTTP request failed: {}", e))?;

                let bytes = response
                    .bytes()
                    .await
                    .map_err(|e| format!("Failed to read response body: {}", e))?;

                Ok::<Vec<u8>, String>(bytes.to_vec())
            }
            .await;

            OpResult::Binary(result)
        },
    );
}
