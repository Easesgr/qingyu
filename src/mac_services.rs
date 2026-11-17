//! Small macOS services used by portable modules. No network or process work runs on the UI frame.
use std::ffi::{c_char, c_void, CStr, CString};
use std::path::{Path, PathBuf};

#[repr(C)]
#[derive(Default)]
struct NativeClipboard {
    change: std::ffi::c_long,
    kind: u32,
    data: *mut u8,
    len: usize,
    extension: *mut c_char,
}

use crate::clipboard::maccy::{self, Pasteboard, ReadError, ReadRequest};
pub(crate) use maccy::Content as ClipboardContent;
pub(crate) use maccy::ReplayPaths as ReplayPath;

unsafe extern "C" {
    fn qy_services_free(value: *mut c_void);
    fn qy_services_clipboard_metadata(name: *const c_char, include_types: bool, result: *mut NativeClipboard);
    fn qy_services_clipboard_read(name: *const c_char, expected: std::ffi::c_long, index: usize, kind: *const c_char,
        limit: usize, property_list: bool, result: *mut NativeClipboard);
    fn qy_services_clipboard_write(name: *const c_char, bytes: *const u8, len: usize) -> bool;
    fn qy_services_local_media_file(path: *const c_char) -> bool;
    fn qy_services_metrics(result: *mut Metrics) -> bool;
    fn qy_services_notify(title: *const c_char, body: *const c_char, sound: bool);
    fn qy_services_direct_interface() -> u32;
    fn qy_services_direct_connect(address: *const c_char, port: u16, interface: u32, timeout_ms: u32) -> i32;
}

unsafe fn owned_string(value: *mut c_char) -> Option<String> {
    if value.is_null() { return None; }
    let result = CStr::from_ptr(value).to_string_lossy().into_owned(); qy_services_free(value.cast()); Some(result)
}
unsafe fn owned_bytes(value: NativeClipboard) -> Vec<u8> {
    let bytes = if value.data.is_null() { Vec::new() } else {
        let bytes = std::slice::from_raw_parts(value.data, value.len).to_vec(); qy_services_free(value.data.cast()); bytes
    };
    if !value.extension.is_null() { qy_services_free(value.extension.cast()); }
    bytes
}

struct NativePasteboard(*const c_char);
impl Pasteboard for NativePasteboard {
    fn change_count(&self) -> i64 {
        let mut result = NativeClipboard::default();
        unsafe { qy_services_clipboard_metadata(self.0, false, &mut result); }
        result.change as i64
    }
    fn metadata(&self) -> Result<maccy::Metadata, ReadError> {
        let mut result = NativeClipboard::default();
        unsafe { qy_services_clipboard_metadata(self.0, true, &mut result); }
        let status = result.kind; let bytes = unsafe { owned_bytes(result) };
        match status { 1 => serde_json::from_slice(&bytes).map_err(|_| ReadError::Unavailable), 2 => Err(ReadError::Changed), _ => Err(ReadError::Unavailable) }
    }
    fn read(&self, request: ReadRequest<'_>) -> Result<Option<Vec<u8>>, ReadError> {
        let kind = CString::new(request.kind).map_err(|_| ReadError::Unavailable)?;
        let mut result = NativeClipboard::default();
        unsafe { qy_services_clipboard_read(self.0, request.change as std::ffi::c_long, request.item.unwrap_or(usize::MAX), kind.as_ptr(),
            request.limit, request.property_list, &mut result); }
        let status = result.kind; let bytes = unsafe { owned_bytes(result) };
        match status { 1 => Ok(Some(bytes)), 2 => Err(ReadError::Changed), 3 => Err(ReadError::TooLarge), _ => Ok(None) }
    }
}

fn clipboard_on(name: *const c_char, previous: &mut std::ffi::c_long, capture: bool) -> Option<ClipboardContent> {
    let mut change = *previous as i64;
    let content = maccy::poll(&NativePasteboard(name), &mut change, capture);
    *previous = change as std::ffi::c_long; content
}
/// Rust selects allowed representations before any low-level native data reads.
pub(crate) fn clipboard(previous: &mut std::ffi::c_long, capture: bool) -> Option<ClipboardContent> { clipboard_on(std::ptr::null(), previous, capture) }
fn write_plan_on(name: *const c_char, plan: &maccy::WritePlan) -> bool {
    serde_json::to_vec(plan).is_ok_and(|bytes| unsafe { qy_services_clipboard_write(name, bytes.as_ptr(), bytes.len()) })
}
fn copy_history_on(name: *const c_char, text: &str) -> bool { write_plan_on(name, &maccy::text_plan(text)) }
pub fn copy_history(text: &str) -> bool { copy_history_on(std::ptr::null(), text) }

