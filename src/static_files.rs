use axum::{
    body::Body,
    extract::Request,
    http::{
        header::{ACCEPT_ENCODING, CACHE_CONTROL, CONTENT_ENCODING, CONTENT_TYPE},
        StatusCode, Uri,
    },
    response::{IntoResponse, Response},
};
use std::path::{Path, PathBuf};
use tokio::fs;
use tower::ServiceExt;
use tower_http::services::ServeDir;
use tracing::{debug, trace};

/// Custom static file service that serves pre-compressed files (.br, .gz)
/// when the client supports them, matching express-static-gzip behavior
pub struct PreCompressedStaticFiles {
    serve_dir: ServeDir,
    base_path: PathBuf,
}

impl PreCompressedStaticFiles {
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        let base_path = path.as_ref().to_path_buf();
        let serve_dir = ServeDir::new(&base_path).append_index_html_on_directories(true);

        Self {
            serve_dir,
            base_path,
        }
    }

    /// Check if a file exists and return its path
    async fn check_file(&self, path: &Path) -> Option<PathBuf> {
        let full_path = self.base_path.join(path);
        if fs::metadata(&full_path).await.is_ok() {
            Some(full_path)
        } else {
            None
        }
    }

    /// Detect supported encodings from Accept-Encoding header
    fn parse_accept_encoding(accept_encoding: &str) -> (bool, bool) {
        let lower = accept_encoding.to_lowercase();
        let supports_br = lower.contains("br");
        let supports_gzip = lower.contains("gzip");
        (supports_br, supports_gzip)
    }

    /// Try to serve a pre-compressed version of the file
    async fn try_compressed(
        &self,
        path: &str,
        supports_br: bool,
        supports_gzip: bool,
    ) -> Option<(PathBuf, &'static str)> {
        // Remove leading slash
        let path = path.trim_start_matches('/');

        // Try brotli first (best compression)
        if supports_br {
            let br_path = format!("{}.br", path);
            if let Some(full_path) = self.check_file(Path::new(&br_path)).await {
                trace!("Serving brotli compressed: {}", br_path);
                return Some((full_path, "br"));
            }
        }

        // Try gzip second
        if supports_gzip {
            let gz_path = format!("{}.gz", path);
            if let Some(full_path) = self.check_file(Path::new(&gz_path)).await {
                trace!("Serving gzip compressed: {}", gz_path);
                return Some((full_path, "gzip"));
            }
        }

        None
    }

    /// Get MIME type from file path
    fn get_mime_type(path: &Path) -> &'static str {
        let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");

        // Strip .br or .gz if present to get actual file extension
        let actual_path = if extension == "br" || extension == "gz" {
            path.file_stem()
                .and_then(|s| s.to_str())
                .and_then(|s| Path::new(s).extension())
                .and_then(|e| e.to_str())
                .unwrap_or("")
        } else {
            extension
        };

        match actual_path {
            "html" => "text/html; charset=utf-8",
            "css" => "text/css; charset=utf-8",
            "js" => "text/javascript; charset=utf-8",
            "json" => "application/json",
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "svg" => "image/svg+xml",
            "ico" => "image/x-icon",
            "woff" => "font/woff",
            "woff2" => "font/woff2",
            _ => "application/octet-stream",
        }
    }

    /// Serve a file with appropriate headers
    async fn serve_file(path: PathBuf, encoding: Option<&str>, is_immutable: bool) -> Response {
        match fs::read(&path).await {
            Ok(contents) => {
                let mime_type = Self::get_mime_type(&path);
                let mut response = Response::builder()
                    .status(StatusCode::OK)
                    .header(CONTENT_TYPE, mime_type);

                // Set Content-Encoding if compressed
                if let Some(enc) = encoding {
                    response = response.header(CONTENT_ENCODING, enc);
                }

                // Set cache headers
                // Assets in /assets/ folder are immutable (they have content hashes)
                let cache_value = if is_immutable {
                    "public, max-age=31536000, immutable"
                } else {
                    "public, max-age=3600"
                };
                response = response.header(CACHE_CONTROL, cache_value);

                response.body(Body::from(contents)).unwrap().into_response()
            }
            Err(err) => {
                debug!("Failed to read file {:?}: {}", path, err);
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
        }
    }

    /// Handle a request for static files
    pub async fn handle(&self, uri: Uri, req: Request) -> Response {
        let path = uri.path();

        // Check if this is an immutable asset (in /assets/ folder)
        let is_immutable = path.starts_with("/assets/");

        // Get Accept-Encoding header
        let accept_encoding = req
            .headers()
            .get(ACCEPT_ENCODING)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        let (supports_br, supports_gzip) = Self::parse_accept_encoding(accept_encoding);

        // Try to serve pre-compressed version
        if supports_br || supports_gzip {
            if let Some((compressed_path, encoding)) =
                self.try_compressed(path, supports_br, supports_gzip).await
            {
                return Self::serve_file(compressed_path, Some(encoding), is_immutable).await;
            }
        }

        // Fall back to regular file serving via ServeDir
        // Convert back to request
        match self.serve_dir.clone().oneshot(req).await {
            Ok(mut response) => {
                // Add cache headers to regular responses
                let cache_value = if is_immutable {
                    "public, max-age=31536000, immutable"
                } else {
                    "public, max-age=3600"
                };
                response
                    .headers_mut()
                    .insert(CACHE_CONTROL, cache_value.parse().unwrap());
                response.into_response()
            }
            Err(err) => {
                debug!("ServeDir error: {}", err);
                StatusCode::NOT_FOUND.into_response()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_accept_encoding() {
        let (br, gzip) = PreCompressedStaticFiles::parse_accept_encoding("gzip, deflate, br");
        assert!(br);
        assert!(gzip);

        let (br, gzip) = PreCompressedStaticFiles::parse_accept_encoding("gzip, deflate");
        assert!(!br);
        assert!(gzip);

        let (br, gzip) = PreCompressedStaticFiles::parse_accept_encoding("identity");
        assert!(!br);
        assert!(!gzip);
    }

    #[test]
    fn test_mime_types() {
        assert_eq!(
            PreCompressedStaticFiles::get_mime_type(Path::new("index.html")),
            "text/html; charset=utf-8"
        );
        assert_eq!(
            PreCompressedStaticFiles::get_mime_type(Path::new("index.html.br")),
            "text/html; charset=utf-8"
        );
        assert_eq!(
            PreCompressedStaticFiles::get_mime_type(Path::new("app.js.gz")),
            "text/javascript; charset=utf-8"
        );
        assert_eq!(
            PreCompressedStaticFiles::get_mime_type(Path::new("style.css")),
            "text/css; charset=utf-8"
        );
    }
}
