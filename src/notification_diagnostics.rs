//! Bounded, metadata-only diagnostics using the production native sampler.
//! This mode never returns sender/body strings or retains native AX actions.
use super::Config;
use std::{ffi::{c_char,c_void,CStr,CString},time::{Duration,Instant}};

extern "C" {
    fn qy_notifications_access_diagnostics()->*mut c_char;
    fn qy_notifications_sample(rules:*const c_char,active:extern "C" fn(*mut c_void)->bool,context:*mut c_void)->*mut c_char;
    fn qy_notifications_free(value:*mut c_char);
    fn qy_notifications_clear();
}
extern "C" fn active(context:*mut c_void)->bool {Instant::now()<unsafe{*(context as *const Instant)}}

/// This is the current command's launch context. TCC can attribute a command
/// to Terminal or Codex; no public API exposes that responsible-process chain.
/// It must never be treated as a permission check on a separate running GUI.
pub fn access_report()->serde_json::Value {
    let raw=unsafe{qy_notifications_access_diagnostics()};
    let mut report=if raw.is_null(){serde_json::json!({"error":"native_identity_unavailable"})}else{
        let result=serde_json::from_slice(unsafe{CStr::from_ptr(raw)}.to_bytes()).unwrap_or_else(|_|serde_json::json!({"error":"native_identity_unavailable"}));
        unsafe{qy_notifications_free(raw)};result
    };
    report["kind"]="notification_access_context".into();
    report["running_gui_authorization"]="not_checked".into();
    report
}

pub fn check(config:Config,seconds:u64)->bool {
    println!("{}",access_report());
    let seconds=seconds.clamp(1,60);
    if !config.enabled||!config.previews {
        println!("{}",serde_json::json!({"kind":"notification_preview_summary","enabled":config.enabled,"previews_enabled":config.previews,"captured":false,"reason":"monitor_or_previews_disabled"}));
        return false;
    }
    let mut rules:serde_json::Value=serde_json::from_str(&config.rules_json()).unwrap_or_default();
    rules["diagnostics"]=true.into();rules["diagnostic_only"]=true.into();
    let rules=CString::new(rules.to_string()).unwrap();
    let deadline=Instant::now()+Duration::from_secs(seconds);
    let mut samples=0;let mut captured=false;let mut accessible=false;
    unsafe{qy_notifications_clear()};
    while Instant::now()<deadline&&samples<120 {
        let mut ticket=deadline;
        let raw=unsafe{qy_notifications_sample(rules.as_ptr(),active,(&mut ticket as *mut Instant).cast())};
        samples+=1;
        let value=if raw.is_null(){None}else{
            let result=serde_json::from_slice::<serde_json::Value>(unsafe{CStr::from_ptr(raw)}.to_bytes()).ok();
            unsafe{qy_notifications_free(raw)};result
        };
        if let Some(value)=value {
            captured|=value["preview_ready"].as_bool().unwrap_or(false);
            accessible|=value["preview_accessible"].as_bool().unwrap_or(false);
            // Construct an allowlisted output. Native diagnostics contain only
            // counts, exact selected bundle IDs, fixed parser/source labels,
            // structural AX roles and AX error numbers. No arbitrary AX text.
            println!("{}",serde_json::json!({"kind":"notification_preview_sample","sample":samples,
                "permission":value["permission"],"locked":value["locked"],"dock_ready":value["ready"],
                "dock_apps":value["items"].as_array().map_or(0,Vec::len),"preview_accessible":value["preview_accessible"],
                "preview_captured":value["preview_ready"],"diagnostics":value["diagnostics"]}));
        }else{println!("{}",serde_json::json!({"kind":"notification_preview_sample","sample":samples,"error":"native_sample_unavailable"}));}
        let remaining=deadline.saturating_duration_since(Instant::now());
        std::thread::sleep(Duration::from_millis(500).min(remaining));
    }
    unsafe{qy_notifications_clear()};
    println!("{}",serde_json::json!({"kind":"notification_preview_summary","samples":samples,"duration_seconds":seconds,"accessible":accessible,"captured":captured,"content_logged":false}));
    captured
}

#[cfg(test)]
mod tests {
    #[test]
    fn notification_access_diagnostics_identify_current_process_without_claiming_another_gui_grant(){
        let report=super::access_report();
        assert_eq!(report["pid"],std::process::id());
        assert_eq!(report["permission_scope"],"current_launch_context");
        assert_eq!(report["running_gui_authorization"],"not_checked");
        assert_eq!(report["responsible_process"],"not_exposed_by_public_api");
        assert!(report["accessibility_granted"].is_boolean());
        assert_eq!(report["signature_status"],0);assert_eq!(report["identity_status"],0);
        let executable=std::env::current_exe().unwrap();
        assert_eq!(std::path::Path::new(report["executable_path"].as_str().unwrap()).canonicalize().unwrap(),executable.canonicalize().unwrap());
        let signature=std::process::Command::new("/usr/bin/codesign").args(["-d","-r-"]).arg(&executable).output().unwrap();
        assert!(signature.status.success());
        let requirement=report["designated_requirement"].as_str().unwrap();
        let signature_text=format!("{}{}",String::from_utf8_lossy(&signature.stdout),String::from_utf8_lossy(&signature.stderr));
        assert!(!requirement.is_empty());assert!(signature_text.contains(requirement),"native requirement {requirement:?} must match codesign: {signature_text}");
        assert!(report.get("sender").is_none()&&report.get("body").is_none());
    }
}