pub(crate) fn local_media_file(path: &Path) -> bool {
    CString::new(path.as_os_str().as_encoded_bytes()).is_ok_and(|path| unsafe { qy_services_local_media_file(path.as_ptr()) })
}
fn valid_replay_file(path: &Path, limit: u64) -> Option<u64> {
    if !local_media_file(path) { return None; }
    std::fs::symlink_metadata(path).ok().filter(|file| file.is_file() && file.len() > 0 && file.len() <= limit).map(|file| file.len())
}
fn copy_media_on(name: *const c_char, path: &Path, image: bool) -> bool {
    if image {
        if valid_replay_file(path, crate::clipboard::MAX_IMAGE_BYTES).is_none() { return false; }
        maccy::image_plan(path).is_some_and(|plan| write_plan_on(name, &plan))
    } else { copy_files_on(name, &[path.into()]) }
}
pub(crate) fn copy_media(path: &Path, image: bool) -> bool { copy_media_on(std::ptr::null(), path, image) }
fn copy_files_on(name: *const c_char, paths: &[PathBuf]) -> bool {
    let Some(plan) = maccy::files_plan(paths) else { return false; };
    let mut total = 0_u64;
    for path in paths {
        let Some(bytes) = valid_replay_file(path, crate::clipboard::MAX_VIDEO_BYTES) else { return false; };
        total += bytes;
        if total > crate::clipboard::MAX_MEDIA_CACHE_BYTES { return false; }
    }
    write_plan_on(name, &plan)
}
pub(crate) fn copy_files(paths: &[PathBuf]) -> bool { copy_files_on(std::ptr::null(), paths) }
fn replay_path_on(name: *const c_char) -> ReplayPath { maccy::replay_paths(&NativePasteboard(name)) }
pub(crate) fn clipboard_replay_path() -> ReplayPath { replay_path_on(std::ptr::null()) }

#[repr(C)]
#[derive(Default)]
struct Metrics { physical: u64, resident: u64, threads: u32, descriptors: u32 }

pub(crate) fn resources() -> Option<crate::resources::Snapshot> {
    let mut metrics = Metrics::default();
    unsafe { qy_services_metrics(&mut metrics) }.then_some(crate::resources::Snapshot {
        private_bytes: metrics.physical as usize, working_bytes: metrics.resident as usize,
        threads: metrics.threads, handles: metrics.descriptors,
    })
}

pub fn notify(title: &str, body: &str, sound: bool) {
    if let (Ok(title), Ok(body)) = (CString::new(title), CString::new(body)) {
        unsafe { qy_services_notify(title.as_ptr(), body.as_ptr(), sound); }
    }
}

/// Only weather uses these sockets. IP_BOUND_IF keeps even TUN's more specific
/// routes from moving a connection onto a VPN interface.
pub(crate) fn direct_connect(address: std::net::SocketAddr, timeout: Option<std::time::Duration>) -> std::io::Result<std::net::TcpStream> {
    use std::{io, net::TcpStream, time::Duration};
    let timeout = timeout.unwrap_or(Duration::from_secs(8));
    if timeout.is_zero() {return Err(io::Error::new(io::ErrorKind::TimedOut, "weather connection timed out"));}
    // Local fixtures/services never need a physical interface, including when offline.
    if address.ip().is_loopback() {return TcpStream::connect_timeout(&address, timeout);}
    let interface = unsafe {qy_services_direct_interface()};
    if interface == 0 {return Err(io::Error::new(io::ErrorKind::NotConnected, "no physical IPv4 uplink for weather"));}
    bound_connect(address, interface, timeout)
}

fn bound_connect(address: std::net::SocketAddr, interface: u32, timeout: std::time::Duration) -> std::io::Result<std::net::TcpStream> {
    use std::{io, net::TcpStream, os::fd::FromRawFd};
    let ip = CString::new(address.ip().to_string()).map_err(|_|io::Error::new(io::ErrorKind::InvalidInput, "invalid IP address"))?;
    let millis = timeout.as_millis().clamp(1, i32::MAX as u128) as u32;
    let socket = unsafe {qy_services_direct_connect(ip.as_ptr(), address.port(), interface, millis)};
    if socket < 0 {return Err(io::Error::last_os_error());}
    // The native side returns an owned, blocking socket with CLOEXEC/NOSIGPIPE.
    Ok(unsafe {TcpStream::from_raw_fd(socket)})
}

#[cfg(test)]
mod network_tests {
    #[test]
    fn weather_bound_socket_does_not_retry_without_interface() {
        use std::{net::TcpListener, time::Duration};
        let listener=TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        for interface in [0, u32::MAX] {
            assert!(super::bound_connect(listener.local_addr().unwrap(),interface,Duration::from_millis(100)).is_err());
            assert_eq!(listener.accept().unwrap_err().kind(),std::io::ErrorKind::WouldBlock,
                "an invalid physical interface must not fall back to the default VPN route");
        }
    }
}

