//! Shared transport for VATUSA API calls.
//!
//! VATUSA authenticates with an `apikey` query parameter, and a `reqwest::Error`
//! prints the request URL, query string included, in both its `Display` and
//! `Debug` output. Every error leaving this module has had its URL stripped, so
//! no caller can log or store the key by logging the error.

use serde::de::DeserializeOwned;

/// Adds the VATUSA API key to `request` and sends it. A send error comes back
/// without its URL.
pub(crate) async fn send_with_api_key(
    request: reqwest::RequestBuilder,
    api_key: &str,
) -> Result<reqwest::Response, reqwest::Error> {
    request
        .query(&[("apikey", api_key)])
        .send()
        .await
        .map_err(reqwest::Error::without_url)
}

/// Decodes a VATUSA response body as JSON. A decode error comes back without
/// the response URL, which carries the API key.
pub(crate) async fn decode_json<T: DeserializeOwned>(
    response: reqwest::Response,
) -> Result<T, reqwest::Error> {
    response
        .json::<T>()
        .await
        .map_err(reqwest::Error::without_url)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error as _;

    const SENTINEL_KEY: &str = "unit-test-sentinel-key-177";

    /// Every rendering a log line could take: `Display`, `Debug`, and each
    /// error in the source chain.
    fn renderings(error: &reqwest::Error) -> Vec<String> {
        let mut out = vec![format!("{error}"), format!("{error:?}")];
        let mut source = error.source();
        while let Some(inner) = source {
            out.push(format!("{inner}"));
            out.push(format!("{inner:?}"));
            source = inner.source();
        }
        out
    }

    fn assert_no_key(error: &reqwest::Error) {
        assert!(error.url().is_none(), "error still carries a URL");
        for rendering in renderings(error) {
            assert!(
                !rendering.contains(SENTINEL_KEY) && !rendering.contains("apikey"),
                "rendered error leaks the key: {rendering}"
            );
        }
    }

    fn client() -> reqwest::Client {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .expect("build test client")
    }

    #[tokio::test]
    async fn send_error_does_not_carry_the_api_key() {
        // Port 1 on loopback refuses the connection, so the send itself fails.
        let error = send_with_api_key(client().get("http://127.0.0.1:1/user/1"), SENTINEL_KEY)
            .await
            .expect_err("send to a closed port fails");

        assert!(
            error.is_connect(),
            "expected a connect error, got {error:?}"
        );
        assert_no_key(&error);
    }

    #[tokio::test]
    async fn decode_error_does_not_carry_the_api_key() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind stub");
        let addr = listener.local_addr().expect("stub address");
        let app = axum::Router::new().route(
            "/user/1",
            axum::routing::get(|| async { "<html>maintenance</html>" }),
        );
        tokio::spawn(async move { axum::serve(listener, app).await.expect("serve stub") });

        let response =
            send_with_api_key(client().get(format!("http://{addr}/user/1")), SENTINEL_KEY)
                .await
                .expect("stub answers");
        // Positive control: the response URL holds the key, so an unstripped
        // decode error would print it.
        assert!(response.url().as_str().contains(SENTINEL_KEY));

        let error = decode_json::<serde_json::Value>(response)
            .await
            .expect_err("non-JSON body fails to decode");

        assert!(error.is_decode(), "expected a decode error, got {error:?}");
        assert_no_key(&error);
    }
}
