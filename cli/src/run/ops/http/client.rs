use v8;

use crate::errors::MycoError;
use crate::run::ops::macros::{async_op, get_state};
use crate::run::state::OpResult;
use crate::{register_async_op, request_op, Capability};

#[derive(serde::Deserialize)]
struct EmptyArg;

#[derive(serde::Deserialize)]
struct TokenOptionalPathArg {
    token: String,
    path: Option<String>,
}

pub fn register_http_client_ops(
    scope: &mut v8::ContextScope<v8::HandleScope>,
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
request_op!(async_op_request_fetch_prefix, FetchPrefix);

// Fetch operation
fn async_op_fetch_url(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
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
                Some(Capability::FetchPrefix(base_url)) => {
                    match input.path {
                        Some(path) => {
                            // Security checks to prevent path traversal
                            if path.contains("..") {
                                return Err(MycoError::PathTraversal);
                            }
                            if path.contains("://") {
                                return Err(MycoError::FullUrlInPath);
                            }
                            format!("{}{}", base_url, path)
                        }
                        None => {
                            return Err(MycoError::PathRequiredForPrefix);
                        }
                    }
                }
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
