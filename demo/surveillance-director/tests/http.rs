//! HTTP-level smoke tests: routes, JSON shapes and error bodies, through the real router.

use std::path::Path;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use surveillance_director::World;
use surveillance_director::api::{ApiDoc, app};
use tower::ServiceExt;
use utoipa::OpenApi;

fn router() -> axum::Router {
    app(World::shared(), Path::new("/nonexistent"))
}

async fn call(router: &axum::Router, method: &str, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
    let mut req = Request::builder().method(method).uri(uri);
    let body = match body {
        Some(b) => {
            req = req.header("content-type", "application/json");
            Body::from(b.to_string())
        }
        None => Body::empty(),
    };
    let res = router.clone().oneshot(req.body(body).unwrap()).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

#[tokio::test]
async fn state_has_the_shape_the_ui_reads() {
    let (status, s) = call(&router(), "GET", "/api/state", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(s["cameras"].as_array().unwrap().len(), 3);
    let cam = &s["cameras"][0];
    for k in ["id", "name", "site", "zone", "keyId", "deviceKey", "pending", "lastSeq", "lastHead", "stored", "members"]
    {
        assert!(cam.get(k).is_some(), "cameras[0].{k}");
    }
    assert_eq!(cam["lastSeq"], 30);
    assert_eq!(s["counterparty"]["role"], "insurer");
    assert_eq!(s["vendor"]["member"], false);
    assert_eq!(s["roles"].as_array().unwrap().len(), 3);
    assert_eq!((&s["segmentSeconds"], &s["segmentsPerBatch"]), (&json!(10), &json!(6)));
    assert!(s["incident"].is_null() && s["lastVerify"].is_null());
    assert!(s["clock"]["sealedTo"].is_string());
    assert_eq!(s["activity"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn record_footage_and_batch_detail() {
    let r = router();
    let (_, s) = call(&r, "POST", "/api/record", Some(json!({ "segments": 3 }))).await;
    assert_eq!(s["cameras"][1]["pending"], 3);
    let (_, f) = call(&r, "GET", "/api/footage/CAM-CAGE", None).await;
    let segs = f["segments"].as_array().unwrap();
    assert_eq!(segs.len(), 183);
    assert_eq!((&segs[0]["batchSeq"], &segs[0]["scene"]), (&json!(1), &json!("empty_aisle")));
    assert!(segs[182]["batchSeq"].is_null(), "the last three are still pending");
    assert_eq!(segs[0]["contentHash"].as_str().unwrap().len(), 64);
    let (_, s) = call(&r, "POST", "/api/record", None).await; // an empty body means one minute
    assert_eq!(s["cameras"][1]["lastSeq"], 31);
    let (status, d) = call(&r, "GET", "/api/batch/CAM-CAGE/31", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(d["batch"]["items"].as_array().unwrap().len(), 6);
    assert!(d["batch"]["tree"]["left"]["hash"].is_string());
    assert_eq!(d["anchor"]["signatories"], json!(["WH3PL", "CPTY"]));
    let (_, l) = call(&r, "GET", "/api/ledger/CPTY", None).await;
    assert_eq!(l["chains"].as_array().unwrap().len(), 3);
    assert!(l["synchronizer"][0]["ciphertext"].is_string());
}

#[tokio::test]
async fn incident_tamper_verify_over_http() {
    let r = router();
    let (status, i) = call(&r, "POST", "/api/incident", None).await;
    assert_eq!(status, StatusCode::OK, "{i}");
    assert_eq!(i["consignment"], "C-1047");
    let (status, t) =
        call(&r, "POST", "/api/tamper", Some(json!({ "cam": "CAM-CAGE", "seq": 128, "mode": "delete_minute" }))).await;
    assert_eq!(status, StatusCode::OK, "{t}");
    let (_, x) = call(&r, "POST", "/api/export", None).await;
    assert_eq!(x["segments"], 534);
    let (_, v) = call(&r, "POST", "/api/verify", Some(json!({ "verifier": "VENDOR" }))).await;
    assert_eq!(
        (&v["ok"], &v["method"], &v["verifierName"]),
        (&json!(false), &json!("evidence_package"), &json!("Vendor"))
    );
    let cage = &v["cams"][1];
    assert_eq!((&cage["cam"], &cage["counts"]["missing"]), (&json!("CAM-CAGE"), &json!(6)));
    assert_eq!(cage["segments"][126]["status"], "missing");
    assert_eq!(cage["batches"][21]["presentCount"], 0);
    let (_, s) = call(&r, "GET", "/api/state", None).await;
    assert_eq!(s["lastVerify"]["ok"], false);
    assert_eq!(s["export"]["segments"], 534);
}

#[tokio::test]
async fn role_and_vendor_membership() {
    let r = router();
    let (_, s) = call(&r, "POST", "/api/counterparty", Some(json!({ "role": "sthiranet" }))).await;
    assert_eq!(s["counterparty"]["name"], "Sthiranet");
    let (_, s) = call(&r, "POST", "/api/vendor/member", None).await;
    assert_eq!(s["vendor"]["member"], true);
    assert_eq!(s["cameras"][0]["members"], json!(["WH3PL", "CPTY", "VENDOR"]));
    let (_, s) = call(&r, "POST", "/api/reset", None).await;
    assert_eq!((&s["vendor"]["member"], &s["counterparty"]["role"]), (&json!(false), &json!("sthiranet")));
}

#[tokio::test]
async fn errors_are_json() {
    let r = router();
    let (status, e) = call(&r, "GET", "/api/footage/CAM-ROOF", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(e["error"], "Unknown camera CAM-ROOF");
    let (status, e) = call(&r, "POST", "/api/verify", Some(json!({ "verifier": "CPTY" }))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(e["error"], "Raise the incident first");
    let (status, e) = call(&r, "GET", "/api/nope", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(e["error"], "Not found");
    let req =
        Request::post("/api/tamper").header("content-type", "application/json").body(Body::from("{oops")).unwrap();
    assert_eq!(r.clone().oneshot(req).await.unwrap().status(), StatusCode::BAD_REQUEST);
    let (status, _) = call(&r, "POST", "/api/counterparty", Some(json!({ "role": "nobody" }))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn rewrite_is_rejected() {
    let (status, r) = call(
        &router(),
        "POST",
        "/api/ledger/rewrite",
        Some(json!({ "actor": "WH3PL", "chain": "CAM-DOCK", "seq": 2 })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(r["ok"], false);
    assert_eq!(r["missing"], json!(["CPTY"]));
}

#[test]
fn the_openapi_spec_lists_every_route() {
    let spec = serde_json::to_value(ApiDoc::openapi()).unwrap();
    let paths = spec["paths"].as_object().unwrap();
    for p in [
        "/api/state",
        "/api/record",
        "/api/footage/{cam}",
        "/api/batch/{cam}/{seq}",
        "/api/ledger/{party}",
        "/api/ledger/rewrite",
        "/api/tamper",
        "/api/incident",
        "/api/export",
        "/api/verify",
        "/api/counterparty",
        "/api/vendor/member",
        "/api/auto",
        "/api/reset",
        "/api/stream",
    ] {
        assert!(paths.contains_key(p), "{p}");
    }
    assert!(spec["components"]["schemas"].get("VerifyResponse").is_some());
}
