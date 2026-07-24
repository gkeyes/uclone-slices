use std::env;
use std::fmt::Display;
use std::fs;
use std::io::{BufRead as _, BufReader, Write as _};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};

use uclone_slices_runtime::{handle_line, production};

const DEFAULT_ROOT: &str = "/data/adb/uclone-slices-v2";
const OPERATION_FAILED_RESPONSE: &str = "{\"error\":{\"code\":\"operation_failed\"}}\n";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root =
        PathBuf::from(env::var("UCLONE_RUNTIME_ROOT").unwrap_or_else(|_| DEFAULT_ROOT.to_owned()));
    let socket = env::var("UCLONE_RUNTIME_SOCKET")
        .map(PathBuf::from)
        .unwrap_or_else(|_| root.join("runtime.sock"));
    let build_id = env::var("UCLONE_BUILD_ID")?;
    fs::create_dir_all(&root)?;
    remove_stale_socket(&socket)?;
    let listener = UnixListener::bind(&socket)?;
    let mut runtime = None;
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                if let Err(error) = serve_one(stream, |line| {
                    with_runtime(
                        &mut runtime,
                        || production(&root, build_id.clone()),
                        |runtime| handle_line(runtime, line),
                    )
                }) {
                    eprintln!("ucloned request failed: {error}");
                }
            }
            Err(error) => eprintln!("ucloned accept failed: {error}"),
        }
    }
    Ok(())
}

fn with_runtime<T, E>(
    runtime: &mut Option<T>,
    create: impl FnOnce() -> Result<T, E>,
    handle: impl FnOnce(&mut T) -> String,
) -> String
where
    E: Display,
{
    if runtime.is_none() {
        match create() {
            Ok(created) => *runtime = Some(created),
            Err(error) => {
                eprintln!("ucloned Runtime unavailable: {error}");
                return OPERATION_FAILED_RESPONSE.to_owned();
            }
        }
    }
    let Some(runtime) = runtime.as_mut() else {
        return OPERATION_FAILED_RESPONSE.to_owned();
    };
    handle(runtime)
}

fn serve_one(
    mut stream: UnixStream,
    handler: impl FnOnce(&str) -> String,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut line = String::new();
    BufReader::new(stream.try_clone()?).read_line(&mut line)?;
    let response = handler(line.trim_end());
    stream.write_all(response.as_bytes())?;
    Ok(())
}

fn remove_stale_socket(path: &Path) -> Result<(), std::io::Error> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use std::net::Shutdown;

    #[test]
    fn one_connection_carries_one_request() {
        let (mut client, server) = UnixStream::pair().unwrap();
        client.write_all(b"{\"op\":\"future\"}\n").unwrap();
        client.shutdown(Shutdown::Write).unwrap();

        let mut received = String::new();
        serve_one(server, |line| {
            received = line.to_owned();
            "{\"error\":{\"code\":\"invalid_request\"}}\n".to_owned()
        })
        .unwrap();

        let mut response = String::new();
        std::io::Read::read_to_string(&mut client, &mut response).unwrap();
        assert_eq!(received, "{\"op\":\"future\"}");
        assert_eq!(response, "{\"error\":{\"code\":\"invalid_request\"}}\n");
    }

    #[test]
    fn runtime_composition_failure_is_retried_on_the_next_request() {
        let mut runtime = None;

        let unavailable = with_runtime(
            &mut runtime,
            || Err::<u8, _>("CE storage is unavailable"),
            |_runtime| "unexpected".to_owned(),
        );
        let available = with_runtime(
            &mut runtime,
            || Ok::<_, &str>(7),
            |runtime| format!("ok:{runtime}"),
        );

        assert_eq!(unavailable, OPERATION_FAILED_RESPONSE);
        assert_eq!(available, "ok:7");
        assert_eq!(runtime, Some(7));
    }
}
