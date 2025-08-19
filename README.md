# Mirrors to Proxy

An HTTP proxy server written in Rust that supports proxying requests to target servers and handling Location headers in responses.

## Features

- Proxy HTTP/HTTPS requests to target servers
- Automatic Host header replacement
- Smart handling of Location header redirects in responses
- Support for custom listening address and port

## Usage

### Direct Execution

```bash
# Use default configuration (0.0.0.0:1234)
cargo run

# Custom host and port
cargo run -- --host 127.0.0.1 --port 8080
cargo run -- -h 0.0.0.0 -p 3000
```

### Command Line Options

- `-h, --host <HOST>`: Binding host address (default: 0.0.0.0)
- `-p, --port <PORT>`: Binding port number (default: 1234)

### Proxy Request Examples

Accessing `http://localhost:1234/https://github.com` will proxy the request to `https://github.com`

If the target URL has no protocol prefix, HTTPS will be used by default:

- `http://localhost:1234/github.com` → `https://github.com`

## Docker Support

To build the Docker image, run:

```bash
docker build -t m2proxy .
```

To run the Docker container:

```bash
# Use default configuration
docker run -p 1234:1234 m2proxy

# Custom configuration
docker run -p 8080:8080 m2proxy --host 0.0.0.0 --port 8080
```

[docker-compose.yml] are provided for easier deployment and management of the proxy server.

```bash
docker-compose up -d
```

## Location Header Processing

The proxy server intelligently handles Location headers in responses:

1. **Full URL**: `Location: https://example.com/path`
   → `Location: http://localhost:1234/https://example.com/path`

2. **Relative Path**: `Location: /redirect-path`
   → `Location: http://localhost:1234/https://target-domain.com/redirect-path`

Origin Acquisition Priority:

1. Get from `Origin` header
2. Build from request protocol and `Host` header

## License

Copyright (c) Cnily03. All rights reserved.

Licensed under the [MIT](LICENSE) License.
