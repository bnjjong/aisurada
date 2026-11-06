//! Chapter 10. No broad asset scope, CORS, or parent capabilities. Network is limited to the hosts
//! named in `CSP`: the Leaflet CDN and the map tile servers, so an attached HTML can show a map.
use std::{fs, path::Path};
use tauri::http::{Response, StatusCode};
/// Sent with every `aisframe://` response. Besides the box's own files, only the Leaflet CDN
/// (`cdnjs.cloudflare.com`: script, style, images) and the tile hosts (`xdworld.vworld.kr`,
/// `server.arcgisonline.com`: images) are reachable. For another host, add it here and rebuild.
/// Keep `connect-src` closed (it falls back to `default-src 'none'`) and use no wildcard or bare scheme.
pub const CSP: &str = concat!(
    "default-src 'none'; ",
    "script-src 'unsafe-inline' aisframe: http://aisframe.localhost https://cdnjs.cloudflare.com; ",
    "style-src 'unsafe-inline' aisframe: http://aisframe.localhost https://cdnjs.cloudflare.com; ",
    "img-src data: blob: aisframe: http://aisframe.localhost https://cdnjs.cloudflare.com https://xdworld.vworld.kr https://server.arcgisonline.com; ",
    "font-src data: aisframe: http://aisframe.localhost; ",
    "media-src data: blob: aisframe: http://aisframe.localhost; ",
    "sandbox allow-scripts"
);
fn response(status: u16, mime: Option<&str>, body: Vec<u8>) -> Response<Vec<u8>> {
    let mut b = Response::builder()
        .status(StatusCode::from_u16(status).unwrap())
        .header("Content-Security-Policy", CSP)
        .header("X-Content-Type-Options", "nosniff")
        .header("Cache-Control", "no-store");
    if let Some(m) = mime {
        b = b.header("Content-Type", m)
    }
    b.body(body).unwrap()
}
fn decode(path: &str) -> Option<String> {
    let bytes = path.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len()
                || !bytes[i + 1].is_ascii_hexdigit()
                || !bytes[i + 2].is_ascii_hexdigit()
            {
                return None;
            }
            i += 3
        } else {
            i += 1
        }
    }
    percent_encoding::percent_decode_str(path)
        .decode_utf8()
        .ok()
        .map(|s| s.into_owned())
}
pub fn serve(vault: Option<&str>, url_path: &str) -> Response<Vec<u8>> {
    let Some(vault) = vault else {
        return response(503, None, vec![]);
    };
    let Some(path) = decode(url_path) else {
        return response(404, None, vec![]);
    };
    let Ok(root) = Path::new(vault).canonicalize() else {
        return response(503, None, vec![]);
    };
    let Ok(path) = Path::new(&path).canonicalize() else {
        return response(404, None, vec![]);
    };
    let Ok(rel) = path.strip_prefix(root) else {
        return response(403, None, vec![]);
    };
    let parts = rel.components().collect::<Vec<_>>();
    if parts.len() < 3
        || parts[0].as_os_str() != "notes"
        || !parts[1..parts.len() - 1]
            .iter()
            .any(|p| p.as_os_str() == "attach")
        || !path.is_file()
    {
        return response(403, None, vec![]);
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let mime = match ext.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        _ => return response(403, None, vec![]),
    };
    let Ok(meta) = fs::metadata(&path) else {
        return response(404, None, vec![]);
    };
    if meta.len() > 33_554_432 {
        return response(413, None, vec![]);
    }
    let Ok(mut bytes) = fs::read(&path) else {
        return response(404, None, vec![]);
    };
    if matches!(ext.as_str(), "html" | "htm") {
        let scripts = format!(
            "<script>{}</script><script>{}</script>",
            include_str!("frame_height.js"),
            include_str!("frame_links.js")
        );
        let at = bytes
            .windows(7)
            .rposition(|b| b.eq_ignore_ascii_case(b"</body>"))
            .unwrap_or(bytes.len());
        bytes.splice(at..at, scripts.bytes());
    }
    response(200, Some(mime), bytes)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn protocol_statuses_scopes_and_injection() {
        let t = tempfile::tempdir().unwrap();
        let root = t.path().to_str().unwrap();
        let p = t.path().join("notes/a/attach");
        fs::create_dir_all(&p).unwrap();
        let f = p.join("frame.HTML");
        fs::write(&f, b"<body>\xff</BoDy>").unwrap();
        let encoded = percent_encoding::utf8_percent_encode(
            f.to_str().unwrap(),
            percent_encoding::NON_ALPHANUMERIC,
        )
        .to_string();
        let r = serve(Some(root), &encoded);
        assert_eq!(r.status(), 200);
        assert_eq!(r.headers()["Content-Security-Policy"], CSP);
        assert!(r.body().contains(&255));
        assert!(String::from_utf8_lossy(r.body()).contains("aisframeHeight"));
        assert!(r.body().ends_with(b"</BoDy>"));
        for (vault, path, status) in [
            (None, f.to_str().unwrap(), 503),
            (Some(root), "/%ZZ", 404),
            (Some(root), root, 403),
            (Some(root), "/missing", 404),
        ] {
            let r = serve(vault, path);
            assert_eq!(r.status(), status);
            assert!(r.body().is_empty());
            assert!(!r.headers().contains_key("Content-Type"));
            assert!(!r.headers().contains_key("Access-Control-Allow-Origin"));
        }
        let outside = t.path().join("outside.html");
        fs::write(&outside, "x").unwrap();
        assert_eq!(serve(Some(root), outside.to_str().unwrap()).status(), 403);
        fs::write(p.join("bad.exe"), "x").unwrap();
        assert_eq!(
            serve(Some(root), p.join("bad.exe").to_str().unwrap()).status(),
            403
        );
        let huge = fs::File::create(p.join("huge.js")).unwrap();
        huge.set_len(33_554_433).unwrap();
        assert_eq!(
            serve(Some(root), p.join("huge.js").to_str().unwrap()).status(),
            413
        );
    }
}

