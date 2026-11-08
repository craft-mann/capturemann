//! The camera half of the Linux recording sources (CAM-1, CAM-2, CAM-4): what the camera adds to
//! the preflight, the device names, the track and the `list_devices` answer. The decisions are
//! functions over the camera list and the permission so they are tested with scripted cameras;
//! `linux.rs` calls them through [`LinuxEnv`](super::linux::LinuxEnv)'s camera methods.

use cm_camera::{CameraConfig, CameraInfo, CameraPermission};
use cm_core::CmError;
use serde_json::{json, Value};

use crate::recording::params::CameraParams;
use crate::recording::sources::SensorRequest;

/// The preflight line in the device report.
pub fn report_line(cameras: &Result<Vec<CameraInfo>, CmError>, perm: CameraPermission) -> String {
    match cameras {
        Err(e) => format!("camera: {e}"),
        Ok(c) if c.is_empty() => "camera: none found (no /dev/video* device)".to_owned(),
        Ok(c) => format!(
            "camera: {} found ({}) through Video4Linux2, recorded as software VP9; access {}",
            c.len(),
            c.iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            if perm.is_authorized() {
                "allowed (no prompt on Linux)"
            } else {
                "denied"
            }
        ),
    }
}

/// The errors a camera request has before it starts: no access to the device nodes, no camera at
/// all, or a named camera that is not there.
pub fn preflight(
    req: &SensorRequest,
    cameras: &Result<Vec<CameraInfo>, CmError>,
    perm: CameraPermission,
) -> Vec<CmError> {
    if !req.camera {
        return Vec::new();
    }
    if let Err(e) = perm.require() {
        return vec![e];
    }
    match cameras {
        Err(e) => vec![e.clone()],
        Ok(c) if c.is_empty() => vec![CmError::not_found(
            "No camera was found. Connect a webcam (it shows up as /dev/video0) and try again.",
        )],
        Ok(c) => match req.camera_device.as_deref() {
            Some(id) if !c.iter().any(|x| x.id == id) => {
                vec![CmError::not_found(format!("Camera {id} is not connected."))]
            }
            _ => Vec::new(),
        },
    }
}

/// The name of the camera the request would use.
pub fn device_name(
    req: &SensorRequest,
    cameras: &Result<Vec<CameraInfo>, CmError>,
) -> Option<String> {
    req.camera.then(|| {
        cameras
            .as_ref()
            .ok()
            .and_then(|c| cm_camera::resolve_camera(c, req.camera_device.as_deref()).ok())
            .map_or_else(|| "the default camera".to_owned(), |(c, _)| c.name.clone())
    })
}

/// How the camera track records.
pub fn config(c: &CameraParams) -> CameraConfig {
    CameraConfig {
        device_id: c.device_id.clone(),
        resolution: c.resolution,
        mirror: c.mirror,
        ..CameraConfig::default()
    }
}

/// The camera fields of `list_devices`.
pub fn devices(
    cameras: Result<Vec<CameraInfo>, CmError>,
    perm: CameraPermission,
) -> [(&'static str, Value); 3] {
    let (list, error) = match cameras {
        Ok(c) => (json!(c), Value::Null),
        Err(e) => (json!([]), json!(e.to_string())),
    };
    [
        ("cameras", list),
        ("cameras_error", error),
        ("camera_permission", json!(perm)),
    ]
}

#[cfg(test)]
mod tests {
    use cm_camera::{CameraKind, CameraMode};

    use super::*;

    fn cam(id: &str) -> CameraInfo {
        CameraInfo {
            id: id.into(),
            name: format!("Camera {id}"),
            kind: CameraKind::BuiltIn,
            is_default: id == "a",
            max_size: [1280, 720],
            max_fps: 30.0,
            modes: vec![CameraMode {
                width: 1280,
                height: 720,
                min_fps: 5.0,
                max_fps: 30.0,
            }],
            in_use_elsewhere: false,
        }
    }

    fn req(device: Option<&str>) -> SensorRequest {
        SensorRequest {
            camera: true,
            camera_device: device.map(str::to_owned),
            ..SensorRequest::default()
        }
    }

    #[test]
    fn no_camera_request_adds_nothing() {
        let r = SensorRequest::default();
        assert!(preflight(&r, &Ok(vec![]), CameraPermission::Denied).is_empty());
        assert_eq!(device_name(&r, &Ok(vec![])), None);
    }

    #[test]
    fn a_missing_camera_or_missing_access_is_named() {
        let none = preflight(&req(None), &Ok(vec![]), CameraPermission::Authorized);
        assert_eq!(none.len(), 1);
        assert_eq!(none[0].code(), cm_core::ErrorCode::NotFound);
        let denied = preflight(&req(None), &Ok(vec![cam("a")]), CameraPermission::Denied);
        assert_eq!(denied[0].code(), cm_core::ErrorCode::PermissionMissing);
        let gone = preflight(
            &req(Some("b")),
            &Ok(vec![cam("a")]),
            CameraPermission::Authorized,
        );
        assert_eq!(gone[0].code(), cm_core::ErrorCode::NotFound);
        assert!(preflight(
            &req(Some("a")),
            &Ok(vec![cam("a")]),
            CameraPermission::Authorized
        )
        .is_empty());
    }

    #[test]
    fn the_name_is_the_chosen_cameras_or_the_default_phrase() {
        let cams = Ok(vec![cam("a"), cam("b")]);
        assert_eq!(
            device_name(&req(Some("b")), &cams).as_deref(),
            Some("Camera b")
        );
        assert_eq!(device_name(&req(None), &cams).as_deref(), Some("Camera a"));
        assert_eq!(
            device_name(&req(None), &Err(CmError::internal("x"))).as_deref(),
            Some("the default camera")
        );
    }

    #[test]
    fn list_devices_carries_the_cameras_and_the_permission() {
        let f = devices(Ok(vec![cam("a")]), CameraPermission::Authorized);
        assert_eq!(f[0].1[0]["id"], "a");
        assert_eq!(f[1].1, Value::Null);
        assert_eq!(f[2].1, "authorized");
        let f = devices(Err(CmError::internal("boom")), CameraPermission::Denied);
        assert_eq!(f[0].1, json!([]));
        assert!(f[1].1.as_str().unwrap().contains("boom"));
    }
}
