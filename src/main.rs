use std::convert::Infallible;
use std::net::SocketAddr;
use std::str::FromStr;

use anyhow::Result;
use clap::Parser;
use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode, Uri, body::Incoming};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;
use tracing::{error, info};
use tracing_subscriber;
use url::Url;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
#[command(disable_help_flag = true)]
struct Args {
    /// Host to bind to
    #[arg(short = 'h', long = "host", default_value = "0.0.0.0")]
    host: String,

    /// Port to bind to
    #[arg(short = 'p', long = "port", default_value_t = 1234)]
    port: u16,

    /// Print help
    #[arg(long = "help", action = clap::ArgAction::Help)]
    help: Option<bool>,
}

async fn proxy_handler(req: Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    let method = req.method().clone();
    let uri = req.uri().clone();

    match proxy_request(req).await {
        Ok(response) => {
            tracing::debug!("{} {} -> {}", method, uri, response.status());
            Ok(response)
        }
        Err(e) => {
            error!("Proxy error for {} {}: {}", method, uri, e);
            Ok(Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Full::new(Bytes::from(format!("Proxy error: {}", e))))
                .unwrap())
        }
    }
}

async fn proxy_request(req: Request<Incoming>) -> Result<Response<Full<Bytes>>> {
    let uri = req.uri();
    let path = uri.path();

    // Extract target URL (remove leading '/')
    let target_url_str = if path.starts_with('/') {
        &path[1..]
    } else {
        path
    };

    // If no protocol prefix, default to https
    let target_url_str =
        if !target_url_str.starts_with("http://") && !target_url_str.starts_with("https://") {
            format!("https://{}", target_url_str)
        } else {
            target_url_str.to_string()
        };

    // Parse target URL
    let target_url = match Url::parse(&target_url_str) {
        Ok(url) => url,
        Err(_) => {
            return Ok(Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Full::new(Bytes::from("Invalid target URL")))
                .unwrap());
        }
    };

    // Build new request
    let target_uri = Uri::from_str(&target_url.to_string())?;

    // Collect original request body
    let (parts, body) = req.into_parts();
    let body_bytes = body.collect().await?.to_bytes();

    // Create new request
    let mut new_req = Request::builder().method(parts.method).uri(&target_uri);

    // Copy all headers but replace Host
    for (name, value) in parts.headers.iter() {
        if name != "host" {
            new_req = new_req.header(name, value);
        }
    }

    // Set new Host header
    if let Some(host) = target_url.host_str() {
        let host_with_port = if let Some(port) = target_url.port() {
            format!("{}:{}", host, port)
        } else {
            host.to_string()
        };
        new_req = new_req.header("host", host_with_port);
    }

    let new_req = new_req.body(Full::new(body_bytes))?;

    // Send request - choose different client based on protocol
    let response = if target_url.scheme() == "https" {
        // HTTPS request
        let https = hyper_tls::HttpsConnector::new();
        let client =
            hyper_util::client::legacy::Client::builder(hyper_util::rt::TokioExecutor::new())
                .build(https);
        client.request(new_req).await
    } else {
        // HTTP request
        let client =
            hyper_util::client::legacy::Client::builder(hyper_util::rt::TokioExecutor::new())
                .build_http();
        client.request(new_req).await
    };

    let response = match response {
        Ok(resp) => resp,
        Err(e) => {
            return Ok(Response::builder()
                .status(StatusCode::BAD_GATEWAY)
                .body(Full::new(Bytes::from(format!("Request failed: {}", e))))
                .unwrap());
        }
    };

    // Process response
    let (mut resp_parts, resp_body) = response.into_parts();
    let resp_body_bytes = resp_body.collect().await?.to_bytes();

    // Process Location header
    if let Some(location_header) = resp_parts.headers.get("location") {
        if let Ok(location_str) = location_header.to_str() {
            let new_location =
                process_location_header(location_str, &parts.headers, &parts.uri, &target_url);
            if let Some(new_loc) = new_location {
                resp_parts
                    .headers
                    .insert("location", new_loc.parse().unwrap());
            }
        }
    }

    // Build response
    let mut response_builder = Response::builder()
        .status(resp_parts.status)
        .version(resp_parts.version);

    for (name, value) in resp_parts.headers.iter() {
        response_builder = response_builder.header(name, value);
    }

    Ok(response_builder.body(Full::new(resp_body_bytes))?)
}

fn process_location_header(
    location: &str,
    request_headers: &hyper::HeaderMap,
    request_uri: &Uri,
    target_url: &Url,
) -> Option<String> {
    // If Location is a complete URL, return proxy version directly
    if location.starts_with("http://") || location.starts_with("https://") {
        if let Ok(_location_url) = Url::parse(location) {
            // Get request origin
            let request_origin = get_request_origin(request_headers, request_uri);
            return Some(format!("{}/{}", request_origin, location));
        }
    } else if location.starts_with('/') {
        // Relative path, need to combine origin
        let request_origin = get_request_origin(request_headers, request_uri);
        let target_origin = format!(
            "{}://{}",
            target_url.scheme(),
            target_url.host_str().unwrap_or("")
        );

        if let Some(port) = target_url.port() {
            let target_origin = format!("{}:{}", target_origin, port);
            return Some(format!("{}{}{}", request_origin, target_origin, location));
        } else {
            return Some(format!("{}{}{}", request_origin, target_origin, location));
        }
    }

    None
}

fn get_request_origin(headers: &hyper::HeaderMap, uri: &Uri) -> String {
    // First try to get from Origin header
    if let Some(origin_header) = headers.get("origin") {
        if let Ok(origin_str) = origin_header.to_str() {
            return origin_str.to_string();
        }
    }

    // If no Origin header, build from request
    let scheme = if let Some(scheme) = uri.scheme_str() {
        scheme
    } else {
        "http" // Default protocol
    };

    let host = if let Some(host_header) = headers.get("host") {
        if let Ok(host_str) = host_header.to_str() {
            host_str
        } else {
            "localhost:1234" // Default value
        }
    } else {
        "localhost:1234" // Default value
    };

    format!("{}://{}", scheme, host)
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing with default info level
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let args = Args::parse();

    let addr = SocketAddr::new(args.host.parse()?, args.port);
    let listener = TcpListener::bind(addr).await?;

    info!("Proxy is running on http://{}", addr);

    loop {
        let (stream, _) = listener.accept().await?;
        let io = TokioIo::new(stream);

        tokio::task::spawn(async move {
            if let Err(err) = http1::Builder::new()
                .serve_connection(io, service_fn(proxy_handler))
                .await
            {
                error!("Error serving connection: {:?}", err);
            }
        });
    }
}