#[cfg(test)]
pub(crate) mod clipboard_fixtures {
    use super::*;
    unsafe extern "C" {
        fn qy_services_test_pasteboard() -> *mut c_char;
        fn qy_services_test_pasteboard_release(name: *const c_char);
        fn qy_services_test_pasteboard_write(name: *const c_char, kind: u32, bytes: *const u8, len: usize, marker: *const c_char) -> bool;
        fn qy_services_test_pasteboard_data(name: *const c_char, kind: *const c_char, len: *mut usize) -> *mut u8;
        fn qy_services_test_pasteboard_items(name: *const c_char, bytes: *const u8, len: usize) -> bool;
        fn qy_services_test_pasteboard_reads(name: *const c_char) -> u64;
    }
    pub(crate) struct Board(CString);
    impl Board {
        pub(crate) fn new() -> Self {
            let name = unsafe { owned_string(qy_services_test_pasteboard()) }.expect("private NSPasteboard fixture");
            Self(CString::new(name).unwrap())
        }
        pub(crate) fn write(&self, kind: u32, data: &[u8], marker: Option<&str>) {
            let marker = marker.map(|m| CString::new(m).unwrap());
            assert!(unsafe { qy_services_test_pasteboard_write(self.0.as_ptr(), kind, data.as_ptr(), data.len(), marker.as_ref().map_or(std::ptr::null(), |m| m.as_ptr())) });
        }
        pub(crate) fn snapshot(&self, previous: &mut std::ffi::c_long, capture: bool) -> Option<ClipboardContent> {
            clipboard_on(self.0.as_ptr(), previous, capture)
        }
        pub(crate) fn copy_text(&self, text: &str) -> bool {
            copy_history_on(self.0.as_ptr(), text)
        }
        pub(crate) fn copy_media(&self, path: &Path, image: bool) -> bool {
            copy_media_on(self.0.as_ptr(), path, image)
        }
        pub(crate) fn replay_path(&self) -> Option<PathBuf> {
            self.replay_paths().into_iter().next()
        }
        pub(crate) fn replay_paths(&self) -> Vec<PathBuf> {
            match replay_path_on(self.0.as_ptr()) { ReplayPath::Active(paths) => paths, ReplayPath::Empty => Vec::new(), ReplayPath::Unstable => panic!("fixture changed unexpectedly") }
        }
        pub(crate) fn copy_files(&self, paths: &[PathBuf]) -> bool { copy_files_on(self.0.as_ptr(), paths) }
        pub(crate) fn write_items(&self, descriptions: serde_json::Value) {
            let bytes = serde_json::to_vec(&descriptions).unwrap();
            assert!(unsafe { qy_services_test_pasteboard_items(self.0.as_ptr(), bytes.as_ptr(), bytes.len()) });
        }
        pub(crate) fn reads(&self) -> u64 { unsafe { qy_services_test_pasteboard_reads(self.0.as_ptr()) } }
        pub(crate) fn values(&self, kind: &str) -> Vec<String> {
            let board = NativePasteboard(self.0.as_ptr()); let metadata = board.metadata().unwrap();
            metadata.items.iter().enumerate().filter_map(|(index, _)| {
                board.read(ReadRequest { change: metadata.change, item: Some(index), kind, limit: 524_288, property_list: false })
                    .ok().flatten().and_then(|data| String::from_utf8(data).ok())
            }).collect()
        }
        pub(crate) fn data(&self, kind: &str) -> Vec<u8> {
            let kind = CString::new(kind).unwrap();
            let mut len = 0;
            unsafe {
                let data = qy_services_test_pasteboard_data(self.0.as_ptr(), kind.as_ptr(), &mut len);
                if data.is_null() { return Vec::new(); }
                let bytes = std::slice::from_raw_parts(data, len).to_vec();
                qy_services_free(data.cast());
                bytes
            }
        }
    }
    impl Drop for Board { fn drop(&mut self) { unsafe { qy_services_test_pasteboard_release(self.0.as_ptr()); } } }
}

#[cfg(test)]
mod clipboard_tests {
    use super::{clipboard_fixtures::Board, maccy, ClipboardContent};

