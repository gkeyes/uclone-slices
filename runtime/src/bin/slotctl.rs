use std::env;
use std::io::{Read as _, Write as _};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

const DEFAULT_SOCKET: &str = "/data/adb/uclone-slices-v2/runtime.sock";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = env::args();
    let _binary = arguments.next();
    if arguments.next().as_deref() != Some("rpc") || arguments.next().is_some() {
        return Err("usage: slotctl rpc".into());
    }
    let socket = env::var("UCLONE_RUNTIME_SOCKET")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_SOCKET));
    let mut request = String::new();
    std::io::stdin().read_to_string(&mut request)?;
    if !request.ends_with('\n') {
        request.push('\n');
    }
    let response = rpc(&socket, &request)?;
    std::io::stdout().write_all(response.as_bytes())?;
    Ok(())
}

fn rpc(socket: &std::path::Path, request: &str) -> Result<String, std::io::Error> {
    let mut stream = UnixStream::connect(socket)?;
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