#[cfg(test)]
mod csp_tests {
    use super::*;
    fn directive(name: &str) -> &'static str {
        CSP.split("; ").find(|d| d.starts_with(name)).unwrap_or("")
    }
    #[test]
    fn network_is_limited_to_the_leaflet_cdn_and_tile_hosts() {
        assert!(CSP.starts_with("default-src 'none'; "));
        assert!(CSP.ends_with("; sandbox allow-scripts"));
        assert!(!CSP.contains("connect-src"));
        for name in ["script-src ", "style-src "] {
            assert!(directive(name).split(' ').any(|s| s == "https://cdnjs.cloudflare.com"), "{name}");
        }
        for host in ["https://cdnjs.cloudflare.com", "https://xdworld.vworld.kr", "https://server.arcgisonline.com"] {
            assert!(directive("img-src ").split(' ').any(|s| s == host), "{host}");
        }
        for name in ["font-src ", "media-src "] {
            assert!(!directive(name).contains("https://"), "{name}");
        }
        for source in CSP.split("; ").flat_map(|d| d.split(' ').skip(1)) {
            assert!(!source.contains('*'), "{source}");
            assert!(!matches!(source, "https:" | "http:"), "{source}");
        }
    }
}

#[cfg(test)]
mod boundary_tests {
    use super::*;
    #[test]
    fn malformed_utf8_and_symlink_escape_are_rejected() {
        let t = tempfile::tempdir().unwrap();
        let root = t.path().join("vault");
        let dir = root.join("notes/a/attach");
        fs::create_dir_all(&dir).unwrap();
        let outside = t.path().join("outside.html");
        fs::write(&outside, "hidden").unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&outside, dir.join("alias.html")).unwrap();
            assert_eq!(
                serve(root.to_str(), dir.join("alias.html").to_str().unwrap()).status(),
                403
            );
        }
        for p in ["/%FF", "/%E0%A4%A", "/%"] {
            assert_eq!(serve(root.to_str(), p).status(), 404);
        }
        let f = dir.join("x.html");
        fs::write(&f, "no body").unwrap();
        let r = serve(root.to_str(), f.to_str().unwrap());
        assert_eq!(r.status(), 200);
        assert!(r.body().starts_with(b"no body<script>"));
    }
}