    #[test]
    fn private_pasteboard_prefers_file_urls_then_images_then_text() {
        let board = Board::new();
        let mut change = -1;
        let files = vec!["/tmp/one.png", "/tmp/two.mov"];
        board.write(3, &serde_json::to_vec(&files).unwrap(), None);
        assert_eq!(board.snapshot(&mut change, true), Some(ClipboardContent::Files(files.iter().map(Into::into).collect())));
        for (kind, extension) in [(2, "png"), (7, "tiff")] {
            board.write(kind, b"raw image fixture", None);
            assert_eq!(board.snapshot(&mut change, true), Some(ClipboardContent::Image { data: b"raw image fixture".to_vec(), extension: extension.into() }));
            assert_eq!(board.snapshot(&mut change, true), None);
        }
        board.write(1, "本机文字".as_bytes(), None);
        assert_eq!(board.snapshot(&mut change, true), Some(ClipboardContent::Text("本机文字".into())));
    }

    #[test]
    fn private_pasteboard_privacy_and_history_markers_cover_all_payloads() {
        let board = Board::new();
        let mut change = -1;
        for marker in maccy::IGNORED.iter().copied().chain([maccy::HISTORY]) {
            for (kind, data) in [(1, b"secret".as_slice()), (2, b"private image".as_slice()), (7, b"private TIFF".as_slice()),
                    (3, b"[\"/tmp/a.png\",\"/tmp/b.mov\"]".as_slice())] {
                board.write(kind, data, Some(marker));
                let before = change;
                assert_eq!(board.snapshot(&mut change, true), None, "{marker}, kind={kind}");
                assert_ne!(before, change, "stable excluded updates must advance the counter");
            }
        }
        assert!(board.copy_text("replayed text"));
        assert_eq!(board.snapshot(&mut change, true), None);
    }

    #[test]
    fn private_pasteboard_disabling_skips_lazy_reads_and_races_retry() {
        let board = Board::new();
        board.write(6, b"old image bytes", None);
        let mut baseline = -1;
        assert_eq!(board.snapshot(&mut baseline, false), None);
        let mut change = -1;
        assert_eq!(board.snapshot(&mut change, true), None, "bytes read during a clipboard mutation must be discarded");
        assert_eq!(change, -1, "a racing snapshot must not advance to a change it did not read");
        assert_eq!(board.snapshot(&mut change, true), Some(ClipboardContent::Text("changed during capture".into())));
        assert_ne!(change, baseline, "disabled capture must not have asked the data provider for bytes");
    }

    #[test]
    fn maccy_private_pasteboard_ignores_unsafe_large_lazy_representations() {
        let board = Board::new(); let mut change = -1;
        board.write_items(serde_json::json!([{"text":"safe text", "lazy":["dyn.ah62d4qmxhk4d425try1g44pdsm11g55gsu1e82xnqzv",
            "com.microsoft.ole.source.bookmark", "com.apple.webarchive", "com.microsoft.Link-Source", "com.microsoft.ObjectLink", "com.adobe.pdf"], "large":true}]));
        assert_eq!(board.snapshot(&mut change, true), Some(ClipboardContent::Text("safe text".into())));
        assert_eq!(board.reads(), 0, "discarded representations must never ask their providers to allocate/read bytes");
        board.write_items(serde_json::json!([{"lazy":["com.apple.AnnotationKit.AnnotationItem"],"large":true}]));
        assert!(board.snapshot(&mut change, true).is_none()); assert_eq!(board.reads(), 0);
        board.write_items(serde_json::json!([{"text":"safe first item"}, {"lazy":["public.png"],"large":true,"marker":"net.antelle.keeweb"}]));
        assert!(board.snapshot(&mut change, true).is_none()); assert_eq!(board.reads(), 0);
    }

    #[test]
    fn maccy_private_pasteboard_merges_items_and_falls_back_to_explicit_plain_text() {
        let board = Board::new(); let mut change = -1;
        board.write_items(serde_json::json!([{"text":"first", "lazy":["public.html","public.rtf"],"large":true},
            {"text":" \n "}, {"text":"second"}, {"lazy":["public.html"],"large":true}]));
        assert_eq!(board.snapshot(&mut change, true), Some(ClipboardContent::Text("first\nsecond".into())));
        assert_eq!(board.reads(), 0);
        board.write_items(serde_json::json!([{"text":"missing file data", "empty":["public.file-url"]}]));
        assert_eq!(board.snapshot(&mut change, true), Some(ClipboardContent::Text("missing file data".into())));
    }

    #[test]
    fn maccy_private_pasteboard_consumes_oversized_selected_copy_once() {
        let board = Board::new(); let mut change = -1;
        board.write_items(serde_json::json!([{"lazy":["public.png"],"large":true}]));
        assert!(board.snapshot(&mut change, true).is_none()); assert_ne!(change, -1);
        assert_eq!(board.reads(), 1);
        assert!(board.snapshot(&mut change, true).is_none()); assert_eq!(board.reads(), 1);
    }
}
