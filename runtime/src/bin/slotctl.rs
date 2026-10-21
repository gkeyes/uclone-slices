use std::env;
use std::io::{Read as _, Write as _};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

use uclone_slices_runtime::launch::connect_control;
use uclone_slices_runtime::{BINARY_BUILD_ID, layout::RUNTIME_SOCKET};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = env::args();
    let _binary = arguments.next();
    let operation = arguments.next();
    if !matches!(operation.as_deref(), Some("rpc" | "gate" | "check-runtime"))
        || arguments.next().is_some()
    {
        return Err("usage: slotctl rpc|gate|check-runtime".into());
    }
    let socket = env::var("UCLONE_RUNTIME_SOCKET")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(RUNTIME_SOCKET));
    if operation.as_deref() == Some("check-runtime") {
        let expected =
            env::var("UCLONE_EXPECTED_BUILD_ID").unwrap_or_else(|_| BINARY_BUILD_ID.to_owned());
        let response = rpc(&socket, "{\"op\":\"probe\"}\n")?;
        if expected != BINARY_BUILD_ID || !runtime_identity_valid(&response, &expected) {
            return Err("runtime executable identity mismatch".into());
        }
        return Ok(());
    }
    let mut request = String::new();
    std::io::stdin().read_to_string(&mut request)?;
    if !request.ends_with('\n') {
        request.push('\n');
    }
    let response = if operation.as_deref() == Some("gate") {
        let stream = connect_control()?;
        exchange(stream, &request)?
    } else {
        rpc(&socket, &request)?
    };
    std::io::stdout().write_all(response.as_bytes())?;
    Ok(())
}

fn runtime_identity_valid(frame: &str, expected: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(frame).is_ok_and(|response| {
        response["ok"]["build_id"].as_str() == Some(expected)
            && response["ok"]["binary_build_id"].as_str() == Some(expected)
    })
}

fn rpc(socket: &std::path::Path, request: &str) -> Result<String, std::io::Error> {
    let stream = UnixStream::connect(socket)?;
    exchange(stream, request)
}

fn exchange(mut stream: UnixStream, request: &str) -> Result<String, std::io::Error> {
    stream.write_all(request.as_bytes())?;
    stream.shutdown(Shutdown::Write)?;
    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    Ok(response)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use std::os::unix::net::UnixListener;
    use std::thread;

    #[test]
    fn transport_identity_ignores_readiness_but_rejects_missing_or_old_executables() {
        assert!(runtime_identity_valid(
            r#"{"ok":{"build_id":"current","binary_build_id":"current","launch_gate_ready":false,"process_view_ready":false}}"#,
            "current"
        ));
        assert!(!runtime_identity_valid(
            r#"{"ok":{"build_id":"current"}}"#,
            "current"
        ));
        assert!(!runtime_identity_valid(
            r#"{"ok":{"build_id":"current","binary_build_id":"old"}}"#,
            "current"
        ));
    }

    #[test]
    fn rpc_forwards_one_frame_without_an_envelope() {
        let root = tempfile::tempdir().unwrap();
        let socket = root.path().join("runtime.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _address) = listener.accept().unwrap();
            let mut request = String::new();
            stream.read_to_string(&mut request).unwrap();
            assert_eq!(request, "{\"op\":\"probe\"}\n");
            stream
                .write_all(b"{\"ok\":{\"build_id\":\"test\"}}\n")
                .unwrap();
        });

        let response = rpc(&socket, "{\"op\":\"probe\"}\n").unwrap();
        server.join().unwrap();

        assert_eq!(response, "{\"ok\":{\"build_id\":\"test\"}}\n");
    }
}
