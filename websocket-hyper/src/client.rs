use lazy_static::lazy_static;
use tokio::prelude::*;

#[derive(Debug)]
pub enum HttpUpgradeError {
    SwitchingProtocolsNotSupported(hyper::StatusCode),
    NoAcceptHeader,
    WrongAcceptHeader,
    UpgradeFailed(hyper::Error),
}

impl std::fmt::Display for HttpUpgradeError {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
        use std::error::Error;

        fmt.write_str("HttpUpgradeError: ")?;
        fmt.write_str(self.description())?;
        Ok(())
    }
}

impl std::error::Error for HttpUpgradeError {
    fn description(&self) -> &str {
        match *self {
            HttpUpgradeError::SwitchingProtocolsNotSupported(_) => {
                "Switching protocols not supported"
            }
            HttpUpgradeError::NoAcceptHeader => "No Accept header",
            HttpUpgradeError::WrongAcceptHeader => "Wrong Accept header",
            HttpUpgradeError::UpgradeFailed(_) => "Upgrade failed",
        }
    }

    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match *self {
            HttpUpgradeError::UpgradeFailed(ref error) => Some(error),
            _ => None,
        }
    }
}

pub async fn connect<U>(
    uri: U,
) -> Result<
    (
        impl Stream<
            Item = Result<
                websocket_lowlevel::message::OwnedMessage,
                websocket_lowlevel::result::WebSocketError,
            >,
        >,
        impl Sink<
            websocket_lowlevel::message::OwnedMessage,
            Error = websocket_lowlevel::result::WebSocketError,
        >,
    ),
    websocket_lowlevel::result::WebSocketError,
>
where
    hyper::Uri: http::HttpTryFrom<U>,
{
    use tokio::codec::Decoder;

    // Our singleton hyper client
    lazy_static! {
        static ref CLIENT: hyper::Client<hyper_tls::HttpsConnector<hyper::client::HttpConnector>, hyper::Body> = {
            let https = hyper_tls::HttpsConnector::new().expect("Can't create HTTPS connector");
            let client = hyper::Client::builder().build::<_, hyper::Body>(https);

            client
        };
    }

    let client = &*CLIENT;

    // Get access to the concrete URI type
    let uri: hyper::Uri = match http::HttpTryFrom::try_from(uri) {
        Ok(uri) => uri,
        Err(e) => {
            return Err(websocket_lowlevel::result::WebSocketError::Other(Box::new(
                e.into(),
            )))
        }
    };

    // Translate ws/wss -> http/https
    let mut parts = uri.into_parts();
    let uri = match parts.scheme {
        Some(ref scheme)
            if scheme == &http::uri::Scheme::HTTP || scheme == &http::uri::Scheme::HTTPS =>
        {
            hyper::Uri::from_parts(parts)
        }
        Some(ref scheme) if scheme.as_str() == "ws" => {
            parts.scheme = Some(http::uri::Scheme::HTTP);
            hyper::Uri::from_parts(parts)
        }
        Some(ref scheme) if scheme.as_str() == "wss" => {
            parts.scheme = Some(http::uri::Scheme::HTTPS);
            hyper::Uri::from_parts(parts)
        }
        _ => unimplemented!(),
    }
    .map_err(|err| websocket_lowlevel::result::WebSocketError::Other(Box::new(err)))?;

    // Remember the host for later for the Host header
    let host = uri.host().map(String::from);

    // WebSocket Key header we use for this connection
    let key = websocket_lowlevel::header::WebSocketKey::new();

    // Generate our request and send it
    let mut req = hyper::Request::builder();
    req.uri::<hyper::Uri>(uri)
        .header(hyper::header::UPGRADE, "websocket")
        .header(hyper::header::CONNECTION, "Upgrade")
        .header("Sec-WebSocket-Version", "13")
        .header("Sec-WebSocket-Key", key.serialize());

    if let Some(host) = host {
        req.header(hyper::header::HOST, host);
    }

    let req = req
        .body(hyper::Body::empty())
        .map_err(|err| websocket_lowlevel::result::WebSocketError::Other(Box::new(err)))?;

    let res = client
        .request(req)
        .await
        .map_err(|err| websocket_lowlevel::result::WebSocketError::Other(Box::new(err)))?;

    // If switching protocols is not supported we can't do anything here
    if res.status() != hyper::StatusCode::SWITCHING_PROTOCOLS {
        return Err(websocket_lowlevel::result::WebSocketError::Other(Box::new(
            HttpUpgradeError::SwitchingProtocolsNotSupported(res.status()),
        )));
    }

    // Check if the accept header we get back is the correct one
    let headers = res.headers();
    let accept = match headers
        .get("Sec-WebSocket-Accept")
        .and_then(|h| h.to_str().ok())
    {
        None => {
            return Err(websocket_lowlevel::result::WebSocketError::Other(Box::new(
                HttpUpgradeError::NoAcceptHeader,
            )))
        }
        Some(accept) => accept,
    };

    let expected_accept = websocket_lowlevel::header::WebSocketAccept::new(&key);
    if expected_accept.serialize() != accept {
        return Err(websocket_lowlevel::result::WebSocketError::Other(Box::new(
            HttpUpgradeError::WrongAcceptHeader,
        )));
    }

    // And get our Stream/Sink for the Ws messages
    let (w, r) = match res.into_body().on_upgrade().await {
        Ok(upgrade) => {
            let framed = websocket_lowlevel::codec::ws::MessageCodec::default(
                websocket_lowlevel::codec::ws::Context::Client,
            )
            .framed(upgrade);

            framed.split()
        }
        Err(err) => {
            return Err(websocket_lowlevel::result::WebSocketError::Other(Box::new(
                HttpUpgradeError::UpgradeFailed(err),
            )));
        }
    };

    Ok((r, w))
}
