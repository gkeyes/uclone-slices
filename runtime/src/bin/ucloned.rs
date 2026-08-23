#[path = "ucloned/boot.rs"]
mod boot;
#[path = "ucloned/rpc.rs"]
mod rpc;

use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use boot::{BOOT_ID_PATH, prepare_runtime_root, reconcile_once_per_boot, wait_for_user0_ready};
use rpc::{OPERATION_FAILED_RESPONSE, RequestHandler, serialize_transaction, spawn_connection};
use uclone_slices_runtime::{handle_line, production};

const DEFAULT_ROOT: &str = "/data/adb/uclone-slices-v2";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root =
        PathBuf::from(env::var("UCLONE_RUNTIME_ROOT").unwrap_or_else(|_| DEFAULT_ROOT.to_owned()));
    let socket = env::var("UCLONE_RUNTIME_SOCKET")
        .map(PathBuf::from)
        .unwrap_or_else(|_| root.join("runtime.sock"));
    let build_id = env::var("UCLONE_BUILD_ID")?;
    prepare_runtime_root(&root)?;
    wait_for_user0_ready();
    let boot_id = fs::read_to_string(BOOT_ID_PATH)?;
    let mut boot_runtime = production(&root, build_id.clone())?;
    reconcile_once_per_boot(&root, boot_id.trim(), || boot_runtime.reconcile_boot())?;
    drop(boot_runtime);
    remove_stale_socket(&socket)?;
    let listener = UnixListener::bind(&socket)?;
    fs::set_permissions(&socket, fs::Permissions::from_mode(0o600))?;
    let transaction_lock = Arc::new(Mutex::new(()));
    let handler_root = root;
    let handler_build_id = build_id;
    let handler: Arc<RequestHandler> = Arc::new(move |line| {
        serialize_transaction(&transaction_lock, || {
            match production(&handler_root, handler_build_id.clone()) {
                Ok(mut runtime) => handle_line(&mut runtime, &handler_build_id, line),
                Err(error) => {
                    eprintln!("ucloned Runtime unavailable: {error}");
                    OPERATION_FAILED_RESPONSE.to_owned()
                }
            }
        })
    });
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => drop(spawn_connection(stream, Arc::clone(&handler))),
            Err(error) => eprintln!("ucloned accept failed: {error}"),
        }
    }
    Ok(())
}

fn remove_stale_socket(path: &Path) -> Result<(), std::io::Error> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}
