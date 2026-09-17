use portrait_core::types::{
    AppError, CatalogPage, ExportMode, ImportKind, ImportRequest, Page, Query, Role,
};
use serde_json::json;

#[test]
fn ipc_contracts_serialize_with_camel_case_and_string_unions() {
    let request = ImportRequest {
        path: "/tmp/source".into(),
        source_name: "Heroes".into(),
        kind: ImportKind::Archive,
        resize: false,
        duplicate_policy: Default::default(),
    };
    assert_eq!(
        serde_json::to_value(request).unwrap(),
        json!({"path":"/tmp/source", "sourceName":"Heroes", "kind":"archive", "resize":false, "duplicatePolicy":"keep"})
    );
    assert_eq!(serde_json::to_value(Role::Large).unwrap(), json!("large"));
    assert_eq!(
        serde_json::to_value(ExportMode::Replace).unwrap(),
        json!("replace")
    );
}

#[test]
fn page_rejects_negative_numbers_at_the_ipc_boundary() {
    let error = serde_json::from_value::<Page>(json!({"offset": -1, "limit": 50})).unwrap_err();
    assert!(error.to_string().contains("invalid value"));
}

#[test]
fn page_rejects_limits_above_two_hundred_at_the_ipc_boundary() {
    let error = serde_json::from_value::<Page>(json!({"offset": 0, "limit": 201})).unwrap_err();
    assert!(error.to_string().contains("200"));
}

#[test]
fn representative_response_contract_round_trips_without_field_drift() {
    let value = json!({
        "items": [],
        "total": 0,
        "revision": 7
    });
    let page: CatalogPage = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(page).unwrap(), value);

    let query = json!({
        "text": "elf ranger",
        "sourceIds": [],
        "labels": [{"category":"class", "value":"ranger"}],
        "selectedOnly": false,
        "trash": false
    });
    let parsed: Query = serde_json::from_value(query.clone()).unwrap();
    assert_eq!(serde_json::to_value(parsed).unwrap(), query);

    let app_error = AppError {
        code: "LIBRARY_LOCKED".into(),
        message: "Close the library in the other window and try again.".into(),
        recoverable: true,
    };
    assert_eq!(
        serde_json::to_value(app_error).unwrap(),
        json!({
            "code":"LIBRARY_LOCKED",
            "message":"Close the library in the other window and try again.",
            "recoverable":true
        })
    );
}
