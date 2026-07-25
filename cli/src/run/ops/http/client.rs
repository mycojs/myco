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

/// Unlike `FetchUrl`, a prefix grant is parsed here rather than kept as the
/// string the caller typed. What the grant covers is then decided by the same
/// parser that will be used to act on it -- see `confine_to_prefix`.
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

    // A path is appended to the prefix, so anything after the path in the
    // prefix swallows it: with `?k=v`, every path a caller passes lands in the
    // query and the request always fetches the prefix itself. Refused here
    // rather than at the call, where it looks like the path is at fault.
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

/// Confine a caller-supplied path to the prefix its token was issued for.
///
/// The check has to be made on parsed URLs, because the string form disagrees
/// with what will actually be requested. `url` treats `%2e%2e` as a double-dot
/// segment and pops the one before it, which no substring test for `..` can
/// see, and a prefix that ends at the authority lets a leading `@` turn the
/// granted host into userinfo and move the request to another host entirely.
/// Both were reachable from an ordinary `FetchPrefixToken` (#64).
fn confine_to_prefix(prefix: &Url, path: &str) -> Result<Url, MycoError> {
    // An encoded slash survives parsing, so the check here and the server can
    // disagree about where the segments are: `..%2f..%2fadmin` stays one
    // segment to `url` and passes the boundary check below, while a server that
    // decodes before normalizing reads `/admin`. That is a known way to walk
    // out of a path prefix, and which servers do it is not something a token
    // holder should have to be right about, so it is refused here rather than
    // relied upon not to matter. The cost is a path that legitimately needs an
    // encoded slash inside a segment, which has to be spelled some other way.
    if path.to_ascii_lowercase().contains("%2f") {
        return Err(MycoError::EncodedSlashInPath {
            path: path.to_string(),
        });
    }

    let candidate = format!("{}{}", prefix, path);
    let outside = || MycoError::UrlOutsidePrefix {
        url: candidate.clone(),
    };

    // Not `outside`: a path that will not parse is not a path shown to leave
    // the prefix, and saying so sends whoever reads it looking for the wrong
    // thing. Defensive rather than reachable -- the prefix parsed when it was
    // granted, and appending to a parsed absolute URL has not been found to
    // break it (brackets, spaces, NULs, bad percent escapes and 100k-character
    // paths all still parse). It costs nothing to name correctly.
    let joined = Url::parse(&candidate).map_err(|_| MycoError::InvalidUrl {
        url: candidate.clone(),
    })?;

    // Compared piecewise rather than through `Origin`, which is opaque and
    // never equal to itself for non-special schemes.
    let same_authority = joined.scheme() == prefix.scheme()
        && joined.host() == prefix.host()
        && joined.port_or_known_default() == prefix.port_or_known_default()
        && joined.username() == prefix.username()
        && joined.password() == prefix.password();
    if !same_authority {
        return Err(outside());
    }

    // At a segment boundary, so `/public` does not admit `/public-secrets`.
    let base = prefix.path();
    let target = joined.path();
    let within = target == base
        || (target.starts_with(base)
            && (base.ends_with('/') || target.as_bytes().get(base.len()) == Some(&b'/')));
    if !within {
        return Err(outside());
    }

    Ok(joined)
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

    // The two shapes #64 reports, plus the literal form that was already caught.
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

    // A path that looks like a URL is not a way out: the request still goes to
    // the granted host, under the granted prefix. The `://` rejection this
    // replaces was a heuristic standing in for the check above, and it refused
    // paths that were never escapes -- a redirect parameter, say.
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

    // For a special scheme the authority is always closed off by a `/` before
    // any path is appended, so the authority check only fires for schemes that
    // are not normalized that way. It is the part of the check that stops a
    // path reaching another host, so it is worth one test that reaches it.
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

    // An encoded slash is where the check here and the server can disagree
    // about how many segments there are.
    #[test]
    fn an_encoded_slash_is_refused() {
        for path in ["..%2f..%2fadmin", "..%2F..%2Fadmin", "a%2fb"] {
            assert!(
                matches!(
                    fetch("https://api.example.com/public/", path),
                    Err(MycoError::EncodedSlashInPath { .. })
                ),
                "{path}"
            );
        }
        // A real slash is how a caller reaches a subdirectory, and is fine.
        assert!(fetch("https://api.example.com/public/", "a/b").is_ok());
    }

    // Prefixes are compared at a segment boundary, so a longer sibling name is
    // not inside a shorter one.
    #[test]
    fn a_prefix_is_not_a_string_prefix() {
        assert!(is_outside("https://api.example.com/public", "-secrets/x"));
        assert_eq!(
            fetch("https://api.example.com/public", "/x").unwrap(),
            "https://api.example.com/public/x"
        );
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
            let url = match state.capabilities.get(&input.token) {
                Some(Capability::FetchUrl(allowed_url)) => {
                    if input.path.is_some() {
                        return Err(MycoError::PathNotAllowedForSpecificUrlTokens);
                    }
                    allowed_url.clone()
                }
                Some(Capability::FetchPrefix(base_url)) => match input.path {
                    Some(path) => confine_to_prefix(base_url, &path)?.to_string(),
                    None => {
                        return Err(MycoError::PathRequiredForPrefix);
                    }
                },
                _ => {
                    return Err(MycoError::InvalidTokenForUrlAccess);
                }
            };
            Ok(url)
        },
        |url| async move {
            let result = async move {
                let client = reqwest::Client::new();
                let response = client
                    .get(&url)
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
