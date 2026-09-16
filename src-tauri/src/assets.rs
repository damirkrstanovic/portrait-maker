use tauri::http::{Response, StatusCode, header};
use uuid::Uuid;

use crate::state::DesktopState;
use portrait_core::thumbnails::MAX_THUMBNAIL_EDGE;
use portrait_core::types::Role;

pub(crate) fn serve(state: &DesktopState, path: &str) -> Response<Vec<u8>> {
    let Some((id, role, edge)) = parse(path) else {
        return response(StatusCode::BAD_REQUEST, Vec::new());
    };
    let path = match state.asset_path(id, role, edge) {
        Ok(path) => path,
        Err(_) => return response(StatusCode::NOT_FOUND, Vec::new()),
    };
    match std::fs::read(path) {
        Ok(bytes) => response(StatusCode::OK, bytes),
        Err(_) => response(StatusCode::NOT_FOUND, Vec::new()),
    }
}

fn parse(path: &str) -> Option<(Uuid, Role, Option<u32>)> {
    // `convertFileSrc` percent-encodes the complete route. Decode exactly once
    // before applying the deliberately narrow route grammar.
    let decoded = decode_path(path)?;
    let parts: Vec<_> = decoded.trim_matches('/').split('/').collect();
    let (variant, id, role, edge) = match parts.as_slice() {
        ["thumbnail", id, role, edge] => ("thumbnail", *id, *role, Some(edge.parse().ok()?)),
        ["original", id, role] => ("original", *id, *role, None),
        _ => return None,
    };
    let role = match role {
        "small" => Role::Small,
        "medium" => Role::Medium,
        "large" => Role::Large,
        _ => return None,
    };
    if variant == "thumbnail" && edge.is_none() {
        return None;
    }
    if edge.is_some_and(|edge| !(1..=MAX_THUMBNAIL_EDGE).contains(&edge)) {
        return None;
    }
    Some((Uuid::parse_str(id).ok()?, role, edge))
}

fn decode_path(path: &str) -> Option<String> {
    let bytes = path.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'%' {
            decoded.push(bytes[index]);
            index += 1;
            continue;
        }
        let high = *bytes.get(index + 1)?;
        let low = *bytes.get(index + 2)?;
        decoded.push((hex(high)? << 4) | hex(low)?);
        index += 3;
    }
    String::from_utf8(decoded).ok()
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn response(status: StatusCode, body: Vec<u8>) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "image/png")
        .header(header::CACHE_CONTROL, "no-store")
        .body(body)
        .expect("static asset response")
}

#[cfg(test)]
mod tests {
    use super::{parse, response};
    use tauri::http::{StatusCode, header};
    #[test]
    fn asset_paths_accept_only_ids_roles_and_bounded_thumbnail_shape() {
        let id = "c058f994-faf8-49d2-bc4f-1f9b7e1eb034";
        assert!(parse(&format!("/thumbnail/{id}/large/360")).is_some());
        assert!(parse("/thumbnail/../../etc/passwd/360").is_none());
        assert!(parse(&format!("/original/{id}/large/100")).is_none());
        assert!(parse(&format!("/thumbnail/{id}/arbitrary/360")).is_none());
        assert!(parse(&format!("/thumbnail/{id}/large/1025")).is_none());
    }

    #[test]
    fn asset_paths_accept_the_percent_encoded_route_from_convert_file_src() {
        let id = "c058f994-faf8-49d2-bc4f-1f9b7e1eb034";
        // Tauri's Linux/macOS and Windows helpers both encode the entire route:
        // portrait://localhost/%2Fthumbnail%2F... or http://portrait.localhost/%2Fthumbnail%2F...
        assert!(parse(&format!("/%2Fthumbnail%2F{id}%2Flarge%2F360")).is_some());
        assert!(parse("/%2Fthumbnail%2F..%2F..%2Fetc%2Fpasswd").is_none());
        assert!(parse("/%252Fthumbnail%252Fbad").is_none());
    }

    #[test]
    fn protocol_responses_are_never_stored_across_library_changes() {
        let response = response(StatusCode::NOT_FOUND, Vec::new());
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    }
}
